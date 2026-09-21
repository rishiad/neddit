use crate::api::docs::{COMMENTS, LISTINGS, MORE_CHILDREN, SUBREDDITS, SUBREDDIT_RULES, SUBREDDIT_SIDEBAR, USERS, WIKI};
use crate::api::error::ApiError;
use crate::api::query::{comment_query, duplicate_query, listing_query, more_children_query, subreddit_search_query, wiki_page_query};
use crate::api::response::{respond, ErrorBody};
use crate::models::{Listing, MoreChildren, Post, PostComments, Sidebar, Subreddit, SubredditRules, Thing, User, WikiPage, WikiPageListing, WikiRevision};
use crate::service::{CommentQuery, DuplicateQuery, ListingQuery, MoreChildrenQuery, PostSort, RedditService, SubredditSearchQuery, SubredditSort, WikiPageQuery};
use axum::{
	extract::{Path, RawQuery, State},
	response::Response,
	routing::get,
};
use utoipa_axum::{router::OpenApiRouter, routes};

type PostListings = Vec<Listing<Thing<Post>>>;

#[utoipa::path(get, path = "/best", description = LISTINGS, params(ListingQuery), responses(
	(status = 200, description = "Reddit post listing", body = Listing<Thing<Post>>),
	(status = "default", description = "Reddit-compatible error", body = ErrorBody)
), tag = "listings")]
async fn best(state: State<RedditService>, query: RawQuery) -> Response {
	front_page(state, query, PostSort::Best).await
}

#[utoipa::path(get, path = "/hot", description = LISTINGS, params(ListingQuery), responses(
	(status = 200, description = "Reddit post listing", body = Listing<Thing<Post>>),
	(status = "default", description = "Reddit-compatible error", body = ErrorBody)
), tag = "listings")]
async fn hot(state: State<RedditService>, query: RawQuery) -> Response {
	front_page(state, query, PostSort::Hot).await
}

#[utoipa::path(get, path = "/new", description = LISTINGS, params(ListingQuery), responses(
	(status = 200, description = "Reddit post listing", body = Listing<Thing<Post>>),
	(status = "default", description = "Reddit-compatible error", body = ErrorBody)
), tag = "listings")]
async fn new(state: State<RedditService>, query: RawQuery) -> Response {
	front_page(state, query, PostSort::New).await
}

#[utoipa::path(get, path = "/rising", description = LISTINGS, params(ListingQuery), responses(
	(status = 200, description = "Reddit post listing", body = Listing<Thing<Post>>),
	(status = "default", description = "Reddit-compatible error", body = ErrorBody)
), tag = "listings")]
async fn rising(state: State<RedditService>, query: RawQuery) -> Response {
	front_page(state, query, PostSort::Rising).await
}

#[utoipa::path(get, path = "/top", description = LISTINGS, params(ListingQuery), responses(
	(status = 200, description = "Reddit post listing", body = Listing<Thing<Post>>),
	(status = "default", description = "Reddit-compatible error", body = ErrorBody)
), tag = "listings")]
async fn top(state: State<RedditService>, query: RawQuery) -> Response {
	front_page(state, query, PostSort::Top).await
}

#[utoipa::path(get, path = "/controversial", description = LISTINGS, params(ListingQuery), responses(
	(status = 200, description = "Reddit post listing", body = Listing<Thing<Post>>),
	(status = "default", description = "Reddit-compatible error", body = ErrorBody)
), tag = "listings")]
async fn controversial(state: State<RedditService>, query: RawQuery) -> Response {
	front_page(state, query, PostSort::Controversial).await
}

#[utoipa::path(get, path = "/r/{subreddit}/hot", description = LISTINGS, params(("subreddit" = String, Path), ListingQuery), responses(
	(status = 200, description = "Reddit post listing", body = Listing<Thing<Post>>),
	(status = "default", description = "Reddit-compatible error", body = ErrorBody)
), tag = "listings")]
async fn subreddit_hot(state: State<RedditService>, path: Path<String>, query: RawQuery) -> Response {
	subreddit(state, path, query, PostSort::Hot).await
}

#[utoipa::path(get, path = "/r/{subreddit}/new", description = LISTINGS, params(("subreddit" = String, Path), ListingQuery), responses(
	(status = 200, description = "Reddit post listing", body = Listing<Thing<Post>>),
	(status = "default", description = "Reddit-compatible error", body = ErrorBody)
), tag = "listings")]
async fn subreddit_new(state: State<RedditService>, path: Path<String>, query: RawQuery) -> Response {
	subreddit(state, path, query, PostSort::New).await
}

