use super::AuthError;
use serde::Deserialize;
use serde_json::json;
use std::{collections::HashMap, time::Duration};
use tegen::tegen::TextGenerator;
use tokio::time::{sleep, timeout, Instant};
use tracing::{debug, trace, warn};
use wreq::Client as WreqClient;

const MOBILE_AUTH_ENDPOINT: &str = "https://www.reddit.com/auth/v2/oauth/access-token/loid";
const MOBILE_AUTHORIZATION: &str = "Basic b2hYcG9xclpZdWIxa2c6";
const WEB_AUTH_ENDPOINT: &str = "https://www.reddit.com/api/v1/access_token";
const WEB_AUTHORIZATION: &str = "Basic M1hmQkpXbGlIdnFBQ25YcmZJWWxMdzo=";
const TOKEN_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const ATTEMPTS_PER_BACKEND: u8 = 5;

// These recent version/build pairs came from Redlib's old generated list.
// They remain static because scraping third-party download sites was brittle.
const ANDROID_APP_VERSIONS: [&str; 9] = [
	"Version 2024.40.0/Build 1928580",
	"Version 2024.41.0/Build 1941199",
	"Version 2024.41.1/Build 1947805",
	"Version 2024.42.0/Build 1952440",
	"Version 2024.43.0/Build 1972250",
	"Version 2024.44.0/Build 1988458",
	"Version 2024.45.0/Build 2001943",
	"Version 2024.46.0/Build 2012731",
	"Version 2024.47.0/Build 2029755",
];

pub(crate) struct Credentials {
	headers: HashMap<String, String>,
	user_agent: String,
	expires_at: Instant,
}

impl Credentials {
	pub(super) async fn authenticate(http: &WreqClient) -> Result<Self, AuthError> {
		let mobile = Backend::Mobile(MobileAuth::new());
		match Self::authenticate_with(http, mobile).await {
			Ok(credentials) => return Ok(credentials),
			Err(error) => warn!(event = "oauth.backend_fallback", backend = "mobile", error = %error, "OAuth authentication failed; falling back to web authentication"),
		}

		Self::authenticate_with(http, Backend::Web(WebAuth::new())).await
	}

	async fn authenticate_with(http: &WreqClient, mut backend: Backend) -> Result<Self, AuthError> {
		let mut last_error = AuthError::Unavailable;
		for attempt in 1..=ATTEMPTS_PER_BACKEND {
			let exchange = timeout(TOKEN_REQUEST_TIMEOUT, backend.authenticate(http)).await;
			match exchange {
				Ok(Ok(token)) => {
					let mut headers = backend.headers();
					headers.insert("Authorization".to_owned(), format!("Bearer {}", token.access_token));
					let expires_at = Instant::now().checked_add(Duration::from_secs(token.expires_in)).ok_or(AuthError::InvalidExpiry)?;
					debug!(
						event = "oauth.credentials_created",
						backend = backend.name(),
						expires_in_seconds = token.expires_in,
						"OAuth credentials created"
					);
					return Ok(Self {
						headers,
						user_agent: backend.user_agent().to_owned(),
						expires_at,
					});
				}
				Ok(Err(error)) => last_error = error,
				Err(_) => last_error = AuthError::Timeout,
			}

			warn!(event = "oauth.attempt_failed", backend = backend.name(), attempt, max_attempts = ATTEMPTS_PER_BACKEND, error = %last_error, "OAuth attempt failed");
			if attempt < ATTEMPTS_PER_BACKEND {
				sleep(TOKEN_REQUEST_TIMEOUT).await;
			}
		}
		Err(last_error)
	}

	pub(crate) fn headers(&self) -> &HashMap<String, String> {
		&self.headers
	}

	pub(crate) fn user_agent(&self) -> &str {
		&self.user_agent
	}

	pub(super) fn expires_at(&self) -> Instant {
		self.expires_at
	}

}

#[derive(Deserialize)]
struct TokenResponse {
	access_token: Option<String>,
	expires_in: Option<u64>,
}

impl TokenResponse {
	fn required(self) -> Result<Token, AuthError> {
		Ok(Token {
			access_token: self.access_token.ok_or(AuthError::MissingField("access_token"))?,
			expires_in: self.expires_in.ok_or(AuthError::MissingField("expires_in"))?,
		})
	}
}

struct Token {
	access_token: String,
	expires_in: u64,
}

enum Backend {
	Mobile(MobileAuth),
	Web(WebAuth),
}

impl Backend {
	async fn authenticate(&mut self, http: &WreqClient) -> Result<Token, AuthError> {
		match self {
			Self::Mobile(backend) => backend.authenticate(http).await,
			Self::Web(backend) => backend.authenticate(http).await,
		}
	}

	fn name(&self) -> &'static str {
		match self {
			Self::Mobile(_) => "mobile",
			Self::Web(_) => "web",
		}
	}

	fn user_agent(&self) -> &str {
		match self {
			Self::Mobile(backend) => backend.user_agent(),
			Self::Web(backend) => backend.user_agent(),
		}
	}

	fn headers(&self) -> HashMap<String, String> {
		match self {
			Self::Mobile(backend) => backend.headers(),
			Self::Web(backend) => backend.headers(),
		}
	}
}

