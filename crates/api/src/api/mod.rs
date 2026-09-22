mod query;
mod response;
mod routes;
mod url_rewrite;

use crate::{media::MediaSigner, service::RedditService};
use axum::{
	extract::{rejection::QueryRejection, Query},
	http::StatusCode,
	response::{IntoResponse, Response},
	routing::{get, post},
	Json, Router,
};

type DiagnosticResponse = (StatusCode, Json<crate::search::Diagnostic>);

fn decode_query<T>(query: Result<Query<T>, QueryRejection>, message: &'static str) -> Result<T, DiagnosticResponse> {
	query.map(|Query(value)| value).map_err(|_| {
		diagnostic(crate::search::Diagnostic {
			code: "invalid_value",
			message: message.into(),
			start: 0,
			end: 0,
		})
	})
}

fn diagnostic(error: crate::search::Diagnostic) -> DiagnosticResponse {
	(error.status(), Json(error))
}

pub fn router(service: RedditService, custom_feeds_enabled: bool) -> Router {
	let mut router = Router::new().merge(routes::router()).route("/search/ql", get(ql_search));
	if custom_feeds_enabled {
		router = router
			.route("/feed", get(custom_feed))
			.route("/feeds/{id}", get(saved_feed))
			.route("/feeds", post(create_feed).layer(axum::extract::DefaultBodyLimit::max(16 * 1024)));
	}
	router.with_state(service)
}

async fn ql_search(axum::extract::State(service): axum::extract::State<RedditService>, query: Result<Query<crate::search::Request>, QueryRejection>) -> Response {
	let result: Result<Json<crate::search::Page>, DiagnosticResponse> = async {
		let request = decode_query(query, "Invalid or obsolete search request controls")?;
		service.search_ql(&request).await.map(Json).map_err(diagnostic)
	}
	.await;
	result.into_response()
}

async fn custom_feed(axum::extract::State(service): axum::extract::State<RedditService>, query: Result<Query<crate::feed::Request>, QueryRejection>) -> Response {
	let result: Result<Json<crate::feed::FeedPage>, DiagnosticResponse> = async {
		let request = decode_query(query, "Invalid or obsolete feed request controls")?;
		service.custom_feed(&request).await.map(Json).map_err(diagnostic)
	}
	.await;
	result.into_response()
}

async fn create_feed(
	axum::extract::State(service): axum::extract::State<RedditService>,
	axum::Json(request): axum::Json<crate::feed::Request>,
) -> Result<(StatusCode, Json<crate::feed::SavedFeed>), DiagnosticResponse> {
	service.save_feed(&request).await.map(|feed| (StatusCode::CREATED, Json(feed))).map_err(diagnostic)
}

async fn saved_feed(
	axum::extract::State(service): axum::extract::State<RedditService>,
	axum::extract::Path(id): axum::extract::Path<String>,
	axum::extract::Query(continuation): axum::extract::Query<crate::feed::Continuation>,
) -> Result<Json<crate::feed::FeedPage>, DiagnosticResponse> {
	let request = service.saved_feed(&id, continuation).await.map_err(diagnostic)?;
	service.custom_feed(&request).await.map(Json).map_err(diagnostic)
}

pub fn with_reddit_urls(router: Router, signer: MediaSigner) -> Router {
	url_rewrite::with_reddit_urls(router, signer)
}
