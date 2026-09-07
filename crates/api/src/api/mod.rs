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
	docs::finish(docs::router().merge(routes::router()).merge(public_routes::router()), service)
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