#[utoipa::path(get, path = "/r/{subreddit}/rising", description = LISTINGS, params(("subreddit" = String, Path), ListingQuery), responses(
	(status = 200, description = "Reddit post listing", body = Listing<Thing<Post>>),
	(status = "default", description = "Reddit-compatible error", body = ErrorBody)
), tag = "listings")]
async fn subreddit_rising(state: State<RedditService>, path: Path<String>, query: RawQuery) -> Response {
	subreddit(state, path, query, PostSort::Rising).await
}

#[utoipa::path(get, path = "/r/{subreddit}/top", description = LISTINGS, params(("subreddit" = String, Path), ListingQuery), responses(
	(status = 200, description = "Reddit post listing", body = Listing<Thing<Post>>),
	(status = "default", description = "Reddit-compatible error", body = ErrorBody)
), tag = "listings")]
async fn subreddit_top(state: State<RedditService>, path: Path<String>, query: RawQuery) -> Response {
	subreddit(state, path, query, PostSort::Top).await
}

#[utoipa::path(get, path = "/r/{subreddit}/controversial", description = LISTINGS, params(("subreddit" = String, Path), ListingQuery), responses(
	(status = 200, description = "Reddit post listing", body = Listing<Thing<Post>>),
	(status = "default", description = "Reddit-compatible error", body = ErrorBody)
), tag = "listings")]
async fn subreddit_controversial(state: State<RedditService>, path: Path<String>, query: RawQuery) -> Response {
	subreddit(state, path, query, PostSort::Controversial).await
}

#[utoipa::path(get, path = "/subreddits/popular", description = SUBREDDITS, params(ListingQuery), responses(
	(status = 200, description = "Reddit subreddit listing", body = Listing<Thing<Subreddit>>),
	(status = "default", description = "Reddit-compatible error", body = ErrorBody)
), tag = "subreddits")]
async fn subreddits_popular(state: State<RedditService>, query: RawQuery) -> Response {
	subreddits(state, query, SubredditSort::Popular).await
}

#[utoipa::path(get, path = "/subreddits/new", description = SUBREDDITS, params(ListingQuery), responses(
	(status = 200, description = "Reddit subreddit listing", body = Listing<Thing<Subreddit>>),
	(status = "default", description = "Reddit-compatible error", body = ErrorBody)
), tag = "subreddits")]
async fn subreddits_new(state: State<RedditService>, query: RawQuery) -> Response {
	subreddits(state, query, SubredditSort::New).await
}

#[utoipa::path(get, path = "/subreddits/default", description = SUBREDDITS, params(ListingQuery), responses(
	(status = 200, description = "Reddit subreddit listing", body = Listing<Thing<Subreddit>>),
	(status = "default", description = "Reddit-compatible error", body = ErrorBody)
), tag = "subreddits")]
async fn subreddits_default(state: State<RedditService>, query: RawQuery) -> Response {
	subreddits(state, query, SubredditSort::Default).await
}

pub(super) fn router() -> OpenApiRouter<RedditService> {
	OpenApiRouter::default()
		.route("/", get(front_page_default))
		.routes(routes!(best))
		.routes(routes!(hot))
		.routes(routes!(new))
		.routes(routes!(rising))
		.routes(routes!(top))
		.routes(routes!(controversial))
		.routes(routes!(subreddit_hot))
		.routes(routes!(subreddit_new))
		.routes(routes!(subreddit_rising))
		.routes(routes!(subreddit_top))
		.routes(routes!(subreddit_controversial))
		.route("/r/{subreddit}", get(subreddit_default))
		.routes(routes!(post_comments))
		.routes(routes!(subreddit_post_comments))
		.route("/comments/{article}/{slug}", get(post_permalink))
		.route("/comments/{article}/{slug}/{comment}", get(post_comment_permalink))
		.route("/r/{subreddit}/comments/{article}/{slug}", get(subreddit_post_permalink))
		.route("/r/{subreddit}/comments/{article}/{slug}/{comment}", get(subreddit_post_comment_permalink))
		.routes(routes!(more_children))
		.routes(routes!(subreddit_about))
		.routes(routes!(subreddit_rules))
		.routes(routes!(subreddit_sidebar))
		.routes(routes!(wiki_pages))
		.routes(routes!(wiki_revisions))
		.route("/r/{subreddit}/wiki", get(wiki_index))
		.route("/r/{subreddit}/wiki/revisions/{*page}", get(wiki_page_revisions))
		.route("/r/{subreddit}/wiki/discussions/{*page}", get(wiki_discussions))
		.route("/r/{subreddit}/wiki/{*page}", get(wiki_page))
		.routes(routes!(user_about))
		.routes(routes!(posts_by_id))
		.routes(routes!(post_duplicates))
		.routes(routes!(subreddits_popular))
		.routes(routes!(subreddits_new))
		.routes(routes!(subreddits_default))
		.routes(routes!(search_subreddits))
}

