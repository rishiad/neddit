use axum::extract::{Query, State};
use neddit_api::{
	client::Access,
	service::{ListingQuery, PostSort, RedditService},
};
use serde::Deserialize;

use crate::{
	error::AppError,
	view::{feed_item, next_url, sort_links, FeedTemplate},
};

const PAGE_SIZE: u8 = 25;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct FeedQuery {
	sort: Option<String>,
	after: Option<String>,
	count: Option<u32>,
}

pub async fn front_page(State(service): State<RedditService>, Query(query): Query<FeedQuery>) -> Result<FeedTemplate, AppError> {
	let (sort, sort_name) = parse_sort(query.sort.as_deref())?;
	let count = query.count.unwrap_or(0);
	let listing = service
		.front_page_posts(
			sort,
			&ListingQuery {
				after: query.after,
				count: Some(count),
				limit: Some(PAGE_SIZE),
				..ListingQuery::default()
			},
			Access::Standard,
		)
		.await?;
	let items = listing.data.children.iter().map(|thing| feed_item(&thing.data)).collect();
	let next_url = next_url(sort_name, listing.data.after.as_deref(), count.saturating_add(u32::from(PAGE_SIZE)));

	Ok(FeedTemplate {
		items,
		sorts: sort_links(sort_name),
		has_next: !next_url.is_empty(),
		next_url,
	})
}

fn parse_sort(value: Option<&str>) -> Result<(PostSort, &'static str), AppError> {
	match value.unwrap_or("hot") {
		"hot" => Ok((PostSort::Hot, "hot")),
		"new" => Ok((PostSort::New, "new")),
		"rising" => Ok((PostSort::Rising, "rising")),
		"top" => Ok((PostSort::Top, "top")),
		_ => Err(AppError::InvalidSort),
	}
}

