use crate::api::docs::{INFO, SEARCH, TROPHIES, USER_DIRECTORIES, USER_HISTORY};
use crate::api::error::ApiError;
use crate::api::query::{info_query, listing_query, search_query, user_history_query, user_search_query};
use crate::api::response::{respond, ErrorBody};
use crate::models::{Listing, Post, PublicThing, Subreddit, Thing, TrophyList, User};
use crate::service::{InfoQuery, ListingQuery, RedditService, SearchQuery, UserDirectorySort, UserHistoryQuery, UserSearchQuery};
use axum::{
	extract::{Path, RawQuery, State},
	response::Response,
	routing::get,
};
use utoipa_axum::{router::OpenApiRouter, routes};

pub(super) fn router() -> OpenApiRouter<RedditService> {
	OpenApiRouter::default()
		.routes(routes!(search))
		.routes(routes!(search_subreddit))
		.routes(routes!(info))
		.routes(routes!(subreddit_info))
		.route("/user/{username}", get(user_overview_default))
		.routes(routes!(user_overview))
		.routes(routes!(user_submitted))
		.routes(routes!(user_comments))
		.routes(routes!(user_trophies))
		.routes(routes!(users_new))
		.routes(routes!(users_popular))
		.routes(routes!(search_users))
}

async fn user_overview_default(state: State<RedditService>, path: Path<String>, query: RawQuery) -> Response {
	user_overview(state, path, query).await
}

#[utoipa::path(
	get,
	path = "/search",
	description = SEARCH,
	params(ListingQuery, SearchQuery),
	responses(
		(status = 200, description = "Reddit search listing", body = Listing<PublicThing>),
		(status = "default", description = "Reddit-compatible error", body = ErrorBody)
	),
	tag = "search"
)]
async fn search(State(service): State<RedditService>, RawQuery(raw_query): RawQuery) -> Response {
	let result = async {
		let query = search_query(raw_query.as_deref())?;
		service.search(&query).await.map_err(ApiError::from)
	}
	.await;
	respond(result)
}

#[utoipa::path(
	get,
	path = "/r/{subreddit}/search",
	description = SEARCH,
	params(("subreddit" = String, Path), ListingQuery, SearchQuery),
	responses(
		(status = 200, description = "Reddit search listing", body = Listing<PublicThing>),
		(status = "default", description = "Reddit-compatible error", body = ErrorBody)
	),
	tag = "search"
)]
async fn search_subreddit(State(service): State<RedditService>, Path(subreddit): Path<String>, RawQuery(raw_query): RawQuery) -> Response {
	let result = async {
		let query = search_query(raw_query.as_deref())?;
		service.search_subreddit(&subreddit, &query).await.map_err(ApiError::from)
	}
	.await;
	respond(result)
}

#[utoipa::path(
	get,
	path = "/api/info",
	description = INFO,
	params(InfoQuery),
	responses(
		(status = 200, description = "Reddit thing listing", body = Listing<PublicThing>),
		(status = "default", description = "Reddit-compatible error", body = ErrorBody)
	),
	tag = "metadata"
)]
async fn info(State(service): State<RedditService>, RawQuery(raw_query): RawQuery) -> Response {
	let query = info_query(raw_query.as_deref());
	respond(service.info(&query).await.map_err(ApiError::from))
}

#[utoipa::path(
	get,
	path = "/r/{subreddit}/api/info",
	description = INFO,
	params(("subreddit" = String, Path), InfoQuery),
	responses(
		(status = 200, description = "Reddit thing listing", body = Listing<PublicThing>),
		(status = "default", description = "Reddit-compatible error", body = ErrorBody)
	),
	tag = "metadata"
)]
async fn subreddit_info(State(service): State<RedditService>, Path(subreddit): Path<String>, RawQuery(raw_query): RawQuery) -> Response {
	let query = info_query(raw_query.as_deref());
	respond(service.subreddit_info(&subreddit, &query).await.map_err(ApiError::from))
}

#[utoipa::path(
	get,
	path = "/user/{username}/overview",
	description = USER_HISTORY,
	params(
		("username" = String, Path),
		("after" = Option<String>, Query),
		("before" = Option<String>, Query),
		("count" = Option<u32>, Query),
		("limit" = Option<u8>, Query, minimum = 1, maximum = 100),
		("t" = Option<String>, Query),
		("sr_detail" = Option<bool>, Query),
		UserHistoryQuery
	),
	responses(
		(status = 200, description = "Reddit user activity listing", body = Listing<PublicThing>),
		(status = "default", description = "Reddit-compatible error", body = ErrorBody)
	),
	tag = "users"
)]
async fn user_overview(State(service): State<RedditService>, Path(username): Path<String>, RawQuery(raw_query): RawQuery) -> Response {
	let result = async {
		let query = user_history_query(raw_query.as_deref())?;
		service.user_overview(&username, &query).await.map_err(ApiError::from)
	}
	.await;
	respond(result)
}

