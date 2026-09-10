pub mod error;
mod oauth;

use crate::media::rewrite_reddit_navigation;
use axum::{body::Body, http::HeaderMap, response::Response};
use futures_lite::{future::Boxed, FutureExt};
use log::{error, info, trace};
use percent_encoding::{percent_encode, CONTROLS};
use serde_json::Value;
use std::{result::Result, time::Duration};
use url::form_urlencoded;
use wreq::redirect::Policy;
use wreq::{header as wreq_header, Client as WreqClient, EmulationFactory, Method, Response as WreqResponse};
use wreq_util::{Emulation, EmulationOS, EmulationOption};

use self::error::ClientError;
pub use self::oauth::{AuthError, OAuthHealth};
use self::oauth::{CredentialLease, OAuthHandle};

const REDDIT_URL_BASE: &str = "https://oauth.reddit.com";
const REDDIT_URL_BASE_HOST: &str = "oauth.reddit.com";
const MAX_JSON_BYTES: usize = 8 * 1024 * 1024;

const REDDIT_SHORT_URL_BASE: &str = "https://redd.it";
const REDDIT_SHORT_URL_BASE_HOST: &str = "redd.it";

const ALTERNATIVE_REDDIT_URL_BASE: &str = "https://www.reddit.com";
const ALTERNATIVE_REDDIT_URL_BASE_HOST: &str = "www.reddit.com";

const URL_PAIRS: [(&str, &str); 2] = [
	(ALTERNATIVE_REDDIT_URL_BASE, ALTERNATIVE_REDDIT_URL_BASE_HOST),
	(REDDIT_SHORT_URL_BASE, REDDIT_SHORT_URL_BASE_HOST),
];

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
pub enum Access {
	#[default]
	Standard,
	Quarantined,
}

impl Access {
	fn quarantine_cookie(self) -> &'static str {
		match self {
			Self::Standard => "",
			Self::Quarantined => "_options=%7B%22pref_quarantine_optin%22%3A%20true%2C%20%22pref_gated_sr_optin%22%3A%20true%7D",
		}
	}
}

#[derive(Clone)]
pub struct RedditClient {
	http: WreqClient,
	oauth: OAuthHandle,
}

struct UpstreamResponse {
	response: WreqResponse,
	lease: CredentialLease,
	rate_limit_reset: Option<String>,
}

impl RedditClient {
	pub async fn new() -> Result<Self, ClientError> {
		let http = Self::build_http_client()?;
		let oauth = OAuthHandle::start(http.clone()).await?;
		Ok(Self { http, oauth })
	}

	pub async fn shutdown(&self) {
		self.oauth.shutdown().await;
	}

	pub async fn oauth_health(&self) -> Result<OAuthHealth, ClientError> {
		Ok(self.oauth.health().await?)
	}

	/// Build the HTTP client used for Reddit and media requests.
	fn build_http_client() -> Result<WreqClient, ClientError> {
		// Keeping this list short to aid in privacy.
		// The more emulations, the more unique a fingerprint each instance has.
		// But some emulations should increase evasiveness.
		let emulation = [Emulation::Chrome145, Emulation::Firefox147];
		let emulation_os = [EmulationOS::Android, EmulationOS::Windows];

		let rand = fastrand::usize(..);
		let emulation = EmulationOption::builder()
			.emulation(emulation[rand % emulation.len()])
			.emulation_os(emulation_os[rand % emulation_os.len()])
			.build()
			.emulation();

		info!("Building Wreq client with random emulation {:?}", emulation);
		WreqClient::builder()
			.emulation(emulation)
			.redirect(Policy::none())
			.build()
			.map_err(ClientError::BuildClient)
	}

