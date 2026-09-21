use crate::{
	api::error::ApiError,
	service::{CommentQuery, DuplicateQuery, InfoQuery, ListingQuery, MoreChildrenQuery, SearchQuery, SubredditSearchQuery, UserHistoryQuery, UserSearchQuery, WikiPageQuery},
};
use serde::de::DeserializeOwned;

fn decode<T: DeserializeOwned>(raw_query: Option<&str>) -> Result<T, ApiError> {
	serde_urlencoded::from_str(raw_query.unwrap_or_default()).map_err(|_| ApiError::InvalidQuery)
}

pub(super) fn listing_query(raw_query: Option<&str>) -> Result<ListingQuery, ApiError> {
	decode(raw_query)
}

pub(super) fn comment_query(raw_query: Option<&str>) -> Result<CommentQuery, ApiError> {
	decode(raw_query)
}

pub(super) fn more_children_query(raw_query: Option<&str>) -> Result<MoreChildrenQuery, ApiError> {
	decode(raw_query)
}

pub(super) fn duplicate_query(raw_query: Option<&str>) -> Result<DuplicateQuery, ApiError> {
	decode(raw_query)
}

pub(super) fn subreddit_search_query(raw_query: Option<&str>) -> Result<SubredditSearchQuery, ApiError> {
	decode(raw_query)
}

pub(super) fn wiki_page_query(raw_query: Option<&str>) -> WikiPageQuery {
	decode(raw_query).unwrap_or_default()
}

pub(super) fn search_query(raw_query: Option<&str>) -> Result<SearchQuery, ApiError> {
	decode(raw_query)
}

pub(super) fn info_query(raw_query: Option<&str>) -> InfoQuery {
	decode(raw_query).unwrap_or_default()
}

pub(super) fn user_history_query(raw_query: Option<&str>) -> Result<UserHistoryQuery, ApiError> {
	decode(raw_query)
}

pub(super) fn user_search_query(raw_query: Option<&str>) -> Result<UserSearchQuery, ApiError> {
	decode(raw_query)
}
