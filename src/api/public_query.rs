use crate::api::error::ApiError;
use crate::api::query::{apply_listing_pair, boolean, invalid, listing_query, number, pairs};
use crate::service::{
	InfoQuery, ListingQuery, SearchQuery, SearchResultType, SearchSort, SubredditSearchSort, Typeahead, UserHistoryQuery, UserHistoryShow, UserHistorySort, UserHistoryType,
	UserSearchQuery,
};

pub(super) fn search_query(raw_query: Option<&str>) -> Result<SearchQuery, ApiError> {
	let mut query = SearchQuery {
		listing: listing_query(raw_query)?,
		..Default::default()
	};
	for (key, value) in pairs(raw_query) {
		match key.as_ref() {
			"category" => query.category = Some(value.into_owned()),
			"include_facets" => query.include_facets = Some(boolean("include_facets", &value)?),
			"q" => query.query = value.into_owned(),
			"restrict_sr" => query.restrict_sr = Some(boolean("restrict_sr", &value)?),
			"sort" => {
				query.sort = Some(match value.as_ref() {
					"relevance" => SearchSort::Relevance,
					"hot" => SearchSort::Hot,
					"top" => SearchSort::Top,
					"new" => SearchSort::New,
					"comments" => SearchSort::Comments,
					_ => return Err(invalid("sort", &value)),
				})
			}
			"type" => {
				query.result_types = comma_values(&value)
					.map(|value| match value {
						"sr" => Ok(SearchResultType::Subreddit),
						"link" => Ok(SearchResultType::Post),
						"user" => Ok(SearchResultType::User),
						_ => Err(invalid("type", value)),
					})
					.collect::<Result<_, _>>()?;
			}
			_ => {}
		}
	}
	Ok(query)
}

pub(super) fn info_query(raw_query: Option<&str>) -> InfoQuery {
	let mut query = InfoQuery::default();
	for (key, value) in pairs(raw_query) {
		match key.as_ref() {
			"id" => query.ids.extend(comma_values(&value).map(str::to_string)),
			"sr_name" => query.subreddit_names.extend(comma_values(&value).map(str::to_string)),
			"url" => query.url = Some(value.into_owned()),
			_ => {}
		}
	}
	query
}

pub(super) fn user_history_query(raw_query: Option<&str>) -> Result<UserHistoryQuery, ApiError> {
	let mut query = UserHistoryQuery {
		listing: ListingQuery::default(),
		..Default::default()
	};
	for (key, value) in pairs(raw_query) {
		if apply_listing_pair(&mut query.listing, &key, &value)? {
			continue;
		}
		match key.as_ref() {
			"context" => query.context = Some(number("context", &value)?),
			"show" => {
				query.show = Some(match value.as_ref() {
					"given" => UserHistoryShow::Given,
					_ => return Err(invalid("show", &value)),
				})
			}
			"sort" => {
				query.sort = Some(match value.as_ref() {
					"hot" => UserHistorySort::Hot,
					"new" => UserHistorySort::New,
					"top" => UserHistorySort::Top,
					"controversial" => UserHistorySort::Controversial,
					_ => return Err(invalid("sort", &value)),
				})
			}
			"type" => {
				query.content_type = Some(match value.as_ref() {
					"links" => UserHistoryType::Posts,
					"comments" => UserHistoryType::Comments,
					_ => return Err(invalid("type", &value)),
				})
			}
			_ => {}
		}
	}
	Ok(query)
}

pub(super) fn user_search_query(raw_query: Option<&str>) -> Result<UserSearchQuery, ApiError> {
	let mut query = UserSearchQuery {
		listing: listing_query(raw_query)?,
		..Default::default()
	};
	for (key, value) in pairs(raw_query) {
		match key.as_ref() {
			"q" => query.query = value.into_owned(),
			"search_query_id" => query.search_query_id = Some(value.into_owned()),
			"sort" => {
				query.sort = Some(match value.as_ref() {
					"relevance" => SubredditSearchSort::Relevance,
					"activity" => SubredditSearchSort::Activity,
					_ => return Err(invalid("sort", &value)),
				})
			}
			"typeahead_active" => {
				query.typeahead_active = Some(match value.as_ref() {
					"true" => Typeahead::Boolean(true),
					"false" => Typeahead::Boolean(false),
					"None" => Typeahead::None,
					_ => return Err(invalid("typeahead_active", &value)),
				})
			}
			_ => {}
		}
	}
	Ok(query)
}

fn comma_values(value: &str) -> impl Iterator<Item = &str> {
	value.split(',').map(str::trim).filter(|value| !value.is_empty())
}
