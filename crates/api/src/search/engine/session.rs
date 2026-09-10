use super::*;
use std::{
	collections::{HashSet, VecDeque},
	sync::Arc,
	time::{Duration, Instant},
};
use tokio::sync::Mutex;
use uuid::Uuid;

const MAX_PAGES: usize = 4;
pub(super) const MAX_SESSION_BYTES: usize = 2 * 1024 * 1024;
const TTL: Duration = Duration::from_secs(900);
const MAX_CANDIDATES: usize = 400;
const MAX_SOURCE_PAGES: usize = 16;
const MAX_SOURCE_CALLS: usize = 32;
const MAX_PARENT_BYTES: usize = 512 * 1024;

struct Entry {
	id: Uuid,
	binding: Request,
	created: Instant,
	state: Arc<Mutex<Collection>>,
}
pub struct Sessions {
	entries: Mutex<VecDeque<Entry>>,
	gate: tokio::sync::Semaphore,
}
impl Default for Sessions {
	fn default() -> Self {
		Self {
			entries: Mutex::new(VecDeque::new()),
			gate: tokio::sync::Semaphore::new(4),
		}
	}
}

#[derive(Clone, Copy)]
enum End {
	Exhausted,
	Stopped,
}

pub(super) fn comment_window(request: &Request, item: &PublicThing, frozen: i128) -> Truth {
	if request.kind != Mode::Comments || request.sort() != "top" {
		return Truth::True;
	}
	let seconds_back: i128 = match request.top_time() {
		ListingTime::Hour => 3600,
		ListingTime::Day => 86400,
		ListingTime::Week => 7 * 86400,
		ListingTime::Month => 30 * 86400,
		ListingTime::Year => 365 * 86400,
		ListingTime::All => return Truth::True,
	};
	match seconds(order_fields(item).0) {
		Some(created) if created >= frozen - seconds_back * 1_000_000_000 && created <= frozen => Truth::True,
		Some(_) => Truth::False,
		None => Truth::Unknown,
	}
}

fn visibility(item: &PublicThing, include_nsfw: bool) -> Truth {
	if include_nsfw {
		return Truth::True;
	}
	let adult = match item {
		PublicThing::Post(p) => Some(p.data.over_18),
		PublicThing::Subreddit(s) => s.data.over18,
		PublicThing::Comment(c) => c.data.extra.get("over_18").and_then(serde_json::Value::as_bool),
		PublicThing::User(_) => None,
	};
	match adult {
		Some(false) => Truth::True,
		Some(true) => Truth::False,
		None => Truth::Unknown,
	}
}
struct Collection {
	expr: Expr,
	plan: Plan,
	frozen: chrono::DateTime<chrono::Utc>,
	pages: Vec<Result<Page>>,
	ready: VecDeque<PublicThing>,
	tie: Vec<PublicThing>,
	last_time: Option<f64>,
	after: Option<String>,
	cursors: HashSet<String>,
	seen: HashSet<String>,
	parents: HashMap<String, Option<Record>>,
	parent_bytes: usize,
	source_pages: usize,
	source_calls: usize,
	candidates: usize,
	bytes: usize,
	observed: usize,
	unknown: usize,
	end: Option<End>,
	failed: bool,
	in_flight: bool,
	notices: Vec<String>,
}

fn invalid_cursor() -> Diagnostic {
	Diagnostic::new("invalid_cursor", "The cursor is malformed, expired, evicted, or belongs to another search", (0, 0))
}
fn timeout() -> Diagnostic {
	Diagnostic::new(
		"execution_timeout",
		"Search deadline exceeded (20 seconds); this continuation cannot perform more source requests",
		(0, 0),
	)
}

impl Sessions {
	pub(super) async fn execute(&self, source: &impl Source, request: &Request, expr: Expr, plan: Plan) -> Result<Page> {
		self
			.execute_before(source, request, expr, plan, tokio::time::Instant::now() + Duration::from_secs(20))
			.await
	}

