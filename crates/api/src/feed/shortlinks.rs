use super::*;
use crate::storage;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Definition {
	version: u8,
	q: String,
	pool: String,
	t: Option<ListingTime>,
	rank: String,
	limit: u8,
	include_nsfw: bool,
}

impl Request {
	pub fn definition(&self) -> Result<Definition, Diagnostic> {
		if self.q.len() > 4096 || self.rank().len() > 512 {
			return Err(Diagnostic::new("invalid_value", "Feed QL limit is 4096 bytes; rank limit is 512 bytes", (0, 0)));
		}
		if !(1..=100).contains(&self.limit()) {
			return Err(Diagnostic::new("invalid_value", "Feed page size must be between 1 and 100", (0, 0)));
		}
		let expression = parse(&self.q, Mode::Posts)?;
		let pool = CandidatePool::parse(self, expression.span)?;
		plan(&expression)?;
		Ranking::resolve(self)?;
		Ok(Definition {
			version: 1,
			q: self.q.trim().into(),
			pool: pool.as_str().into(),
			t: (pool == CandidatePool::Top).then(|| self.time()),
			rank: expand_rank_preset(self.rank()).into(),
			limit: self.limit(),
			include_nsfw: self.include_nsfw,
		})
	}

	/// One serializer for self-contained share URLs and pagination.
	pub fn url(&self, base: &str, continuation: Option<(u32, &str)>) -> String {
		let mut query = url::form_urlencoded::Serializer::new(String::new());
		if !base.starts_with("/f/") {
			query.append_pair("q", self.q.trim());
			if self.pool() != "new" {
				query.append_pair("pool", self.pool());
			}
			if self.pool() == "top" {
				query.append_pair("t", self.time().as_str());
			}
			query.append_pair("rank", expand_rank_preset(self.rank()));
			if self.limit() != 25 {
				query.append_pair("limit", &self.limit().to_string());
			}
			if self.include_nsfw {
				query.append_pair("include_nsfw", "true");
			}
		}
		if let Some((page, cursor)) = continuation {
			query.append_pair("page", &page.to_string()).append_pair("cursor", cursor);
		}
		let query = query.finish();
		if query.is_empty() {
			base.into()
		} else {
			format!("{base}?{query}")
		}
	}
}

impl Definition {
	pub(super) fn canonical(&self) -> String {
		serde_json::to_string(self).expect("feed definition has only JSON-safe fields")
	}

	pub fn request(&self) -> Request {
		Request {
			q: self.q.clone(),
			pool: Some(self.pool.clone()),
			t: self.t,
			rank: Some(self.rank.clone()),
			limit: Some(self.limit),
			include_nsfw: self.include_nsfw,
			..Default::default()
		}
	}
}

#[derive(Default, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(default, deny_unknown_fields)]
pub struct Continuation {
	pub page: Option<u32>,
	pub cursor: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SavedFeed {
	pub id: String,
	pub url: String,
}

impl RedditService {
	pub fn feed_url(&self, id: &str) -> Option<String> {
		self.shortlinks.as_ref().map(|store| store.url(id))
	}

	pub async fn save_feed(&self, request: &Request) -> Result<SavedFeed, Diagnostic> {
		let store = self.shortlinks.as_ref().ok_or_else(disabled)?;
		if request.page.is_some() || request.cursor.is_some() {
			return Err(Diagnostic::new("invalid_value", "Save a definition without page or cursor controls", (0, 0)));
		}
		if request.include_nsfw && !self.allows_nsfw() {
			return Err(Diagnostic::new("content_blocked", "NSFW content is disabled by server policy", (0, 0)));
		}
		let definition = request.definition()?;
		let id = store.save(definition.canonical()).await.map_err(storage_error)?;
		Ok(SavedFeed { url: store.url(&id), id })
	}

	pub async fn saved_feed(&self, id: &str, continuation: Continuation) -> Result<Request, Diagnostic> {
		let store = self.shortlinks.as_ref().ok_or_else(disabled)?;
		let data = store
			.get(id)
			.await
			.map_err(storage_error)?
			.ok_or_else(|| Diagnostic::new("not_found", "Feed shortlink not found", (0, 0)))?;
		let definition: Definition = serde_json::from_str(&data).map_err(|_| unavailable())?;
		if definition.version != 1 {
			return Err(unavailable());
		}
		let mut request = definition.request();
		request.page = continuation.page;
		request.cursor = continuation.cursor;
		Ok(request)
	}
}

fn disabled() -> Diagnostic {
	Diagnostic::new("not_found", "Feed shortlinks are disabled", (0, 0))
}
fn unavailable() -> Diagnostic {
	Diagnostic::new("storage_unavailable", "Feed storage is unavailable", (0, 0))
}
fn storage_error(error: storage::Error) -> Diagnostic {
	match error {
		storage::Error::RateLimited => Diagnostic::new("rate_limited", "Feed creation rate exceeded; retry in one minute", (0, 0)),
		storage::Error::Capacity => Diagnostic::new("storage_full", "Feed storage capacity reached", (0, 0)),
		error => {
			log::error!("feed storage: {error}");
			unavailable()
		}
	}
}

pub fn status(error: &Diagnostic) -> axum::http::StatusCode {
	use axum::http::StatusCode;
	match error.code {
		"not_found" => StatusCode::NOT_FOUND,
		"rate_limited" => StatusCode::TOO_MANY_REQUESTS,
		"storage_full" | "storage_unavailable" | "execution_busy" => StatusCode::SERVICE_UNAVAILABLE,
		"invalid_cursor" => StatusCode::GONE,
		"source_failed" => StatusCode::BAD_GATEWAY,
		"execution_timeout" => StatusCode::GATEWAY_TIMEOUT,
		_ => StatusCode::BAD_REQUEST,
	}
}