	/// Gets the canonical path for a resource on Reddit. This is accomplished by
	/// making a `HEAD` request to Reddit at the path given in `path`.
	///
	/// This function returns `Ok(Some(path))`, where `path`'s value is identical
	/// to that of the value of the argument `path`, if Reddit responds to our
	/// `HEAD` request with a 2xx-family HTTP code. It will also return an
	/// `Ok(Some(String))` if Reddit responds to our `HEAD` request with a
	/// `Location` header in the response, and the HTTP code is in the 3xx-family;
	/// the `String` will contain the path as reported in `Location`. The return
	/// value is `Ok(None)` if Reddit responded with another 3xx status.
	#[async_recursion::async_recursion]
	pub async fn canonical_path(&self, path: String, tries: u8) -> Result<Option<String>, ClientError> {
		if tries == 0 {
			return Ok(None);
		}

		// for each URL pair, try the HEAD request
		let res = {
			let mut res = None;
			for (url_base, url_base_host) in URL_PAIRS {
				res = self.reddit_short_head(path.clone(), Access::Quarantined, url_base, url_base_host).await.ok();
				if let Some(res) = &res {
					if !res.response.status().is_client_error() {
						break;
					}
				}
			}
			res
		};

		let UpstreamResponse { response: res, .. } = res.ok_or_else(|| ClientError::HeadUnavailable { path: path.clone() })?;
		let status = res.status().as_u16();
		let policy_error = res.headers().get(wreq_header::RETRY_AFTER).is_some();

		match status {
			// If Reddit responds with a 2xx, then the path is already canonical.
			200..=299 => Ok(Some(path)),

			// If Reddit responds with a 301, then the path is redirected.
			301 => match res.headers().get(wreq_header::LOCATION) {
				Some(val) => {
					let original = val.to_str().map_err(|source| ClientError::InvalidLocationHeader { path: path.clone(), source })?;

					// We need to strip the .json suffix from the original path.
					// In addition, we want to remove share parameters.
					// Cut it off here instead of letting it propagate all the way
					// to main.rs
					let stripped_uri = original.strip_suffix(".json").unwrap_or(original).split('?').next().unwrap_or_default();

					// OAuth endpoints can return absolute Reddit URLs. Keep canonical
					// resolution inside this service rather than redirecting to Reddit.
					let uri = rewrite_reddit_navigation(stripped_uri).unwrap_or_else(|| stripped_uri.to_string());

					// Decrement tries and try again
					self.canonical_path(uri, tries - 1).await
				}
				None => Err(ClientError::MissingRedirectLocation { path }),
			},

			// If Reddit responds with anything other than 3xx (except for the 2xx and 301
			// as above), return a None.
			300..=399 => Ok(None),

			// Rate limiting
			429 => Err(ClientError::RateLimited { reset: None }),

			// Special condition rate limiting - https://github.com/redlib-org/redlib/issues/229
			403 if policy_error => Err(ClientError::RateLimited { reset: None }),

			_ => Ok(
				res
					.headers()
					.get(wreq_header::LOCATION)
					.map(|val| percent_encode(val.as_bytes(), CONTROLS).to_string().trim_start_matches(REDDIT_URL_BASE).to_string()),
			),
		}
	}

	pub async fn proxy(&self, url: String, request_headers: &HeaderMap) -> Result<Response, ClientError> {
		// First parameter is target URL (mandatory).
		let wreq_uri = wreq::Uri::try_from(&url).map_err(|_| ClientError::InvalidProxyUrl { url: url.clone() })?;

		let mut builder = self.http.get(wreq_uri);

		// Copy useful headers from original request
		for &key in &["Range", "If-Modified-Since", "Cache-Control"] {
			if let Some(value) = request_headers.get(key) {
				builder = builder.header(key, value.as_bytes());
			}
		}

		// Add User-Agent header of the currently spoofed device
		let credentials = self.oauth.current();
		builder = builder.header("User-Agent", credentials.user_agent());

		// This is needed or Reddit will redirect us to a /media landing page that just renders the image.
		builder = builder.header(wreq_header::ACCEPT, "*/*");

		let response = builder.send().await.map_err(|source| ClientError::Request { url, source })?;
		response.into_http_response()
	}

	/// Makes a GET request to Reddit at `path`. By default, this will honor HTTP
	/// 3xx codes Reddit returns and will automatically redirect.
	fn reddit_get(&self, path: String, access: Access) -> Boxed<Result<UpstreamResponse, ClientError>> {
		self.request(&Method::GET, with_raw_json(path), true, access, REDDIT_URL_BASE, REDDIT_URL_BASE_HOST)
	}

