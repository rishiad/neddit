use axum::{
	extract::{Path, Query, State},
	response::Redirect,
};
use neddit_api::{
	client::Access,
	media::MediaSigner,
	models::PostComments,
	server::MediaProxy,
	service::{CommentQuery, CommentSort, ListingQuery, ListingTime, MoreChildrenQuery, PostSort, RedditService, ThreadCommentSearchQuery, WikiPageQuery},
};
use serde::Deserialize;

use crate::{
	error::AppError,
	view::{
		comment_sort_controls, comment_tree, community_view, feed_controls, feed_item, feed_pagination, gallery_view, loaded_comment_tree, post_view, search_comment_tree,
		wiki_view, FeedTemplate, GalleryTemplate, MoreCommentsTemplate, PostContentTemplate, PostTemplate, SubredditTemplate, VideoPlayerTemplate, WikiTemplate,
	},
};

const PAGE_SIZE: u8 = 25;
const MORE_COMMENTS_BATCH_SIZE: usize = 100;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct FeedQuery {
	sort: Option<String>,
	t: Option<String>,
	after: Option<String>,
	before: Option<String>,
	page: Option<u32>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct PostQuery {
	sort: Option<String>,
	q: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MoreCommentsQuery {
	children: String,
	link_id: String,
	parent_id: String,
	sort: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct VideoPlayerQuery {
	url: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct WikiQuery {
	v: Option<String>,
	v2: Option<String>,
}

pub async fn front_page(State(service): State<RedditService>, Query(query): Query<FeedQuery>) -> Result<FeedTemplate, AppError> {
	let (sort, sort_name) = parse_sort(query.sort.as_deref())?;
	let (time, time_name) = parse_feed_time(sort, query.t.as_deref())?;
	let page = query.page.unwrap_or(1).max(1);
	let count = listing_count(page, query.before.is_some());
	let listing = service
		.front_page_posts(
			sort,
			&ListingQuery {
				after: query.after,
				before: query.before,
				count: Some(count),
				limit: Some(PAGE_SIZE),
				time,
				..ListingQuery::default()
			},
			Access::Standard,
		)
		.await?;
	let before = listing
		.data
		.before
		.as_deref()
		.or_else(|| listing.data.children.first().map(|thing| thing.data.name.as_str()));
	let pagination = feed_pagination("/", sort_name, time_name, before, listing.data.after.as_deref(), page);
	let items = listing.data.children.iter().map(|thing| feed_item(&thing.data)).collect();

	Ok(FeedTemplate {
		items,
		controls: feed_controls("/", sort_name, time_name, false),
		pagination,
		feed_label: "Front page posts".into(),
	})
}

pub async fn subreddit_feed(State(service): State<RedditService>, Path(subreddit): Path<String>, Query(query): Query<FeedQuery>) -> Result<SubredditTemplate, AppError> {
	let (sort, sort_name) = parse_subreddit_sort(query.sort.as_deref())?;
	let (time, time_name) = parse_feed_time(sort, query.t.as_deref())?;
	let page = query.page.unwrap_or(1).max(1);
	let count = listing_count(page, query.before.is_some());
	let listing_query = ListingQuery {
		after: query.after,
		before: query.before,
		count: Some(count),
		limit: Some(PAGE_SIZE),
		time,
		..ListingQuery::default()
	};
	let (listing, community) = tokio::try_join!(
		service.subreddit_posts(&subreddit, sort, &listing_query, Access::Standard),
		service.subreddit_about(&subreddit, Access::Standard),
	)?;
	let before = listing
		.data
		.before
		.as_deref()
		.or_else(|| listing.data.children.first().map(|thing| thing.data.name.as_str()));
	let pagination = feed_pagination(&format!("/r/{subreddit}"), sort_name, time_name, before, listing.data.after.as_deref(), page);
	let items = listing.data.children.iter().map(|thing| feed_item(&thing.data)).collect();

	Ok(SubredditTemplate {
		community: community_view(&community.data),
		items,
		controls: feed_controls(&format!("/r/{subreddit}"), sort_name, time_name, true),
		pagination,
		feed_label: format!("r/{subreddit} posts"),
	})
}

fn listing_count(page: u32, before: bool) -> u32 {
	let traversed_pages = if before { page } else { page.saturating_sub(1) };
	traversed_pages.saturating_mul(u32::from(PAGE_SIZE))
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

pub async fn post_comments(
	State(service): State<RedditService>,
	State(signer): State<MediaSigner>,
	Path(article): Path<String>,
	Query(query): Query<PostQuery>,
) -> Result<PostTemplate, AppError> {
	post_page(service, signer, None, article, None, query).await
}

pub async fn subreddit_post_comments(
	State(service): State<RedditService>,
	State(signer): State<MediaSigner>,
	Path((subreddit, article)): Path<(String, String)>,
	Query(query): Query<PostQuery>,
) -> Result<PostTemplate, AppError> {
	post_page(service, signer, Some(subreddit), article, None, query).await
}

pub async fn post_permalink(
	State(service): State<RedditService>,
	State(signer): State<MediaSigner>,
	Path((article, _slug)): Path<(String, String)>,
	Query(query): Query<PostQuery>,
) -> Result<PostTemplate, AppError> {
	post_page(service, signer, None, article, None, query).await
}

pub async fn post_comment_permalink(
	State(service): State<RedditService>,
	State(signer): State<MediaSigner>,
	Path((article, _slug, comment)): Path<(String, String, String)>,
	Query(query): Query<PostQuery>,
) -> Result<PostTemplate, AppError> {
	post_page(service, signer, None, article, Some(comment), query).await
}

pub async fn subreddit_post_permalink(
	State(service): State<RedditService>,
	State(signer): State<MediaSigner>,
	Path((subreddit, article, _slug)): Path<(String, String, String)>,
	Query(query): Query<PostQuery>,
) -> Result<PostTemplate, AppError> {
	post_page(service, signer, Some(subreddit), article, None, query).await
}

pub async fn subreddit_post_comment_permalink(
	State(service): State<RedditService>,
	State(signer): State<MediaSigner>,
	Path((subreddit, article, _slug, comment)): Path<(String, String, String, String)>,
	Query(query): Query<PostQuery>,
) -> Result<PostTemplate, AppError> {
	post_page(service, signer, Some(subreddit), article, Some(comment), query).await
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

pub async fn video_player(State(media): State<MediaProxy>, Query(query): Query<VideoPlayerQuery>) -> Result<VideoPlayerTemplate, neddit_api::video::VideoError> {
	Ok(VideoPlayerTemplate {
		playback: media.resolve_video(&query.url).await?,
	})
}

pub async fn gallery(
	State(service): State<RedditService>,
	State(signer): State<MediaSigner>,
	Path((article, index)): Path<(String, usize)>,
) -> Result<GalleryTemplate, AppError> {
	let post = service
		.posts_by_id(&format!("t3_{article}"), Access::Standard)
		.await?
		.data
		.children
		.into_iter()
		.next()
		.ok_or(AppError::PostNotFound)?
		.data;
	Ok(GalleryTemplate {
		gallery: gallery_view(&post, &signer, index).ok_or(AppError::PostNotFound)?,
	})
}

pub async fn post_content(State(service): State<RedditService>, State(signer): State<MediaSigner>, Path(article): Path<String>) -> Result<PostContentTemplate, AppError> {
	let post = service
		.posts_by_id(&format!("t3_{article}"), Access::Standard)
		.await?
		.data
		.children
		.into_iter()
		.next()
		.ok_or(AppError::PostNotFound)?
		.data;
	let mut post = post_view(&post, &signer);
	post.hide_content = false;
	Ok(PostContentTemplate { post })
}

async fn post_page(
	service: RedditService,
	signer: MediaSigner,
	subreddit: Option<String>,
	article: String,
	comment: Option<String>,
	query: PostQuery,
) -> Result<PostTemplate, AppError> {
	let (sort, sort_name) = parse_comment_sort(query.sort.as_deref())?;
	let search = query.q.map(|value| value.trim().to_owned()).filter(|value| !value.is_empty());
	if search.as_ref().is_some_and(|value| value.chars().count() > 512) {
		return Err(AppError::InvalidCommentSearch);
	}
	let comment_query = CommentQuery {
		comment,
		sort: Some(sort),
		..CommentQuery::default()
	};
	let (post, comment_tree, result_count) = if let Some(search) = &search {
		let post = service
			.posts_by_id(&format!("t3_{article}"), Access::Standard)
			.await?
			.data
			.children
			.into_iter()
			.next()
			.ok_or(AppError::PostNotFound)?
			.data;
		let comments = service
			.search_post_comments(&post.subreddit, &article, &ThreadCommentSearchQuery { query: search.clone(), sort }, Access::Standard)
			.await?;
		let count = comments.len();
		(post, search_comment_tree(&comments), count)
	} else {
		let PostComments(posts, comments) = match subreddit {
			Some(subreddit) => service.subreddit_post_comments(&subreddit, &article, &comment_query, Access::Standard).await?,
			None => service.post_comments(&article, &comment_query, Access::Standard).await?,
		};
		let post = posts.data.children.into_iter().next().ok_or(AppError::PostNotFound)?.data;
		let link_id = post.name.clone();
		let tree = comment_tree(&comments.data.children, &link_id, sort_name);
		(post, tree, 0)
	};
	let comment_count = post.num_comments;
	let post = post_view(&post, &signer);
	let search_query = search.as_deref().unwrap_or_default();

	Ok(PostTemplate {
		controls: comment_sort_controls(sort_name, &post.item.permalink, search_query),
		comments_heading: if search.is_some() {
			format!("{result_count} matching comment{}", if result_count == 1 { "" } else { "s" })
		} else {
			format!("{} comment{}", post.item.comments, if comment_count == 1 { "" } else { "s" })
		},
		empty_message: if search.is_some() { "No comments matched." } else { "No comments yet." },
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

fn parse_feed_time(sort: PostSort, value: Option<&str>) -> Result<(Option<ListingTime>, &'static str), AppError> {
	if sort != PostSort::Top {
		return if value.is_none() { Ok((None, "day")) } else { Err(AppError::InvalidFeedTime) };
	}

	match value.unwrap_or("day") {
		"hour" => Ok((Some(ListingTime::Hour), "hour")),
		"day" => Ok((Some(ListingTime::Day), "day")),
		"week" => Ok((Some(ListingTime::Week), "week")),
		"month" => Ok((Some(ListingTime::Month), "month")),
		"year" => Ok((Some(ListingTime::Year), "year")),
		"all" => Ok((Some(ListingTime::All), "all")),
		_ => Err(AppError::InvalidFeedTime),
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