async fn front_page_default(state: State<RedditService>, query: RawQuery) -> Response {
	front_page(state, query, PostSort::Hot).await
}

async fn subreddit_default(state: State<RedditService>, path: Path<String>, query: RawQuery) -> Response {
	subreddit(state, path, query, PostSort::Hot).await
}

#[utoipa::path(
	get,
	path = "/morechildren",
	description = MORE_CHILDREN,
	params(MoreChildrenQuery),
	responses(
		(status = 200, description = "Reddit additional-comment response", body = MoreChildren),
		(status = "default", description = "Reddit-compatible error", body = ErrorBody)
	),
	tag = "comments"
)]
async fn more_children(State(service): State<RedditService>, RawQuery(raw_query): RawQuery) -> Response {
	let result = async {
		let query = more_children_query(raw_query.as_deref())?;
		service.more_children(&query).await.map_err(ApiError::from)
	}
	.await;
	respond(result)
}

async fn front_page(State(service): State<RedditService>, RawQuery(raw_query): RawQuery, sort: PostSort) -> Response {
	let result = async {
		let query = listing_query(raw_query.as_deref())?;
		service.front_page_posts(sort, &query).await.map_err(ApiError::from)
	}
	.await;
	respond(result)
}

async fn subreddit(State(service): State<RedditService>, Path(subreddit): Path<String>, RawQuery(raw_query): RawQuery, sort: PostSort) -> Response {
	let result = async {
		let query = listing_query(raw_query.as_deref())?;
		service.subreddit_posts(&subreddit, sort, &query).await.map_err(ApiError::from)
	}
	.await;
	respond(result)
}

#[utoipa::path(
	get,
	path = "/comments/{article}",
	description = COMMENTS,
	params(("article" = String, Path), CommentQuery),
	responses(
		(status = 200, description = "Reddit post and comment listings", body = PostComments),
		(status = "default", description = "Reddit-compatible error", body = ErrorBody)
	),
	tag = "comments"
)]
async fn post_comments(State(service): State<RedditService>, Path(article): Path<String>, RawQuery(raw_query): RawQuery) -> Response {
	comments(service, None, article, None, raw_query).await
}

#[utoipa::path(
	get,
	path = "/r/{subreddit}/comments/{article}",
	description = COMMENTS,
	params(("subreddit" = String, Path), ("article" = String, Path), CommentQuery),
	responses(
		(status = 200, description = "Reddit post and comment listings", body = PostComments),
		(status = "default", description = "Reddit-compatible error", body = ErrorBody)
	),
	tag = "comments"
)]
async fn subreddit_post_comments(State(service): State<RedditService>, Path((subreddit, article)): Path<(String, String)>, RawQuery(raw_query): RawQuery) -> Response {
	comments(service, Some(subreddit), article, None, raw_query).await
}

async fn post_permalink(State(service): State<RedditService>, Path((article, _slug)): Path<(String, String)>, RawQuery(raw_query): RawQuery) -> Response {
	comments(service, None, article, None, raw_query).await
}

async fn post_comment_permalink(
	State(service): State<RedditService>,
	Path((article, _slug, comment)): Path<(String, String, String)>,
	RawQuery(raw_query): RawQuery,
) -> Response {
	comments(service, None, article, Some(comment), raw_query).await
}

async fn subreddit_post_permalink(
	State(service): State<RedditService>,
	Path((subreddit, article, _slug)): Path<(String, String, String)>,
	RawQuery(raw_query): RawQuery,
) -> Response {
	comments(service, Some(subreddit), article, None, raw_query).await
}

async fn subreddit_post_comment_permalink(
	State(service): State<RedditService>,
	Path((subreddit, article, _slug, comment)): Path<(String, String, String, String)>,
	RawQuery(raw_query): RawQuery,
) -> Response {
	comments(service, Some(subreddit), article, Some(comment), raw_query).await
}