	/// Makes a HEAD request to Reddit at `path, using the short URL base. This will not follow redirects.
	fn reddit_short_head(&self, path: String, access: Access, base_path: &'static str, host: &'static str) -> Boxed<Result<UpstreamResponse, ClientError>> {
		self.request(&Method::HEAD, path, false, access, base_path, host)
	}

	/// Makes a request to Reddit. If `redirect` is `true`, `request_with_redirect`
	/// will recurse on the URL that Reddit provides in the Location HTTP header
	/// in its response.
	fn request(
		&self,
		method: &'static Method,
		path: String,
		redirect: bool,
		access: Access,
		base_path: &'static str,
		host: &'static str,
	) -> Boxed<Result<UpstreamResponse, ClientError>> {
		let url = format!("{base_path}{path}");
		let client = self.clone();

		async move {
			let lease = client.oauth.acquire().await?;
			let generation = lease.generation();
			let mut headers = Vec::with_capacity(lease.headers().len() + 2);
			headers.push(("Host", host));
			headers.push(("Cookie", access.quarantine_cookie()));
			headers.extend(lease.headers().iter().map(|(name, value)| (name.as_str(), value.as_str())));
			fastrand::shuffle(&mut headers);

			let mut builder = client.http.request(method.clone(), &url);
			for (key, value) in headers {
				builder = builder.header(key, value);
			}
			let result = builder.send().await;
			let response = result.map_err(|source| {
				crate::dbg_msg!("{method} {url}: {}", source);
				ClientError::Request { url: url.clone(), source }
			})?;
			let rate_limit_reset = client.observe_rate_limit_headers(generation, &response).await;
			if !response.status().is_redirection() || !redirect {
				return Ok(UpstreamResponse {
					response,
					lease,
					rate_limit_reset,
				});
			}

			let location = response
				.headers()
				.get(wreq::header::LOCATION)
				.ok_or_else(|| ClientError::MissingRedirectLocation { path: path.clone() })?;
			if location.to_str().ok() == Some(ALTERNATIVE_REDDIT_URL_BASE) {
				return Err(ClientError::InvalidRedirect { path });
			}
			let new_path = percent_encode(location.as_bytes(), CONTROLS)
				.to_string()
				.trim_start_matches(REDDIT_URL_BASE)
				.trim_start_matches(ALTERNATIVE_REDDIT_URL_BASE)
				.to_string();
			let new_path = with_raw_json(new_path);
			drop(response);
			drop(lease);
			client.request(method, new_path, true, access, base_path, host).await
		}
		.boxed()
	}

	/// Make a request to a Reddit API and parse the JSON response.
	pub async fn json(&self, path: String, access: Access) -> Result<Value, ClientError> {
		let UpstreamResponse {
			mut response,
			lease,
			rate_limit_reset,
		} = self.reddit_get(path.clone(), access).await?;
		let generation = lease.generation();
		let status = response.status();
		if response.content_length().is_some_and(|n| n > MAX_JSON_BYTES as u64) {
			return Err(ClientError::BodyTooLarge);
		}
		let mut body = Vec::new();
		while let Some(chunk) = response.chunk().await.map_err(|source| ClientError::Body { path: path.clone(), source })? {
			append_json_chunk(&mut body, &chunk)?;
		}
		drop(lease);

		if body.is_empty() {
			self.oauth.invalidate(generation).await;
			return Err(ClientError::RateLimited { reset: rate_limit_reset });
		}
		if status.as_u16() == 429 {
			self.oauth.invalidate(generation).await;
			return Err(ClientError::RateLimited { reset: rate_limit_reset });
		}
		if status.as_u16() == 401 {
			self.oauth.invalidate(generation).await;
			return Err(ClientError::Unauthorized);
		}

		let json: Value = serde_json::from_slice(&body).map_err(|source| {
			error!("Got an invalid response from Reddit {source}. Status code: {status}");
			if status.is_server_error() {
				ClientError::UpstreamUnavailable { status: status.as_u16(), source }
			} else {
				ClientError::Decode { path: path.clone(), source }
			}
		})?;

		if json["data"]["is_suspended"].as_bool() == Some(true) {
			return Err(ClientError::Suspended);
		}
		validate_json_envelope(status.as_u16(), &json)?;

		let Some(code) = json["error"].as_i64() else {
			return Ok(json);
		};
		let reason = json["reason"].as_str().unwrap_or_default();
		let message = json["message"].as_str().unwrap_or_default();
		if message == "Unauthorized" {
			error!("Requesting an OAuth refresh");
			self.oauth.invalidate(generation).await;
			return Err(ClientError::Unauthorized);
		}

		match reason {
			"quarantined" => Err(ClientError::Quarantined),
			"gated" => Err(ClientError::Gated),
			"private" => Err(ClientError::Private),
			"banned" => Err(ClientError::Banned),
			_ => Err(ClientError::Reddit {
				code,
				reason: reason.to_string(),
				message: message.to_string(),
				path,
			}),
		}
	}