#[utoipa::path(
	get,
	path = "/user/{username}/submitted",
	description = USER_HISTORY,
	params(
		("username" = String, Path),
		("after" = Option<String>, Query),
		("before" = Option<String>, Query),
		("count" = Option<u32>, Query),
		("limit" = Option<u8>, Query, minimum = 1, maximum = 100),
		("t" = Option<String>, Query),
		("sr_detail" = Option<bool>, Query),
		UserHistoryQuery
	),
	responses(
		(status = 200, description = "Reddit user post listing", body = Listing<Thing<Post>>),
		(status = "default", description = "Reddit-compatible error", body = ErrorBody)
	),
	tag = "users"
)]
async fn user_submitted(State(service): State<RedditService>, Path(username): Path<String>, RawQuery(raw_query): RawQuery) -> Response {
	let result = async {
		let query = user_history_query(raw_query.as_deref())?;
		service.user_submitted(&username, &query).await.map_err(ApiError::from)
	}
	.await;
	respond(result)
}

#[utoipa::path(
	get,
	path = "/user/{username}/comments",
	description = USER_HISTORY,
	params(
		("username" = String, Path),
		("after" = Option<String>, Query),
		("before" = Option<String>, Query),
		("count" = Option<u32>, Query),
		("limit" = Option<u8>, Query, minimum = 1, maximum = 100),
		("t" = Option<String>, Query),
		("sr_detail" = Option<bool>, Query),
		UserHistoryQuery
	),
	responses(
		(status = 200, description = "Reddit user comment listing", body = Listing<PublicThing>),
		(status = "default", description = "Reddit-compatible error", body = ErrorBody)
	),
	tag = "users"
)]
async fn user_comments(State(service): State<RedditService>, Path(username): Path<String>, RawQuery(raw_query): RawQuery) -> Response {
	let result = async {
		let query = user_history_query(raw_query.as_deref())?;
		service.user_comments(&username, &query).await.map_err(ApiError::from)
	}
	.await;
	respond(result)
}

#[utoipa::path(
	get,
	path = "/api/v1/user/{username}/trophies",
	description = TROPHIES,
	params(("username" = String, Path)),
	responses(
		(status = 200, description = "Reddit trophy list", body = TrophyList),
		(status = "default", description = "Reddit-compatible error", body = ErrorBody)
	),
	tag = "users"
)]
async fn user_trophies(State(service): State<RedditService>, Path(username): Path<String>) -> Response {
	respond(service.user_trophies(&username).await.map_err(ApiError::from))
}

#[utoipa::path(
	get,
	path = "/users/new",
	description = USER_DIRECTORIES,
	params(ListingQuery),
	responses(
		(status = 200, description = "Reddit user subreddit listing", body = Listing<Thing<Subreddit>>),
		(status = "default", description = "Reddit-compatible error", body = ErrorBody)
	),
	tag = "users"
)]
async fn users_new(state: State<RedditService>, query: RawQuery) -> Response {
	users(state, query, UserDirectorySort::New).await
}

#[utoipa::path(
	get,
	path = "/users/popular",
	description = USER_DIRECTORIES,
	params(ListingQuery),
	responses(
		(status = 200, description = "Reddit user subreddit listing", body = Listing<Thing<Subreddit>>),
		(status = "default", description = "Reddit-compatible error", body = ErrorBody)
	),
	tag = "users"
)]
async fn users_popular(state: State<RedditService>, query: RawQuery) -> Response {
	users(state, query, UserDirectorySort::Popular).await
}

async fn users(State(service): State<RedditService>, RawQuery(raw_query): RawQuery, sort: UserDirectorySort) -> Response {
	let result = async {
		let query = listing_query(raw_query.as_deref())?;
		service.users(sort, &query).await.map_err(ApiError::from)
	}
	.await;
	respond(result)
}

#[utoipa::path(
	get,
	path = "/users/search",
	description = USER_DIRECTORIES,
	params(ListingQuery, UserSearchQuery),
	responses(
		(status = 200, description = "Reddit user listing", body = Listing<Thing<User>>),
		(status = "default", description = "Reddit-compatible error", body = ErrorBody)
	),
	tag = "users"
)]
async fn search_users(State(service): State<RedditService>, RawQuery(raw_query): RawQuery) -> Response {
	let result = async {
		let query = user_search_query(raw_query.as_deref())?;
		service.search_users(&query).await.map_err(ApiError::from)
	}
	.await;
	respond(result)
}
