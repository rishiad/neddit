#![forbid(unsafe_code)]

mod app;
mod error;
mod search;
mod view;

use axum::{
	extract::FromRef,
	http::{header, HeaderValue, StatusCode},
	middleware::{self, Next},
	response::{IntoResponse, Response},
	routing::get,
	Router,
};
use neddit_api::{media::MediaSigner, server::MediaProxy, service::RedditService};
use tower_http::{compression::CompressionLayer, trace::TraceLayer};
#[cfg(debug_assertions)]
use tower_livereload::LiveReloadLayer;

#[cfg(debug_assertions)]
const CONTENT_SECURITY_POLICY: &str =
	"default-src 'none'; style-src 'self'; script-src 'self' 'unsafe-inline' https://cdn.jsdelivr.net; connect-src 'self' ws: wss:; img-src 'self'; media-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'";
#[cfg(not(debug_assertions))]
const CONTENT_SECURITY_POLICY: &str =
	"default-src 'none'; style-src 'self'; script-src 'self' https://cdn.jsdelivr.net; connect-src 'self'; img-src 'self'; media-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'";

#[derive(Clone)]
struct WebState {
	service: RedditService,
	signer: MediaSigner,
	media: MediaProxy,
}

impl FromRef<WebState> for RedditService {
	fn from_ref(state: &WebState) -> Self {
		state.service.clone()
	}
}

impl FromRef<WebState> for MediaSigner {
	fn from_ref(state: &WebState) -> Self {
		state.signer.clone()
	}
}

impl FromRef<WebState> for MediaProxy {
	fn from_ref(state: &WebState) -> Self {
		state.media.clone()
	}
}

pub fn router(service: RedditService, media: MediaProxy) -> Router {
	let state = WebState {
		service,
		signer: media.signer().clone(),
		media: media.clone(),
	};
	let app = Router::new()
		.route("/", get(app::front_page))
		.route("/search", get(search::page))
		.route("/more-comments", get(app::more_comments))
		.route("/video/player", get(app::video_player))
		.route("/gallery/{article}/{index}", get(app::gallery))
		.route("/post-content/{article}", get(app::post_content))
		.route("/r/{subreddit}", get(app::subreddit_feed))
		.route("/r/{subreddit}/wiki", get(app::wiki_root))
		.route("/r/{subreddit}/wiki/", get(app::wiki_root))
		.route("/r/{subreddit}/wiki/{*page}", get(app::wiki_page))
		.route("/comments/{article}", get(app::post_comments))
		.route("/comments/{article}/{slug}", get(app::post_permalink))
		.route("/comments/{article}/{slug}/", get(app::post_permalink))
		.route("/comments/{article}/{slug}/{comment}", get(app::post_comment_permalink))
		.route("/r/{subreddit}/comments/{article}", get(app::subreddit_post_comments))
		.route("/r/{subreddit}/comments/{article}/{slug}", get(app::subreddit_post_permalink))
		.route("/r/{subreddit}/comments/{article}/{slug}/", get(app::subreddit_post_permalink))
		.route("/r/{subreddit}/comments/{article}/{slug}/{comment}", get(app::subreddit_post_comment_permalink))
		.with_state(state)
		.merge(system_routes())
		.merge(neddit_api::server::with_middleware(neddit_api::server::router(media)));
	#[cfg(debug_assertions)]
	let app = app.layer(LiveReloadLayer::new());
	app
		.layer(CompressionLayer::new())
		.layer(TraceLayer::new_for_http())
		.layer(middleware::from_fn(security_headers))
}

fn system_routes() -> Router {
	Router::new()
		.route("/healthz", get(health))
		.route("/assets/app.css", get(stylesheet))
		.route("/assets/app.js", get(javascript))
		.route("/search/controls", get(search::controls))
}

async fn health() -> StatusCode {
	StatusCode::NO_CONTENT
}

async fn stylesheet() -> impl IntoResponse {
	(
		[(header::CONTENT_TYPE, "text/css; charset=utf-8"), (header::CACHE_CONTROL, "no-cache")],
		include_str!("../assets/app.css"),
	)
}

async fn javascript() -> impl IntoResponse {
	(
		[(header::CONTENT_TYPE, "text/javascript; charset=utf-8"), (header::CACHE_CONTROL, "no-cache")],
		include_str!("../assets/app.js"),
	)
}

async fn security_headers(request: axum::extract::Request, next: Next) -> Response {
	let mut response = next.run(request).await;
	let headers = response.headers_mut();
	headers.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(CONTENT_SECURITY_POLICY));
	headers.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
	headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
	response
}