async fn comments(service: RedditService, subreddit: Option<String>, article: String, comment: Option<String>, raw_query: Option<String>) -> Response {
	let result = async {
		let mut query = comment_query(raw_query.as_deref())?;
		if comment.is_some() {
			query.comment = comment;
		}
		match subreddit {
			Some(subreddit) => service.subreddit_post_comments(&subreddit, &article, &query).await.map_err(ApiError::from),
			None => service.post_comments(&article, &query).await.map_err(ApiError::from),
		}
	}
	.await;
	respond(result)
}

#[utoipa::path(
	get,
	path = "/r/{subreddit}/about",
	description = SUBREDDITS,
	params(("subreddit" = String, Path)),
	responses(
		(status = 200, description = "Reddit subreddit", body = Thing<Subreddit>),
		(status = "default", description = "Reddit-compatible error", body = ErrorBody)
	),
	tag = "subreddits"
)]
async fn subreddit_about(State(service): State<RedditService>, Path(subreddit): Path<String>) -> Response {
	respond(service.subreddit_about(&subreddit).await.map_err(ApiError::from))
}

#[utoipa::path(
	get,
	path = "/r/{subreddit}/about/rules",
	description = SUBREDDIT_RULES,
	params(("subreddit" = String, Path)),
	responses(
		(status = 200, description = "Reddit subreddit rules", body = SubredditRules),
		(status = "default", description = "Reddit-compatible error", body = ErrorBody)
	),
	tag = "subreddits"
)]
async fn subreddit_rules(State(service): State<RedditService>, Path(subreddit): Path<String>) -> Response {
	respond(service.subreddit_rules(&subreddit).await.map_err(ApiError::from))
}

#[utoipa::path(
	get,
	path = "/r/{subreddit}/sidebar",
	description = SUBREDDIT_SIDEBAR,
	params(("subreddit" = String, Path)),
	responses(
		(status = 200, description = "Reddit subreddit sidebar", body = Sidebar),
		(status = "default", description = "Reddit-compatible error", body = ErrorBody)
	),
	tag = "subreddits"
)]
async fn subreddit_sidebar(State(service): State<RedditService>, Path(subreddit): Path<String>) -> Response {
	respond(service.subreddit_sidebar(&subreddit).await.map_err(ApiError::from))
}

#[utoipa::path(
	get,
	path = "/r/{subreddit}/wiki/pages",
	description = WIKI,
	params(("subreddit" = String, Path)),
	responses(
		(status = 200, description = "Reddit wiki page names", body = WikiPageListing),
		(status = "default", description = "Reddit-compatible error", body = ErrorBody)
	),
	tag = "wiki"
)]
async fn wiki_pages(State(service): State<RedditService>, Path(subreddit): Path<String>) -> Response {
	respond(service.wiki_pages(&subreddit).await.map_err(ApiError::from))
}

#[utoipa::path(
	get,
	path = "/r/{subreddit}/wiki/{page}",
	description = WIKI,
	params(("subreddit" = String, Path), ("page" = String, Path), WikiPageQuery),
	responses(
		(status = 200, description = "Reddit wiki page", body = WikiPage),
		(status = "default", description = "Reddit-compatible error", body = ErrorBody)
	),
	tag = "wiki"
)]
pub(super) async fn wiki_page(State(service): State<RedditService>, Path((subreddit, page)): Path<(String, String)>, RawQuery(raw_query): RawQuery) -> Response {
	respond(service.wiki_page(&subreddit, &page, &wiki_page_query(raw_query.as_deref())).await.map_err(ApiError::from))
}

async fn wiki_index(State(service): State<RedditService>, Path(subreddit): Path<String>, RawQuery(raw_query): RawQuery) -> Response {
	respond(service.wiki_page(&subreddit, "index", &wiki_page_query(raw_query.as_deref())).await.map_err(ApiError::from))
}

#[utoipa::path(
	get,
	path = "/r/{subreddit}/wiki/revisions",
	description = WIKI,
	params(("subreddit" = String, Path), ListingQuery),
	responses(
		(status = 200, description = "Reddit wiki revision listing", body = Listing<WikiRevision>),
		(status = "default", description = "Reddit-compatible error", body = ErrorBody)
	),
	tag = "wiki"
)]
async fn wiki_revisions(State(service): State<RedditService>, Path(subreddit): Path<String>, RawQuery(raw_query): RawQuery) -> Response {
	let result = async {
		let query = listing_query(raw_query.as_deref())?;
		service.wiki_revisions(&subreddit, None, &query).await.map_err(ApiError::from)
	}
	.await;
	respond(result)
}

