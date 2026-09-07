use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClientError {
	#[error("failed to build Reddit HTTP client")]
	BuildClient(#[source] wreq::Error),

	#[error(transparent)]
	Auth(#[from] super::oauth::AuthError),

	#[error("request to `{url}` failed")]
	Request {
		url: String,
		#[source]
		source: wreq::Error,
	},

	#[error("invalid proxy URL `{url}`")]
	InvalidProxyUrl { url: String },

	#[error("unable to make a HEAD request for `{path}`")]
	HeadUnavailable { path: String },

	#[error("Reddit returned an invalid redirect for `{path}`")]
	InvalidRedirect { path: String },

	#[error("Reddit returned a redirect without a Location header for `{path}`")]
	MissingRedirectLocation { path: String },

	#[error("Reddit returned an invalid Location header for `{path}`")]
	InvalidLocationHeader {
		path: String,
		#[source]
		source: wreq::header::ToStrError,
	},

	#[error("failed to receive the response body for `{path}`")]
	Body {
		path: String,
		#[source]
		source: wreq::Error,
	},

	#[error("failed to convert the upstream response")]
	ResponseBuild(#[source] axum::http::Error),

	#[error("failed to decode Reddit JSON for `{path}`")]
	Decode {
		path: String,
		#[source]
		source: serde_json::Error,
	},

	#[error("Reddit rate limit exceeded{reset_message}", reset_message = reset.as_ref().map(|value| format!("; resets in {value}")) .unwrap_or_default())]
	RateLimited { reset: Option<String> },

	#[error("Reddit OAuth token expired")]
	Unauthorized,

	#[error("Reddit account is suspended")]
	Suspended,

	#[error("subreddit is quarantined")]
	Quarantined,

	#[error("subreddit is gated")]
	Gated,

	#[error("subreddit is private")]
	Private,

	#[error("subreddit is banned")]
	Banned,

	#[error("Reddit returned error {code} `{reason}`: {message} for `{path}`")]
	Reddit { code: i64, reason: String, message: String, path: String },

	#[error("Reddit is unavailable (HTTP {status})")]
	UpstreamUnavailable {
		status: u16,
		#[source]
		source: serde_json::Error,
	},
}
