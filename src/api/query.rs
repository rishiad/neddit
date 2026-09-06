use crate::api::error::ApiError;
use crate::service::{
	CommentQuery, CommentSort, CommentTheme, DuplicateQuery, DuplicateSort, ListingQuery, ListingShow, ListingTime, MoreChildrenApiType, MoreChildrenQuery,
	SubredditSearchQuery, SubredditSearchSort, Typeahead, WikiPageQuery,
};
use std::str::FromStr;
use url::form_urlencoded;

pub(super) fn listing_query(raw_query: Option<&str>) -> Result<ListingQuery, ApiError> {
	let mut query = ListingQuery::default();
	for (key, value) in pairs(raw_query) {
		if apply_listing_pair(&mut query, &key, &value)? {
			continue;
		}
		if key == "show" {
			query.show = Some(match value.as_ref() {
				"all" => ListingShow::All,
				_ => return Err(invalid("show", &value)),
			});
		}
	}
	Ok(query)
}

pub(super) fn wiki_page_query(raw_query: Option<&str>) -> WikiPageQuery {
	let mut query = WikiPageQuery::default();
	for (key, value) in pairs(raw_query) {
		match key.as_ref() {
			"v" => query.v = Some(value.into_owned()),
			"v2" => query.v2 = Some(value.into_owned()),
			_ => {}
		}
	}
	query
}

pub(super) fn apply_listing_pair(query: &mut ListingQuery, key: &str, value: &str) -> Result<bool, ApiError> {
	match key {
		"after" => query.after = Some(value.to_string()),
		"before" => query.before = Some(value.to_string()),
		"limit" => query.limit = Some(number("limit", value)?),
		"count" => query.count = Some(number("count", value)?),
		"t" => {
			query.time = Some(match value {
				"hour" => ListingTime::Hour,
				"day" => ListingTime::Day,
				"week" => ListingTime::Week,
				"month" => ListingTime::Month,
				"year" => ListingTime::Year,
				"all" => ListingTime::All,
				_ => return Err(invalid("t", value)),
			})
		}
		"sr_detail" => query.sr_detail = Some(boolean("sr_detail", value)?),
		"g" => query.geo_filter = Some(value.to_string()),
		_ => return Ok(false),
	}
	Ok(true)
}

pub(super) fn comment_query(raw_query: Option<&str>) -> Result<CommentQuery, ApiError> {
	let mut query = CommentQuery::default();
	for (key, value) in pairs(raw_query) {
		match key.as_ref() {
			"comment" => query.comment = Some(value.into_owned()),
			"context" => query.context = Some(number("context", &value)?),
			"depth" => query.depth = Some(number("depth", &value)?),
			"limit" => query.limit = Some(number("limit", &value)?),
			"showedits" => query.showedits = Some(boolean("showedits", &value)?),
			"showmedia" => query.showmedia = Some(boolean("showmedia", &value)?),
			"showmore" => query.showmore = Some(boolean("showmore", &value)?),
			"showtitle" => query.showtitle = Some(boolean("showtitle", &value)?),
			"sort" => query.sort = Some(comment_sort(&value)?),
			"sr_detail" => query.sr_detail = Some(boolean("sr_detail", &value)?),
			"theme" => {
				query.theme = Some(match value.as_ref() {
					"default" => CommentTheme::Default,
					"dark" => CommentTheme::Dark,
					_ => return Err(invalid("theme", &value)),
				})
			}
			"threaded" => query.threaded = Some(boolean("threaded", &value)?),
			"truncate" => query.truncate = Some(number("truncate", &value)?),
			_ => {}
		}
	}
	Ok(query)
}

pub(super) fn more_children_query(raw_query: Option<&str>) -> Result<MoreChildrenQuery, ApiError> {
	let mut query = MoreChildrenQuery::default();
	for (key, value) in pairs(raw_query) {
		match key.as_ref() {
			"api_type" => {
				query.api_type = Some(match value.as_ref() {
					"json" => MoreChildrenApiType::Json,
					_ => return Err(invalid("api_type", &value)),
				})
			}
			"children" => query.children.extend(comma_values(&value).map(str::to_string)),
			"depth" => query.depth = Some(number("depth", &value)?),
			"id" => query.id = Some(value.into_owned()),
			"limit_children" => query.limit_children = Some(boolean("limit_children", &value)?),
			"link_id" => query.link_id = value.into_owned(),
			"sort" => query.sort = Some(comment_sort(&value)?),
			_ => {}
		}
	}
	Ok(query)
}

fn comment_sort(value: &str) -> Result<CommentSort, ApiError> {
	match value {
		"confidence" => Ok(CommentSort::Confidence),
		"top" => Ok(CommentSort::Top),
		"new" => Ok(CommentSort::New),
		"controversial" => Ok(CommentSort::Controversial),
		"old" => Ok(CommentSort::Old),
		"random" => Ok(CommentSort::Random),
		"qa" => Ok(CommentSort::Qa),
		"live" => Ok(CommentSort::Live),
		_ => Err(invalid("sort", value)),
	}
}

fn comma_values(value: &str) -> impl Iterator<Item = &str> {
	value.split(',').map(str::trim).filter(|value| !value.is_empty())
}

pub(super) fn duplicate_query(raw_query: Option<&str>) -> Result<DuplicateQuery, ApiError> {
	let mut query = DuplicateQuery {
		listing: listing_query(raw_query)?,
		..Default::default()
	};
	for (key, value) in pairs(raw_query) {
		match key.as_ref() {
			"crossposts_only" => query.crossposts_only = Some(boolean("crossposts_only", &value)?),
			"sort" => {
				query.sort = Some(match value.as_ref() {
					"num_comments" => DuplicateSort::NumComments,
					"new" => DuplicateSort::New,
					_ => return Err(invalid("sort", &value)),
				})
			}
			"sr" => query.subreddit = Some(value.into_owned()),
			_ => {}
		}
	}
	Ok(query)
}

pub(super) fn subreddit_search_query(raw_query: Option<&str>) -> Result<SubredditSearchQuery, ApiError> {
	let mut query = SubredditSearchQuery {
		listing: listing_query(raw_query)?,
		query: String::new(),
		search_query_id: None,
		show_users: None,
		sort: None,
		typeahead_active: None,
	};
	for (key, value) in pairs(raw_query) {
		match key.as_ref() {
			"q" => query.query = value.into_owned(),
			"search_query_id" => query.search_query_id = Some(value.into_owned()),
			"show_users" => query.show_users = Some(boolean("show_users", &value)?),
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

pub(super) fn pairs(raw_query: Option<&str>) -> form_urlencoded::Parse<'_> {
	form_urlencoded::parse(raw_query.unwrap_or_default().as_bytes())
}

pub(super) fn number<T>(parameter: &str, value: &str) -> Result<T, ApiError>
where
	T: FromStr,
{
	value.parse().map_err(|_| invalid(parameter, value))
}

pub(super) fn boolean(parameter: &str, value: &str) -> Result<bool, ApiError> {
	match value {
		"true" => Ok(true),
		"false" => Ok(false),
		_ => Err(invalid(parameter, value)),
	}
}

pub(super) fn invalid(parameter: &str, value: &str) -> ApiError {
	ApiError::InvalidQuery {
		parameter: parameter.to_string(),
		value: value.to_string(),
	}
}