	async fn observe_rate_limit_headers(&self, generation: u64, response: &WreqResponse) -> Option<String> {
		let remaining = response.headers().get("x-ratelimit-remaining")?.to_str().ok()?;
		let reset = response.headers().get("x-ratelimit-reset")?.to_str().ok()?;
		let used = response.headers().get("x-ratelimit-used")?.to_str().ok()?;
		trace!("Ratelimit remaining: {remaining}. Resets in {reset}. Used: {used}");

		if let Some(remaining) = parse_rate_limit_remaining(remaining) {
			let reset_after = reset
				.parse::<f64>()
				.ok()
				.filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
				.and_then(|seconds| Duration::try_from_secs_f64(seconds).ok());
			self.oauth.observe_rate_limit(generation, remaining, reset_after).await;
		}

		Some(reset.to_string())
	}
}

fn parse_rate_limit_remaining(value: &str) -> Option<u16> {
	let value = value.parse::<f64>().ok()?.floor();
	if !value.is_finite() || value < 0.0 {
		return None;
	}
	Some(value.min(f64::from(u16::MAX)) as u16)
}

fn append_json_chunk(body: &mut Vec<u8>, chunk: &[u8]) -> Result<(), ClientError> {
	if chunk.len() > MAX_JSON_BYTES.saturating_sub(body.len()) {
		return Err(ClientError::BodyTooLarge);
	}
	body.extend_from_slice(chunk);
	Ok(())
}

fn validate_json_envelope(status: u16, json: &Value) -> Result<(), ClientError> {
	let numeric_error = json.get("error").is_some_and(Value::is_i64);
	let error = json.get("error").is_some_and(|v| !v.is_null() && !v.is_i64());
	let errors = json.get("errors").is_some_and(|v| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty()));
	if (!(200..300).contains(&status) && !numeric_error) || error || errors {
		return Err(ClientError::InvalidEnvelope);
	}
	Ok(())
}

fn with_raw_json(path: String) -> String {
	let (base, query) = path.split_once('?').unwrap_or((&path, ""));
	let mut serializer = form_urlencoded::Serializer::new(String::new());
	for (key, value) in form_urlencoded::parse(query.as_bytes()) {
		if key != "raw_json" {
			serializer.append_pair(&key, &value);
		}
	}
	serializer.append_pair("raw_json", "1");
	format!("{base}?{}", serializer.finish())
}

trait IntoHttpResponse {
	fn into_http_response(self) -> Result<Response, ClientError>;
}

impl IntoHttpResponse for WreqResponse {
	fn into_http_response(self) -> Result<Response, ClientError> {
		let status = self.status();
		let version = self.version();

		let mut builder = Response::builder().status(status).version(version);

		for (name, value) in self.headers() {
			if is_safe_proxy_header(name.as_str()) {
				builder = builder.header(name, value);
			}
		}

		builder.body(Body::from_stream(self.bytes_stream())).map_err(ClientError::ResponseBuild)
	}
}

fn is_safe_proxy_header(name: &str) -> bool {
	matches!(
		name,
		"accept-ranges"
			| "cache-control"
			| "content-disposition"
			| "content-encoding"
			| "content-language"
			| "content-length"
			| "content-range"
			| "content-type"
			| "expires"
			| "last-modified"
			| "location"
	)
}