	pub(super) async fn execute_before(&self, source: &impl Source, request: &Request, expr: Expr, plan: Plan, deadline: tokio::time::Instant) -> Result<Page> {
		if request.cursor.as_ref().is_some_and(|c| c.len() > 64) {
			return Err(invalid_cursor());
		}
		let binding = request.binding();
		let (id, index, created, state) = if let Some(cursor) = &request.cursor {
			let (id, index) = cursor.split_once('.').ok_or_else(invalid_cursor)?;
			let id = Uuid::parse_str(id).map_err(|_| invalid_cursor())?;
			let index = index.parse::<usize>().map_err(|_| invalid_cursor())?;
			if index == 0 {
				return Err(invalid_cursor());
			}
			let entries = self.entries.lock().await;
			let entry = entries
				.iter()
				.find(|e| e.id == id && e.binding == binding && e.created.elapsed() < TTL)
				.ok_or_else(invalid_cursor)?;
			(id, index, entry.created, Arc::clone(&entry.state))
		} else {
			(Uuid::new_v4(), 0, Instant::now(), Arc::new(Mutex::new(Collection::new(expr, plan))))
		};
		let mut collection = tokio::time::timeout_at(deadline, state.lock()).await.map_err(|_| timeout())?;
		if created.elapsed() >= TTL {
			return Err(invalid_cursor());
		}
		if let Some(page) = collection.pages.get(index) {
			return page.clone();
		}
		if index != collection.pages.len() || (index > 0 && !collection.pages.last().is_some_and(|p| p.as_ref().is_ok_and(|p| p.cursor.is_some()))) {
			return Err(invalid_cursor());
		}
		// A dropped request may leave an upstream operation partially consumed. Never restart it.
		let result = if collection.in_flight {
			Err(Diagnostic::new(
				"invalid_cursor",
				"The previous attempt was cancelled; this continuation is terminal",
				(0, 0),
			))
		} else {
			let needs_io = collection.end.is_none() && collection.ready.len() < usize::from(request.limit());
			let _permit = if needs_io {
				Some(
					self
						.gate
						.try_acquire()
						.map_err(|_| Diagnostic::new("execution_busy", "Concurrent search limit: 4", (0, 0)))?,
				)
			} else {
				None
			};
			collection.in_flight = true;
			tokio::time::timeout_at(deadline, collection.advance(source, request, id, index))
				.await
				.unwrap_or_else(|_| Err(timeout()))
		};
		collection.in_flight = false;
		if result.is_err() {
			collection.end = Some(End::Stopped);
			collection.ready.clear();
			collection.tie.clear();
		}
		collection.pages.push(result.clone());
		drop(collection);
		if request.cursor.is_none() && result.as_ref().is_ok_and(|p| p.cursor.is_some()) {
			let mut entries = self.entries.lock().await;
			entries.retain(|e| e.created.elapsed() < TTL);
			if entries.len() >= 32 {
				entries.pop_front();
			}
			entries.push_back(Entry { id, binding, created, state });
		}
		result
	}

}

