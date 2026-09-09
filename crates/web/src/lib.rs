#![forbid(unsafe_code)]

mod app;
mod error;
mod view;

use axum::{
	http::{header, HeaderValue, StatusCode},
	middleware::{self, Next},
	response::{IntoResponse, Response},
	routing::get,
	Router,
};
use neddit_api::service::RedditService;
use tower_http::{compression::CompressionLayer, trace::TraceLayer};
#[cfg(debug_assertions)]
use tower_livereload::LiveReloadLayer;

#[cfg(debug_assertions)]
const CONTENT_SECURITY_POLICY: &str =
	"default-src 'none'; style-src 'self'; script-src 'self' 'unsafe-inline' https://cdn.jsdelivr.net; connect-src 'self' ws: wss:; base-uri 'none'; form-action 'self'; frame-ancestors 'none'";
#[cfg(not(debug_assertions))]
const CONTENT_SECURITY_POLICY: &str =
	"default-src 'none'; style-src 'self'; script-src https://cdn.jsdelivr.net; connect-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'";

pub fn router(service: RedditService) -> Router {
	let app = Router::new()
		.route("/", get(app::front_page))
		.route("/more-comments", get(app::more_comments))
		.route("/comments/{article}", get(app::post_comments))
		.route("/comments/{article}/{slug}", get(app::post_permalink))
		.route("/comments/{article}/{slug}/", get(app::post_permalink))
		.route("/comments/{article}/{slug}/{comment}", get(app::post_comment_permalink))
		.route("/r/{subreddit}/comments/{article}", get(app::subreddit_post_comments))
		.route("/r/{subreddit}/comments/{article}/{slug}", get(app::subreddit_post_permalink))
		.route("/r/{subreddit}/comments/{article}/{slug}/", get(app::subreddit_post_permalink))
		.route("/r/{subreddit}/comments/{article}/{slug}/{comment}", get(app::subreddit_post_comment_permalink))
		.with_state(service)
		.merge(system_routes());
	#[cfg(debug_assertions)]
	let app = app.layer(LiveReloadLayer::new());
	app
		.layer(CompressionLayer::new())
		.layer(TraceLayer::new_for_http())
		.layer(middleware::from_fn(security_headers))
}

fn system_routes() -> Router {
	Router::new().route("/healthz", get(health)).route("/assets/app.css", get(stylesheet))
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

async fn security_headers(request: axum::extract::Request, next: Next) -> Response {
	let mut response = next.run(request).await;
	let headers = response.headers_mut();
	headers.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(CONTENT_SECURITY_POLICY));
	headers.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
	headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
	response
}