struct MobileAuth {
	device: AndroidDevice,
	response_headers: HashMap<String, String>,
}

impl MobileAuth {
	fn new() -> Self {
		Self {
			device: AndroidDevice::new(),
			response_headers: HashMap::new(),
		}
	}

	async fn authenticate(&mut self, http: &WreqClient) -> Result<Token, AuthError> {
		let mut request = http.post(MOBILE_AUTH_ENDPOINT);
		for (name, value) in &self.device.headers {
			request = request.header(name, value);
		}

		request = request.header("Authorization", MOBILE_AUTHORIZATION);

		trace!(backend = "mobile", "sending OAuth token request");
		let response = request.json(&json!({ "scopes": ["*", "email", "pii"] })).send().await?;
		self.response_headers.clear();
		capture_response_headers(response.headers(), &mut self.response_headers)?;
		decode_token(response).await
	}

	fn user_agent(&self) -> &str {
		&self.device.user_agent
	}

	fn headers(&self) -> HashMap<String, String> {
		let mut headers = self.device.headers.clone();
		headers.extend(self.response_headers.clone());
		headers
	}
}

struct WebAuth {
	device_id: String,
	user_agent: String,
	response_headers: HashMap<String, String>,
}

impl WebAuth {
	fn new() -> Self {
		let characters = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
		let device_id = (0..20).map(|_| characters[fastrand::usize(..characters.len())] as char).collect();
		Self {
			device_id,
			user_agent: fake_user_agent::get_rua().to_owned(),
			response_headers: HashMap::new(),
		}
	}

	async fn authenticate(&mut self, http: &WreqClient) -> Result<Token, AuthError> {
		let request = http
			.post(WEB_AUTH_ENDPOINT)
			.header("Host", "www.reddit.com")
			.header("User-Agent", &self.user_agent)
			.header("Accept", "*/*")
			.header("Accept-Language", "en-US,en;q=0.5")
			.header("Authorization", WEB_AUTHORIZATION)
			.header("Content-Type", "application/x-www-form-urlencoded")
			.header("Sec-GPC", "1")
			.header("Connection", "keep-alive");
		let body = format!("grant_type=https%3A%2F%2Foauth.reddit.com%2Fgrants%2Finstalled_client&device_id={}", self.device_id);

		trace!(backend = "web", "sending OAuth token request");
		let response = request.body(body).send().await?;
		self.response_headers.clear();
		capture_response_headers(response.headers(), &mut self.response_headers)?;
		decode_token(response).await
	}

	fn user_agent(&self) -> &str {
		&self.user_agent
	}

	fn headers(&self) -> HashMap<String, String> {
		let mut headers = self.response_headers.clone();
		headers.insert("Origin".to_owned(), "https://www.reddit.com".to_owned());
		headers.insert("User-Agent".to_owned(), self.user_agent.clone());
		headers
	}
}

async fn decode_token(response: wreq::Response) -> Result<Token, AuthError> {
	let status = response.status();
	if !status.is_success() {
		return Err(AuthError::Rejected(status.as_u16()));
	}
	response.json::<TokenResponse>().await?.required()
}

fn capture_response_headers(headers: &wreq::header::HeaderMap, destination: &mut HashMap<String, String>) -> Result<(), AuthError> {
	for name in ["x-reddit-loid", "x-reddit-session"] {
		let Some(value) = headers.get(name) else {
			continue;
		};
		let value = value.to_str().map_err(|_| AuthError::InvalidHeader(name))?;
		destination.insert(name.to_owned(), value.to_owned());
	}
	Ok(())
}

struct AndroidDevice {
	headers: HashMap<String, String>,
	user_agent: String,
}

impl AndroidDevice {
	fn new() -> Self {
		let uuid = uuid::Uuid::new_v4().to_string();
		let app_version = ANDROID_APP_VERSIONS[fastrand::usize(..ANDROID_APP_VERSIONS.len())];
		let android_version = fastrand::u8(9..=14);
		let user_agent = format!("Reddit/{app_version}/Android {android_version}");
		let qos = format!("{:.3}", fastrand::u32(1000..=100_000) as f32 / 1000.0);
		let codecs = TextGenerator::new().generate("available-codecs=video/avc, video/hevc{, video/x-vnd.on2.vp9|}");
		let headers = HashMap::from([
			("User-Agent".to_owned(), user_agent.clone()),
			("x-reddit-retry".to_owned(), "algo=no-retries".to_owned()),
			("x-reddit-compression".to_owned(), "1".to_owned()),
			("x-reddit-qos".to_owned(), qos),
			("x-reddit-media-codecs".to_owned(), codecs),
			("Content-Type".to_owned(), "application/json; charset=UTF-8".to_owned()),
			("client-vendor-id".to_owned(), uuid.clone()),
			("X-Reddit-Device-Id".to_owned(), uuid),
		]);

		debug!(backend = "mobile", "OAuth identity created");
		Self { headers, user_agent }
	}
}