impl Collection {
	fn new(expr: Expr, plan: Plan) -> Self {
		let mut notices = vec!["Discovery is bounded, not exhaustive Reddit coverage; counts are observed, not exact totals.".into()];
		if contradictory(&expr) {
			notices.push("Contradiction: one object cannot satisfy these distinct identity predicates.".into());
		}
		if matches!(plan, Plan::Posts(_) | Plan::Communities(_)) {
			notices.push("Reddit text and field filters may omit local matches; self-post URL hosts can differ from Reddit's site: classification.".into());
		}
		if matches!(plan, Plan::Comments(..)) {
			notices.push("Comment discovery covers only the available recent community or author history.".into());
		}
		Self {
			expr,
			plan,
			notices,
			frozen: std::time::SystemTime::now().into(),
			pages: Vec::new(),
			ready: VecDeque::new(),
			tie: Vec::new(),
			last_time: None,
			after: None,
			cursors: HashSet::new(),
			seen: HashSet::new(),
			parents: HashMap::new(),
			parent_bytes: 0,
			source_pages: 0,
			source_calls: 0,
			candidates: 0,
			bytes: 0,
			observed: 0,
			unknown: 0,
			end: None,
			failed: false,
			in_flight: false,
		}
	}
	fn stop(&mut self, notice: &str, failed: bool) {
		self.end = Some(End::Stopped);
		self.failed |= failed;
		if !self.notices.iter().any(|n| n == notice) {
			self.notices.push(notice.into());
		}
	}
	fn bounded(&mut self, top: bool) {
		if self.end.is_none()
			&& (self.source_pages >= if top { MAX_PAGES } else { MAX_SOURCE_PAGES } || self.candidates >= MAX_CANDIDATES || self.source_calls >= MAX_SOURCE_CALLS)
		{
			self.stop("Session discovery budget reached; uncollected candidates remain outside this search.", false);
		}
	}
	fn flush_tie(&mut self) {
		self.tie.sort_by(|a, b| identity(a).cmp(identity(b)));
		self.ready.extend(self.tie.drain(..));
	}
	async fn fetch(&mut self, source: &impl Source, request: &Request, limit: u8) -> Result<Vec<PublicThing>> {
		self.source_pages += 1;
		self.source_calls += 1;
		let response = source
			.page(
				&self.plan,
				request,
				ListingQuery {
					after: self.after.clone(),
					limit: Some(limit),
					..Default::default()
				},
			)
			.await;
		let response = match response {
			Ok(r) => r,
			Err(_) => {
				self.stop("The source failed after partial discovery; no more source requests will run for this session.", true);
				if self.seen.is_empty() {
					return Err(Diagnostic::new(
						"source_failed",
						"The Reddit source failed; this is not an empty successful search",
						self.expr.span,
					));
				}
				return Ok(Vec::new());
			}
		};
		if response.data.children.len() > 100 {
			return Err(Diagnostic::new("source_failed", "Source page exceeds the 100-candidate limit", self.expr.span));
		}
		if response.data.after.as_ref().is_some_and(|c| c.is_empty() || c.len() > 128) {
			return Err(Diagnostic::new("source_failed", "The source returned an empty or oversized continuation", self.expr.span));
		}
		let mut batch = Vec::new();
		for item in response.data.children {
			if !correct_kind(&item, request.kind) {
				return Err(Diagnostic::new("source_failed", "The source returned an unexpected object kind", self.expr.span));
			}
			if self.candidates == MAX_CANDIDATES {
				self.stop("Session candidate limit reached: 400.", false);
				break;
			}
			self.candidates += 1;
			self.bytes += serde_json::to_vec(&item).map_or(MAX_SESSION_BYTES + 1, |v| v.len());
			if self.bytes > MAX_SESSION_BYTES {
				self.stop("Candidate byte budget reached: 2097152.", false);
				break;
			}
			if self.seen.insert(identity(&item).to_owned()) {
				batch.push(item);
			}
		}
		if self.end.is_none() {
			self.after = response.data.after;
			if self.after.is_none() || matches!(self.plan, Plan::Names(_)) {
				self.end = Some(End::Exhausted);
			} else if !self.cursors.insert(self.after.clone().unwrap()) {
				self.stop("The source repeated a continuation cursor; execution stopped.", true);
			}
		}
		self.bounded(request.sort() == "top");
		Ok(batch)
	}
	fn evaluated_record(&self, item: &PublicThing) -> Record {
		let mut r = record(item);
		if let PublicThing::Comment(c) = item {
			if let Some(Some(parent)) = self.parents.get(&c.data.link_id) {
				for (from, to) in [(Field::Title, Field::PostTitle), (Field::Domain, Field::PostDomain), (Field::Flair, Field::PostFlair)] {
					r.fields.insert(to, parent.fields.get(&from).cloned().unwrap_or(Datum::Unknown));
				}
			}
		}
		r
	}
	fn frozen_ns(&self) -> i128 {
		i128::from(self.frozen.timestamp()) * 1_000_000_000 + i128::from(self.frozen.timestamp_subsec_nanos())
	}
	async fn hydrate(&mut self, source: &impl Source, batch: &[PublicThing], request: &Request) {
		if self.failed || !needs_parent(&self.expr) {
			return;
		}
		let mut ids: Vec<_> = batch
			.iter()
			.filter(|item| visibility(item, request.include_nsfw) == Truth::True)
			.filter(|item| comment_window(request, item, self.frozen_ns()) == Truth::True)
			.filter_map(|item| match item {
				PublicThing::Comment(c) if !self.parents.contains_key(&c.data.link_id) && self.expr.evaluate(&self.evaluated_record(item), self.frozen_ns()) == Truth::Unknown => {
					Some(c.data.link_id.clone())
				}
				_ => None,
			})
			.collect();
		ids.sort();
		ids.dedup();
		for ids in ids.chunks(100) {
			if self.source_calls >= MAX_SOURCE_CALLS {
				self.stop("Session source-call limit reached: 32; parent predicates remain unknown.", false);
				break;
			}
			self.source_calls += 1;
			let result = source.parents(ids).await;
			for id in ids {
				self.parents.insert(id.clone(), None);
			}
			match result {
				Ok(response) => {
					for item in response.data.children {
						if let PublicThing::Post(p) = item {
							if !ids.contains(&p.data.name) {
								continue;
							}
							let name = p.data.name.clone();
							let mut parent = record(&PublicThing::Post(p));
							parent.text.clear();
							parent.created = None;
							parent.fields.retain(|f, _| matches!(f, Field::Title | Field::Domain | Field::Flair));
							let bytes: usize = parent.fields.values().map(|v| if let Datum::Text(s) = v { s.len() } else { 0 }).sum();
							if self.parent_bytes + bytes > MAX_PARENT_BYTES {
								self.stop("Parent cache byte limit reached: 524288; remaining parent predicates stay unknown.", false);
								return;
							}
							self.parent_bytes += bytes;
							self.parents.insert(name, Some(parent));
						}
					}
				}
				Err(_) => {
					self.stop(
						"Parent metadata failed; unresolved predicates remain unknown and this session cannot fetch more data.",
						true,
					);
					break;
				}
			}
		}
	}
	async fn evaluate(&mut self, source: &impl Source, request: &Request, mut batch: Vec<PublicThing>) -> Result<()> {
		let new = request.sort() == "new";
		if new {
			if batch.iter().any(|p| seconds(order_fields(p).0).is_none()) {
				return Err(Diagnostic::new("source_failed", "New requires valid creation timestamps", self.expr.span));
			}
			batch.sort_by(|a, b| order_fields(b).0.total_cmp(&order_fields(a).0));
			if batch.first().is_some_and(|p| self.last_time.is_some_and(|t| order_fields(p).0 > t)) {
				self.stop(
					"Source chronology changed across pages; later candidates were excluded to preserve the published New order.",
					true,
				);
				self.flush_tie();
				return Ok(());
			}
		}
		self.hydrate(source, &batch, request).await;
		for item in batch {
			if new {
				let time = order_fields(&item).0;
				if self.last_time.is_some_and(|previous| time < previous) {
					self.flush_tie();
				}
				self.last_time = Some(time);
			}
			match self
				.expr
				.evaluate(&self.evaluated_record(&item), self.frozen_ns())
				.and(visibility(&item, request.include_nsfw))
				.and(comment_window(request, &item, self.frozen_ns()))
			{
				Truth::True => {
					self.observed += 1;
					if new {
						self.tie.push(item);
					} else {
						self.ready.push_back(item);
					}
				}
				Truth::Unknown => self.unknown += 1,
				Truth::False => {}
			}
			tokio::task::yield_now().await;
		}
		if self.end.is_some() {
			self.flush_tie();
		}
		Ok(())
	}
	async fn advance(&mut self, source: &impl Source, request: &Request, id: Uuid, index: usize) -> Result<Page> {
		let size = usize::from(request.limit());
		let top = request.sort() == "top";
		let complete_set = top || matches!(self.plan, Plan::Names(_));
		let mut pending = Vec::new();
		let mut requests = 0;
		while self.end.is_none() && (complete_set || self.ready.len() < size) && requests < MAX_PAGES {
			let limit = if complete_set {
				100
			} else {
				(size - self.ready.len() + usize::from(request.sort() == "new")).clamp(25, 100) as u8
			};
			let batch = self.fetch(source, request, limit).await?;
			requests += 1;
			if complete_set {
				pending.extend(batch);
			} else {
				self.evaluate(source, request, batch).await?;
			}
		}
		if complete_set && !pending.is_empty() {
			self.evaluate(source, request, pending).await?;
			let ready = self.ready.make_contiguous();
			ready.sort_by(|a, b| {
				if top {
					let (ac, ascore) = order_fields(a);
					let (bc, bscore) = order_fields(b);
					bscore.cmp(&ascore).then_with(|| bc.total_cmp(&ac)).then_with(|| identity(a).cmp(identity(b)))
				} else {
					identity(a).cmp(identity(b))
				}
			});
		}
		if self.end.is_some() {
			self.flush_tie();
		}
		let items: Vec<_> = self.ready.drain(..size.min(self.ready.len())).collect();
		let more = !self.ready.is_empty() || !self.tie.is_empty() || self.end.is_none();
		let continuation = if !self.ready.is_empty() {
			"buffered_results_remain"
		} else if self.end.is_none() {
			"more_results_may_exist"
		} else if matches!(self.end, Some(End::Exhausted)) {
			"selected_sources_exhausted"
		} else {
			"stopped_early"
		};
		let mut coverage = self.notices.clone();
		if request.kind == Mode::Comments && top && request.top_time() != ListingTime::All {
			coverage.push("Comment Top windows use frozen creation time within available history; month means 30 days and year means 365 days.".into());
		}
		if !request.include_nsfw {
			coverage.push("NSFW results are excluded. Candidates with missing or invalid NSFW metadata remain unconfirmed.".into());
		}
		if self.unknown > 0 {
			coverage.push(format!("{} candidates remain unresolved because required metadata is unavailable.", self.unknown));
		}
		if items.len() < size && self.end.is_none() {
			coverage.push("Per-response discovery budget reached; continue with the cursor, even if this page has no matches.".into());
		}
		let ranking = if top {
			coverage.push("Top orders only this bounded collection by observed score, creation time, and fullname; it is not global Reddit Top.".into());
			"neddit.collected-top".into()
		} else if request.sort() == "new" {
			coverage.push(
				"New checks chronology across source pages and sorts timestamp ties by fullname. Later source reordering stops discovery; this is not a Reddit snapshot.".into(),
			);
			"neddit.source-new".into()
		} else if matches!(self.plan, Plan::Names(_)) {
			"neddit.exact-name".into()
		} else {
			format!("reddit.{}", request.sort())
		};
		Ok(Page {
			items,
			cursor: more.then(|| format!("{id}.{}", index + 1)),
			ranking,
			continuation: continuation.into(),
			coverage,
			observed_matches: self.observed,
			frozen_at: self.frozen.to_rfc3339(),
			text_profile: TEXT_PROFILE,
		})
	}
}
