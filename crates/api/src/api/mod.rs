mod docs;
mod error;
mod public_query;
mod public_routes;
mod query;
mod response;
mod routes;
mod url_rewrite;

use crate::{media::MediaSigner, service::RedditService};
use axum::{extract::Request, http::uri::PathAndQuery, Router};
use tower::{util::MapRequest, ServiceExt};

pub fn router(service: RedditService) -> Router {
	let custom = Router::new()
		.route("/api/search/ql", axum::routing::get(ql_search))
		.route("/api/feed", axum::routing::get(custom_feed))
		.with_state(service.clone());
	docs::finish(docs::router().merge(routes::router()).merge(public_routes::router()), service)
		.merge(custom)
		
}

/// Execute validated QL against bounded REST sources. Cursors expire after 15 minutes or eviction.
#[utoipa::path(get, path = "/api/search/ql", params(crate::search::Request), responses(
	(status = 200, body = crate::search::Page),
	(status = 400, body = crate::search::Diagnostic),
	(status = 502, body = crate::search::Diagnostic),
	(status = 503, body = crate::search::Diagnostic),
	(status = 504, body = crate::search::Diagnostic)
))]
async fn ql_search(
	axum::extract::State(service): axum::extract::State<RedditService>,
	query: Result<axum::extract::Query<crate::search::Request>, axum::extract::rejection::QueryRejection>,
) -> axum::response::Response {
	use axum::response::IntoResponse;
	let result: Result<axum::Json<crate::search::Page>, (axum::http::StatusCode, axum::Json<crate::search::Diagnostic>)> = async {
		let axum::extract::Query(request) = query.map_err(|_| {
			(
				axum::http::StatusCode::BAD_REQUEST,
				axum::Json(crate::search::Diagnostic {
					code: "invalid_value",
					message: "Invalid or obsolete search request controls".into(),
					start: 0,
					end: 0,
				}),
			)
		})?;
		service.search_ql(&request).await.map(axum::Json).map_err(|e| {
			let status = match e.code {
				"source_failed" => axum::http::StatusCode::BAD_GATEWAY,
				"execution_timeout" => axum::http::StatusCode::GATEWAY_TIMEOUT,
				"execution_busy" => axum::http::StatusCode::SERVICE_UNAVAILABLE,
				_ => axum::http::StatusCode::BAD_REQUEST,
			};
			(status, axum::Json(e))
		})
	}
	.await;
	result.into_response()
}

/// Build a stateless post feed from native Reddit listings, local QL filters, and an optional ranking expression.
#[utoipa::path(get, path = "/api/feed", params(crate::feed::Request), responses(
	(status = 200, body = crate::feed::FeedPage),
	(status = 400, body = crate::search::Diagnostic),
	(status = 502, body = crate::search::Diagnostic),
	(status = 504, body = crate::search::Diagnostic)
))]
async fn custom_feed(
	axum::extract::State(service): axum::extract::State<RedditService>,
	query: Result<axum::extract::Query<crate::feed::Request>, axum::extract::rejection::QueryRejection>,
) -> axum::response::Response {
	use axum::response::IntoResponse;
	let result: Result<axum::Json<crate::feed::FeedPage>, (axum::http::StatusCode, axum::Json<crate::search::Diagnostic>)> = async {
		let axum::extract::Query(request) = query.map_err(|_| {
			(
				axum::http::StatusCode::BAD_REQUEST,
				axum::Json(crate::search::Diagnostic {
					code: "invalid_value",
					message: "Invalid or obsolete feed request controls".into(),
					start: 0,
					end: 0,
				}),
			)
		})?;
		service.custom_feed(&request).await.map(axum::Json).map_err(|error| {
			let status = match error.code {
				"source_failed" => axum::http::StatusCode::BAD_GATEWAY,
				"execution_timeout" => axum::http::StatusCode::GATEWAY_TIMEOUT,
				_ => axum::http::StatusCode::BAD_REQUEST,
			};
			(status, axum::Json(error))
		})
	}
	.await;
	result.into_response()
}

pub fn with_json_aliases(router: Router) -> MapRequest<Router, fn(Request) -> Request> {
	router.map_request(normalize_json_alias as fn(Request) -> Request)
}

pub fn with_reddit_urls(router: Router, signer: MediaSigner) -> Router {
	url_rewrite::with_reddit_urls(router, signer)
}

fn normalize_json_alias(mut request: Request) -> Request {
	let path = request.uri().path();
	if path == "/openapi.json" {
		return request;
	}
	let Some(path) = path.strip_suffix(".json") else {
		return request;
	};
	let path = if path.is_empty() { "/" } else { path };
	let path_and_query = match request.uri().query() {
		Some(query) => format!("{path}?{query}"),
		None => path.to_string(),
	};
	let Ok(path_and_query) = path_and_query.parse::<PathAndQuery>() else {
		return request;
	};
	let mut parts = request.uri().clone().into_parts();
	parts.path_and_query = Some(path_and_query);
	if let Ok(uri) = axum::http::Uri::from_parts(parts) {
		*request.uri_mut() = uri;
	}
	request
}

