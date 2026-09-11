use super::{backend::Credentials, AuthError};
use std::{
	future::Future,
	pin::Pin,
	sync::{
		atomic::{AtomicUsize, Ordering},
		Arc, Weak,
	},
	time::Duration,
};
use tokio::{
	sync::{mpsc, oneshot, watch},
	task::JoinSet,
	time::{sleep_until, Instant},
};
use tracing::{error, trace, warn};
use wreq::Client as WreqClient;

const INITIAL_RATE_LIMIT: u16 = 99;
const REFRESH_THRESHOLD: u16 = 10;
const REFRESH_MARGIN: Duration = Duration::from_secs(120);
const REFRESH_RETRY_DELAY: Duration = Duration::from_secs(5);

type AuthFuture = Pin<Box<dyn Future<Output = Result<Credentials, AuthError>> + Send>>;

trait Authenticator: Send + Sync {
	fn authenticate(&self, http: WreqClient) -> AuthFuture;
}

struct RedditAuthenticator;

impl Authenticator for RedditAuthenticator {
	fn authenticate(&self, http: WreqClient) -> AuthFuture {
		Box::pin(async move { Credentials::authenticate(&http).await })
	}
}

struct Generation {
	id: u64,
	credentials: Arc<Credentials>,
	in_flight: AtomicUsize,
}

impl Generation {
	fn new(id: u64, credentials: Credentials) -> Self {
		Self {
			id,
			credentials: Arc::new(credentials),
			in_flight: AtomicUsize::new(0),
		}
	}

	fn in_flight(&self) -> usize {
		self.in_flight.load(Ordering::Acquire)
	}
}

pub(crate) struct CredentialLease {
	generation: Arc<Generation>,
}

impl CredentialLease {
	fn new(generation: Arc<Generation>) -> Self {
		generation.in_flight.fetch_add(1, Ordering::Relaxed);
		Self { generation }
	}

	pub(crate) fn generation(&self) -> u64 {
		self.generation.id
	}

	pub(crate) fn headers(&self) -> &std::collections::HashMap<String, String> {
		self.generation.credentials.headers()
	}
}