#[utoipa::path(
	get,
	path = "/r/{subreddit}/wiki/revisions/{page}",
	description = WIKI,
	params(("subreddit" = String, Path), ("page" = String, Path), ListingQuery),
	responses(
		(status = 200, description = "Reddit wiki page revision listing", body = Listing<WikiRevision>),
		(status = "default", description = "Reddit-compatible error", body = ErrorBody)
	),
	tag = "wiki"
)]
pub(super) async fn wiki_page_revisions(State(service): State<RedditService>, Path((subreddit, page)): Path<(String, String)>, RawQuery(raw_query): RawQuery) -> Response {
	let result = async {
		let query = listing_query(raw_query.as_deref())?;
		service.wiki_revisions(&subreddit, Some(&page), &query).await.map_err(ApiError::from)
	}
	.await;
	respond(result)
}

#[utoipa::path(
	get,
	path = "/r/{subreddit}/wiki/discussions/{page}",
	description = WIKI,
	params(("subreddit" = String, Path), ("page" = String, Path), ListingQuery),
	responses(
		(status = 200, description = "Reddit wiki page discussion listing", body = Listing<Thing<Post>>),
		(status = "default", description = "Reddit-compatible error", body = ErrorBody)
	),
	tag = "wiki"
)]
pub(super) async fn wiki_discussions(State(service): State<RedditService>, Path((subreddit, page)): Path<(String, String)>, RawQuery(raw_query): RawQuery) -> Response {
	let result = async {
		let query = listing_query(raw_query.as_deref())?;
		service.wiki_discussions(&subreddit, &page, &query).await.map_err(ApiError::from)
	}
	.await;
	respond(result)
}

#[utoipa::path(
	get,
	path = "/user/{username}/about",
	description = USERS,
	params(("username" = String, Path)),
	responses(
		(status = 200, description = "Reddit user", body = Thing<User>),
		(status = "default", description = "Reddit-compatible error", body = ErrorBody)
	),
	tag = "users"
)]
async fn user_about(State(service): State<RedditService>, Path(username): Path<String>) -> Response {
	respond(service.user_about(&username).await.map_err(ApiError::from))
}

#[utoipa::path(
	get,
	path = "/by_id/{names}",
	description = LISTINGS,
	params(("names" = String, Path)),
	responses(
		(status = 200, description = "Reddit post listing", body = Listing<Thing<Post>>),
		(status = "default", description = "Reddit-compatible error", body = ErrorBody)
	),
	tag = "listings"
)]
async fn posts_by_id(State(service): State<RedditService>, Path(names): Path<String>) -> Response {
	respond(service.posts_by_id(&names).await.map_err(ApiError::from))
}

#[utoipa::path(
	get,
	path = "/duplicates/{article}",
	description = LISTINGS,
	params(("article" = String, Path), ListingQuery, DuplicateQuery),
	responses(
		(status = 200, description = "Reddit post and duplicate listings", body = PostListings),
		(status = "default", description = "Reddit-compatible error", body = ErrorBody)
	),
	tag = "listings"
)]
async fn post_duplicates(State(service): State<RedditService>, Path(article): Path<String>, RawQuery(raw_query): RawQuery) -> Response {
	let result = async {
		let query = duplicate_query(raw_query.as_deref())?;
		service.post_duplicates(&article, &query).await.map_err(ApiError::from)
	}
	.await;
	respond(result)
}

async fn subreddits(State(service): State<RedditService>, RawQuery(raw_query): RawQuery, sort: SubredditSort) -> Response {
	let result = async {
		let query = listing_query(raw_query.as_deref())?;
		service.subreddits(sort, &query).await.map_err(ApiError::from)
	}
	.await;
	respond(result)
}

#[utoipa::path(
	get,
	path = "/subreddits/search",
	description = SUBREDDITS,
	params(ListingQuery, SubredditSearchQuery),
	responses(
		(status = 200, description = "Reddit subreddit listing", body = Listing<Thing<Subreddit>>),
		(status = "default", description = "Reddit-compatible error", body = ErrorBody)
	),
	tag = "subreddits"
)]
async fn search_subreddits(State(service): State<RedditService>, RawQuery(raw_query): RawQuery) -> Response {
	let result = async {
		let query = subreddit_search_query(raw_query.as_deref())?;
		service.search_subreddits(&query).await.map_err(ApiError::from)
	}
	.await;
	respond(result)
}
