use axum::{
	extract::{Path, Query, State},
	response::Redirect,
};
use neddit_api::{
	client::Access,
	models::PostComments,
	service::{CommentQuery, CommentSort, ListingQuery, ListingTime, MoreChildrenQuery, PostSort, RedditService, SearchQuery, SearchResultType, SearchSort, WikiPageQuery},
};
use serde::Deserialize;
use url::form_urlencoded;

use crate::{
	error::AppError,
	view::{
		comment_sort_links, comment_tree, community_view, feed_item, loaded_comment_tree, next_url, post_view, search_choices, search_result, sort_links, subreddit_next_url,
		subreddit_sort_links, wiki_view, FeedTemplate, MoreCommentsTemplate, PostTemplate, SearchTemplate, SubredditTemplate, WikiTemplate,
	},
};

const PAGE_SIZE: u8 = 25;
const MORE_COMMENTS_BATCH_SIZE: usize = 100;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct FeedQuery {
	sort: Option<String>,
	after: Option<String>,
	count: Option<u32>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct PostQuery {
	sort: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MoreCommentsQuery {
	children: String,
	link_id: String,
	parent_id: String,
	sort: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct WikiQuery {
	v: Option<String>,
	v2: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct SearchPageQuery {
	q: Option<String>,
	kind: Option<String>,
	sort: Option<String>,
	time: Option<String>,
	community: Option<String>,
	limit: Option<u8>,
	after: Option<String>,
	count: Option<u32>,
}

#[derive(Clone, Debug)]
struct SearchState {
	query: String,
	kind: SearchResultType,
	kind_name: &'static str,
	sort: SearchSort,
	sort_name: &'static str,
	time: ListingTime,
	time_name: &'static str,
	community: String,
	limit: u8,
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
		feed_label: "Front page posts".into(),
	})
}

pub async fn search_page(State(service): State<RedditService>, Query(query): Query<SearchPageQuery>) -> Result<SearchTemplate, AppError> {
	let after = query.after.clone();
	let count = query.count.unwrap_or(0);
	let state = parse_search_state(&query)?;
	let (kinds, sorts, times, limits) = search_choices(state.kind_name, state.sort_name, state.time_name, state.limit);
	let advanced_open = !state.community.is_empty() || state.time != ListingTime::All || state.limit != PAGE_SIZE;
	if state.query.is_empty() {
		return Ok(SearchTemplate {
			query: state.query,
			community: state.community,
			kinds,
			sorts,
			times,
			limits,
			results: Vec::new(),
			result_count: 0,
			searched: false,
			advanced_open,
			next_url: String::new(),
			has_next: false,
		});
	}

	let search_query = SearchQuery {
		listing: ListingQuery {
			after,
			count: Some(count),
			limit: Some(state.limit),
			time: Some(state.time),
			..ListingQuery::default()
		},
		query: state.query.clone(),
		restrict_sr: (!state.community.is_empty() && state.kind == SearchResultType::Post).then_some(true),
		sort: Some(state.sort),
		result_types: vec![state.kind],
		..SearchQuery::default()
	};
	let listing = if state.community.is_empty() || state.kind != SearchResultType::Post {
		service.search(&search_query, Access::Standard).await?
	} else {
		service.search_subreddit(&state.community, &search_query, Access::Standard).await?
	};
	let results: Vec<_> = listing.data.children.iter().filter_map(search_result).collect();
	let next_url = search_next_url(&state, listing.data.after.as_deref(), count.saturating_add(u32::from(state.limit)));

	Ok(SearchTemplate {
		query: state.query,
		community: state.community,
		kinds,
		sorts,
		times,
		limits,
		result_count: results.len(),
		results,
		searched: true,
		advanced_open,
		has_next: !next_url.is_empty(),
		next_url,
	})
}

pub async fn subreddit_feed(State(service): State<RedditService>, Path(subreddit): Path<String>, Query(query): Query<FeedQuery>) -> Result<SubredditTemplate, AppError> {
	let (sort, sort_name) = parse_subreddit_sort(query.sort.as_deref())?;
	let count = query.count.unwrap_or(0);
	let listing_query = ListingQuery {
		after: query.after,
		count: Some(count),
		limit: Some(PAGE_SIZE),
		..ListingQuery::default()
	};
	let (listing, community) = tokio::try_join!(
		service.subreddit_posts(&subreddit, sort, &listing_query, Access::Standard),
		service.subreddit_about(&subreddit, Access::Standard),
	)?;
	let items = listing.data.children.iter().map(|thing| feed_item(&thing.data)).collect();
	let next_url = subreddit_next_url(&subreddit, sort_name, listing.data.after.as_deref(), count.saturating_add(u32::from(PAGE_SIZE)));

	Ok(SubredditTemplate {
		community: community_view(&community.data),
		items,
		sorts: subreddit_sort_links(sort_name, &subreddit),
		has_next: !next_url.is_empty(),
		next_url,
		feed_label: format!("r/{subreddit} posts"),
	})
}

pub async fn wiki_root(Path(subreddit): Path<String>) -> Redirect {
	Redirect::permanent(&format!("/r/{subreddit}/wiki/index"))
}

pub async fn wiki_page(
	State(service): State<RedditService>,
	Path((subreddit, page)): Path<(String, String)>,
	Query(query): Query<WikiQuery>,
) -> Result<WikiTemplate, AppError> {
	let query = WikiPageQuery { v: query.v, v2: query.v2 };
	let (community, wiki, pages) = tokio::try_join!(
		service.subreddit_about(&subreddit, Access::Standard),
		service.wiki_page(&subreddit, &page, &query, Access::Standard),
		service.wiki_pages(&subreddit, Access::Standard),
	)?;

	Ok(WikiTemplate {
		community: community_view(&community.data),
		wiki: wiki_view(&subreddit, &page, &wiki, &pages),
	})
}

pub async fn post_comments(State(service): State<RedditService>, Path(article): Path<String>, Query(query): Query<PostQuery>) -> Result<PostTemplate, AppError> {
	post_page(service, None, article, None, query).await
}

pub async fn subreddit_post_comments(
	State(service): State<RedditService>,
	Path((subreddit, article)): Path<(String, String)>,
	Query(query): Query<PostQuery>,
) -> Result<PostTemplate, AppError> {
	post_page(service, Some(subreddit), article, None, query).await
}

pub async fn post_permalink(
	State(service): State<RedditService>,
	Path((article, _slug)): Path<(String, String)>,
	Query(query): Query<PostQuery>,
) -> Result<PostTemplate, AppError> {
	post_page(service, None, article, None, query).await
}

pub async fn post_comment_permalink(
	State(service): State<RedditService>,
	Path((article, _slug, comment)): Path<(String, String, String)>,
	Query(query): Query<PostQuery>,
) -> Result<PostTemplate, AppError> {
	post_page(service, None, article, Some(comment), query).await
}

pub async fn subreddit_post_permalink(
	State(service): State<RedditService>,
	Path((subreddit, article, _slug)): Path<(String, String, String)>,
	Query(query): Query<PostQuery>,
) -> Result<PostTemplate, AppError> {
	post_page(service, Some(subreddit), article, None, query).await
}

pub async fn subreddit_post_comment_permalink(
	State(service): State<RedditService>,
	Path((subreddit, article, _slug, comment)): Path<(String, String, String, String)>,
	Query(query): Query<PostQuery>,
) -> Result<PostTemplate, AppError> {
	post_page(service, Some(subreddit), article, Some(comment), query).await
}

pub async fn more_comments(State(service): State<RedditService>, Query(query): Query<MoreCommentsQuery>) -> Result<MoreCommentsTemplate, AppError> {
	let (sort, sort_name) = parse_comment_sort(query.sort.as_deref())?;
	let mut children: Vec<String> = query.children.split(',').filter(|child| !child.is_empty()).map(str::to_owned).collect();
	let remaining = if children.len() > MORE_COMMENTS_BATCH_SIZE {
		children.split_off(MORE_COMMENTS_BATCH_SIZE)
	} else {
		Vec::new()
	};
	let response = service
		.more_children(
			&MoreChildrenQuery {
				children,
				link_id: query.link_id.clone(),
				limit_children: Some(false),
				sort: Some(sort),
				..MoreChildrenQuery::default()
			},
			Access::Standard,
		)
		.await?;

	Ok(MoreCommentsTemplate {
		comment_tree: loaded_comment_tree(&response.json.data.things, &query.parent_id, &query.link_id, sort_name, &remaining),
	})
}

async fn post_page(service: RedditService, subreddit: Option<String>, article: String, comment: Option<String>, query: PostQuery) -> Result<PostTemplate, AppError> {
	let (sort, sort_name) = parse_comment_sort(query.sort.as_deref())?;
	let query = CommentQuery {
		comment,
		sort: Some(sort),
		..CommentQuery::default()
	};
	let PostComments(posts, comments) = match subreddit {
		Some(subreddit) => service.subreddit_post_comments(&subreddit, &article, &query, Access::Standard).await?,
		None => service.post_comments(&article, &query, Access::Standard).await?,
	};
	let post = posts.data.children.into_iter().next().ok_or(AppError::PostNotFound)?.data;
	let comment_count = post.num_comments;
	let link_id = post.name.clone();
	let post = post_view(&post);
	let comment_tree = comment_tree(&comments.data.children, &link_id, sort_name);

	Ok(PostTemplate {
		sorts: comment_sort_links(sort_name, &post.item.permalink),
		comments_heading: format!("{} comment{}", post.item.comments, if comment_count == 1 { "" } else { "s" }),
		has_comments: !comment_tree.is_empty(),
		post,
		comment_tree,
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

fn parse_subreddit_sort(value: Option<&str>) -> Result<(PostSort, &'static str), AppError> {
	match value.unwrap_or("hot") {
		"hot" => Ok((PostSort::Hot, "hot")),
		"new" => Ok((PostSort::New, "new")),
		"rising" => Ok((PostSort::Rising, "rising")),
		"top" => Ok((PostSort::Top, "top")),
		"controversial" => Ok((PostSort::Controversial, "controversial")),
		_ => Err(AppError::InvalidSort),
	}
}

fn parse_comment_sort(value: Option<&str>) -> Result<(CommentSort, &'static str), AppError> {
	match value.unwrap_or("best") {
		"best" | "confidence" => Ok((CommentSort::Confidence, "best")),
		"top" => Ok((CommentSort::Top, "top")),
		"new" => Ok((CommentSort::New, "new")),
		"old" => Ok((CommentSort::Old, "old")),
		"controversial" => Ok((CommentSort::Controversial, "controversial")),
		_ => Err(AppError::InvalidCommentSort),
	}
}

fn parse_search_state(query: &SearchPageQuery) -> Result<SearchState, AppError> {
	let (kind, kind_name) = match query.kind.as_deref().unwrap_or("posts") {
		"posts" => (SearchResultType::Post, "posts"),
		"communities" => (SearchResultType::Subreddit, "communities"),
		_ => return Err(AppError::InvalidSearch),
	};
	let (sort, sort_name) = match query.sort.as_deref().unwrap_or("relevance") {
		"relevance" => (SearchSort::Relevance, "relevance"),
		"new" => (SearchSort::New, "new"),
		"top" => (SearchSort::Top, "top"),
		"hot" => (SearchSort::Hot, "hot"),
		"comments" => (SearchSort::Comments, "comments"),
		_ => return Err(AppError::InvalidSearch),
	};
	let (time, time_name) = match query.time.as_deref().unwrap_or("all") {
		"all" => (ListingTime::All, "all"),
		"hour" => (ListingTime::Hour, "hour"),
		"day" => (ListingTime::Day, "day"),
		"week" => (ListingTime::Week, "week"),
		"month" => (ListingTime::Month, "month"),
		"year" => (ListingTime::Year, "year"),
		_ => return Err(AppError::InvalidSearch),
	};
	let limit = query.limit.unwrap_or(PAGE_SIZE);
	if ![10, 25, 50, 100].contains(&limit) {
		return Err(AppError::InvalidSearch);
	}
	let community = query.community.as_deref().unwrap_or_default().trim().trim_start_matches("r/").trim_matches('/').to_owned();
	Ok(SearchState {
		query: query.q.as_deref().unwrap_or_default().trim().to_owned(),
		kind,
		kind_name,
		sort,
		sort_name,
		time,
		time_name,
		community,
		limit,
	})
}

fn search_next_url(state: &SearchState, after: Option<&str>, count: u32) -> String {
	let Some(after) = after else {
		return String::new();
	};
	let mut query = form_urlencoded::Serializer::new(String::new());
	query.append_pair("q", &state.query);
	query.append_pair("kind", state.kind_name);
	query.append_pair("sort", state.sort_name);
	query.append_pair("time", state.time_name);
	query.append_pair("limit", &state.limit.to_string());
	if !state.community.is_empty() {
		query.append_pair("community", &state.community);
	}
	query.append_pair("after", after);
	query.append_pair("count", &count.to_string());
	format!("/search?{}", query.finish())
}

