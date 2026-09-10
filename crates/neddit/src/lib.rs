#![forbid(unsafe_code)]

use axum::Router;
use neddit_api::{api, server, server::MediaProxy, service::RedditService};

pub fn router(service: RedditService, media: MediaProxy) -> Router {
	let signer = media.signer().clone();
	let api = api::with_json_aliases(api::with_reddit_urls(api::router(service.clone()), signer));
	let api = server::with_middleware(Router::new().fallback_service(api));
	neddit_web::router(service, media).fallback_service(api)
}