impl Drop for CredentialLease {
	fn drop(&mut self) {
		let previous = self.generation.in_flight.fetch_sub(1, Ordering::Release);
		debug_assert!(previous > 0, "credential lease count underflowed");
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OAuthHealth {
	pub generation: u64,
	pub remaining: u16,
	pub active_requests: usize,
	pub retiring_generations: usize,
	pub retiring_requests: usize,
	pub refreshing: bool,
}

enum Command {
	Acquire { reply: oneshot::Sender<Result<CredentialLease, AuthError>> },
	ObserveRateLimit { generation: u64, remaining: u16, reset_after: Option<Duration> },
	Invalidate { generation: u64 },
	Health { reply: oneshot::Sender<OAuthHealth> },
	Shutdown { reply: oneshot::Sender<()> },
}

struct State {
	active: Arc<Generation>,
	retiring: Vec<Weak<Generation>>,
	next_generation: u64,
	remaining: u16,
	reset_at: Option<Instant>,
	next_refresh_at: Instant,
}

impl State {
	fn new(credentials: Credentials) -> Self {
		let next_refresh_at = refresh_deadline(credentials.expires_at());
		Self {
			active: Arc::new(Generation::new(0, credentials)),
			retiring: Vec::new(),
			next_generation: 1,
			remaining: INITIAL_RATE_LIMIT,
			reset_at: None,
			next_refresh_at,
		}
	}

	fn install(&mut self, credentials: Credentials) -> Arc<Credentials> {
		let generation = Arc::new(Generation::new(self.next_generation, credentials));
		self.next_generation = self.next_generation.checked_add(1).expect("OAuth generation counter overflowed");
		let previous = std::mem::replace(&mut self.active, generation);
		if previous.in_flight() > 0 {
			trace!(generation = previous.id, active_requests = previous.in_flight(), "retiring OAuth generation");
			self.retiring.push(Arc::downgrade(&previous));
		}

		self.next_refresh_at = refresh_deadline(self.active.credentials.expires_at());
		self.remaining = INITIAL_RATE_LIMIT;
		self.reset_at = None;
		self.reap_retired();
		self.active.credentials.clone()
	}

	fn observe_rate_limit(&mut self, generation: u64, remaining: u16, reset_after: Option<Duration>) {
		if generation != self.active.id {
			trace!(generation, "ignoring rate limit from retired OAuth generation");
			return;
		}

		let now = Instant::now();
		self.remaining = if self.reset_at.is_some_and(|deadline| deadline <= now) {
			remaining
		} else {
			self.remaining.min(remaining)
		};
		self.reset_at = reset_after.and_then(|duration| now.checked_add(duration));
	}

	fn invalidate(&mut self, generation: u64) -> bool {
		if generation != self.active.id {
			trace!(generation, "ignoring invalidation from retired OAuth generation");
			return false;
		}
		self.remaining = 0;
		true
	}

	fn reset_elapsed_rate_limit(&mut self) {
		if self.reset_at.is_some_and(|deadline| deadline <= Instant::now()) {
			self.remaining = INITIAL_RATE_LIMIT;
			self.reset_at = None;
		}
	}

	fn reap_retired(&mut self) {
		self.retiring.retain(|generation| generation.upgrade().is_some_and(|generation| generation.in_flight() > 0));
	}

	fn health(&mut self, refreshing: bool) -> OAuthHealth {
		self.reset_elapsed_rate_limit();
		self.reap_retired();
		let retiring_requests = self.retiring.iter().filter_map(Weak::upgrade).map(|generation| generation.in_flight()).sum();
		OAuthHealth {
			generation: self.active.id,
			remaining: self.remaining,
			active_requests: self.active.in_flight(),
			retiring_generations: self.retiring.len(),
			retiring_requests,
			refreshing,
		}
	}
}

#[derive(Clone)]
pub(crate) struct OAuthHandle {
	commands: mpsc::Sender<Command>,
	credentials: watch::Receiver<Arc<Credentials>>,
}

impl OAuthHandle {

	pub(crate) async fn start(http: WreqClient) -> Result<Self, AuthError> {
		Self::start_with_authenticator(http, Arc::new(RedditAuthenticator)).await
	}

	async fn start_with_authenticator(http: WreqClient, authenticator: Arc<dyn Authenticator>) -> Result<Self, AuthError> {
		let state = State::new(authenticator.authenticate(http.clone()).await?);
		let (credentials_tx, credentials_rx) = watch::channel(state.active.credentials.clone());
		let (commands_tx, commands_rx) = mpsc::channel(32);
		tokio::spawn(run_manager(http, authenticator, state, commands_rx, credentials_tx));

		Ok(Self {
			commands: commands_tx,
			credentials: credentials_rx,
		})
	}

	pub(crate) fn current(&self) -> Arc<Credentials> {
		self.credentials.borrow().clone()
	}

	pub(crate) async fn acquire(&self) -> Result<CredentialLease, AuthError> {
		let (reply, response) = oneshot::channel();
		self.commands.send(Command::Acquire { reply }).await.map_err(|_| AuthError::ManagerStopped)?;
		response.await.map_err(|_| AuthError::ManagerStopped)?
	}

	pub(crate) async fn observe_rate_limit(&self, generation: u64, remaining: u16, reset_after: Option<Duration>) {
		let _ = self
			.commands
			.send(Command::ObserveRateLimit {
				generation,
				remaining,
				reset_after,
			})
			.await;
	}

	pub(crate) async fn invalidate(&self, generation: u64) {
		let _ = self.commands.send(Command::Invalidate { generation }).await;
	}

	pub(crate) async fn health(&self) -> Result<OAuthHealth, AuthError> {
		let (reply, response) = oneshot::channel();
		self.commands.send(Command::Health { reply }).await.map_err(|_| AuthError::ManagerStopped)?;
		response.await.map_err(|_| AuthError::ManagerStopped)
	}

	pub(crate) async fn shutdown(&self) {
		let (reply, response) = oneshot::channel();
		if self.commands.send(Command::Shutdown { reply }).await.is_ok() {
			let _ = response.await;
		}
	}
}

async fn run_manager(
	http: WreqClient,
	authenticator: Arc<dyn Authenticator>,
	mut state: State,
	mut commands: mpsc::Receiver<Command>,
	credentials: watch::Sender<Arc<Credentials>>,
) {
	let mut refreshes = JoinSet::new();
	loop {
		tokio::select! {
			command = commands.recv() => match command {
				Some(Command::Acquire { reply }) => {
					state.reset_elapsed_rate_limit();
					state.reap_retired();
					if Instant::now() >= state.active.credentials.expires_at() {
						start_refresh(&http, &authenticator, &mut refreshes);
						let _ = reply.send(Err(AuthError::Expired));
						continue;
					}
					if state.remaining == 0 {
						start_refresh(&http, &authenticator, &mut refreshes);
						let _ = reply.send(Err(AuthError::RateLimited));
						continue;
					}

					state.remaining -= 1;
					if state.remaining < REFRESH_THRESHOLD {
						start_refresh(&http, &authenticator, &mut refreshes);
					}
					let _ = reply.send(Ok(CredentialLease::new(state.active.clone())));
				}
				Some(Command::ObserveRateLimit { generation, remaining, reset_after }) => {
					state.observe_rate_limit(generation, remaining, reset_after);
				}
				Some(Command::Invalidate { generation }) => {
					if state.invalidate(generation) {
						start_refresh(&http, &authenticator, &mut refreshes);
					}
				}
				Some(Command::Health { reply }) => {
					let _ = reply.send(state.health(!refreshes.is_empty()));
				}
				Some(Command::Shutdown { reply }) => {
					refreshes.shutdown().await;
					let _ = reply.send(());
					break;
				}
				None => {
					refreshes.shutdown().await;
					break;
				}
			},
			result = refreshes.join_next(), if !refreshes.is_empty() => match result {
				Some(Ok(Ok(new_credentials))) => {
					credentials.send_replace(state.install(new_credentials));
				}
				Some(Ok(Err(error))) => {
					warn!(event = "oauth.refresh_failed", error = %error, "OAuth refresh failed");
					state.next_refresh_at = Instant::now() + REFRESH_RETRY_DELAY;
				}
				Some(Err(error)) => {
					error!(event = "oauth.refresh_task_failed", error = %error, "OAuth refresh task failed");
					state.next_refresh_at = Instant::now() + REFRESH_RETRY_DELAY;
				}
				None => {}
			},
			_ = sleep_until(state.next_refresh_at), if refreshes.is_empty() => {
				start_refresh(&http, &authenticator, &mut refreshes);
			},
		}
	}
}

fn start_refresh(http: &WreqClient, authenticator: &Arc<dyn Authenticator>, refreshes: &mut JoinSet<Result<Credentials, AuthError>>) {
	if !refreshes.is_empty() {
		return;
	}

	let http = http.clone();
	let authenticator = authenticator.clone();
	refreshes.spawn(async move { authenticator.authenticate(http).await });
}

fn refresh_deadline(expires_at: Instant) -> Instant {
	expires_at.checked_sub(REFRESH_MARGIN).unwrap_or_else(Instant::now)
}

