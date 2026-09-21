use crate::models::{CommentChild, Listing, More};
use crate::service::RedditService;
use axum::{routing::get, Json, Router};
use std::sync::Arc;
use utoipa::{openapi::OpenApi, OpenApi as OpenApiDerive};
use utoipa_axum::router::OpenApiRouter;

pub(super) const LISTINGS: &str = "See the [Reddit listing documentation](https://www.reddit.com/dev/api/#section_listings).";
pub(super) const COMMENTS: &str = "See the [Reddit comments documentation](https://www.reddit.com/dev/api/#GET_comments_{article}).";
pub(super) const MORE_CHILDREN: &str = "See the [Reddit more-children documentation](https://www.reddit.com/dev/api/#GET_api_morechildren).";
pub(super) const SUBREDDITS: &str = "See the [Reddit subreddit documentation](https://www.reddit.com/dev/api/#section_subreddits).";
pub(super) const SUBREDDIT_RULES: &str = "See the [Reddit subreddit-rules documentation](https://www.reddit.com/dev/api/#GET_r_{subreddit}_about_rules).";
pub(super) const SUBREDDIT_SIDEBAR: &str = "See the [Reddit sidebar documentation](https://www.reddit.com/dev/api/#GET_sidebar).";
pub(super) const TROPHIES: &str = "See the [Reddit trophy documentation](https://www.reddit.com/dev/api/#GET_api_v1_user_{username}_trophies).";
pub(super) const USERS: &str = "See the [Reddit user documentation](https://www.reddit.com/dev/api/#section_users).";
pub(super) const SEARCH: &str = "See the [Reddit search documentation](https://www.reddit.com/dev/api/#GET_search).";
pub(super) const INFO: &str = "See the [Reddit info documentation](https://www.reddit.com/dev/api/#GET_api_info).";
pub(super) const USER_HISTORY: &str = "See the [Reddit user history documentation](https://www.reddit.com/dev/api/#GET_user_{username}_{where}).";
pub(super) const USER_DIRECTORIES: &str = "See the [Reddit user directory documentation](https://www.reddit.com/dev/api/#GET_users_{where}).";
pub(super) const WIKI: &str = "See the [Reddit wiki documentation](https://www.reddit.com/dev/api/#section_wiki).";

#[derive(OpenApiDerive)]
#[openapi(
	info(description = "Private Reddit-compatible read API proxy. Reddit-owned links and media URLs resolve through this service. Common default-listing and permalink aliases are accepted but omitted here."),
	servers((url = "/api")),
	paths(
		crate::api::routes::wiki_page,
		crate::api::routes::wiki_page_revisions,
		crate::api::routes::wiki_discussions
	),
	components(schemas(CommentChild, More, Listing<CommentChild>)),
	external_docs(url = "https://www.reddit.com/dev/api/", description = "Canonical Reddit API documentation")
)]
struct ApiDoc;

pub(super) fn router() -> OpenApiRouter<RedditService> {
	OpenApiRouter::with_openapi(ApiDoc::openapi())
}

pub(super) fn finish(router: OpenApiRouter<RedditService>, service: RedditService) -> Router {
	let (router, openapi) = router.with_state(service).split_for_parts();
	let openapi = Arc::new(openapi);
	router.route(
		"/openapi.json",
		get(move || {
			let openapi = Arc::clone(&openapi);
			async move { Json(OpenApi::clone(&openapi)) }
		}),
	)
}
