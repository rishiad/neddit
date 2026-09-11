use axum::{
	extract::{Path, Query, State},
	response::{IntoResponse, Redirect, Response},
};
use neddit_api::{
	client::Access,
	media::MediaSigner,
	models::{PostComments, PublicThing},
	server::MediaProxy,
	service::{
		CommentQuery, CommentSort, ListingQuery, ListingTime, MoreChildrenQuery, PostSort, RedditService, ServiceError, ThreadCommentSearchQuery, UserHistoryQuery,
		UserHistorySort, WikiPageQuery,
	},
	video::VideoSupport,
};
use serde::Deserialize;

use crate::{
	error::AppError,
	view::{
		comment_sort_controls, comment_tree, community_view, feed_controls, feed_item, feed_pagination, gallery_view, generated_wiki_index, has_public_wiki_pages, has_wiki_page,
		loaded_comment_tree, post_result, post_view, search_comment_tree, search_result, subreddit_feed_item, user_controls, user_profile, wiki_path, wiki_view, FeedTemplate,
		GalleryTemplate, MoreCommentsTemplate, PostContentTemplate, PostTemplate, SearchResultView, SubredditTemplate, UserTemplate, VideoPlayerTemplate, WikiTemplate,
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

pub async fn front_page(State(service): State<RedditService>, State(signer): State<MediaSigner>, Query(query): Query<FeedQuery>) -> Result<FeedTemplate, AppError> {
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
	let items = listing.data.children.iter().map(|thing| feed_item(&thing.data, &signer)).collect();

	Ok(FeedTemplate {
		items,
		controls: feed_controls("/", sort_name, time_name, false),
		pagination,
		feed_label: "Front page posts".into(),
	})
}

pub async fn subreddit_feed(
	State(service): State<RedditService>,
	State(signer): State<MediaSigner>,
	Path(subreddit): Path<String>,
	Query(query): Query<FeedQuery>,
) -> Result<SubredditTemplate, AppError> {
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
	let subreddit_is_nsfw = community.data.over18 == Some(true);
	let items = listing
		.data
		.children
		.iter()
		.map(|thing| subreddit_feed_item(&thing.data, &signer, subreddit_is_nsfw))
		.collect();

	let has_wiki = if community.data.wiki_enabled == Some(true) {
		service.wiki_pages(&subreddit, Access::Standard).await.is_ok_and(|pages| has_public_wiki_pages(&pages))
	} else {
		false
	};

	Ok(SubredditTemplate {
		community: community_view(&community.data, &signer, has_wiki),
		items,
		controls: feed_controls(&format!("/r/{subreddit}"), sort_name, time_name, true),
		pagination,
		feed_label: format!("r/{subreddit} posts"),
	})
}

#[derive(Clone, Copy)]
enum UserSection {
	Posts,
	Comments,
}

struct UserActivityPage {
	before: Option<String>,
	after: Option<String>,
	results: Vec<SearchResultView>,
}

pub async fn user_posts(
	state: State<RedditService>,
	signer: State<MediaSigner>,
	media: State<MediaProxy>,
	path: Path<String>,
	query: Query<FeedQuery>,
) -> Result<UserTemplate, AppError> {
	user_page(state, signer, media, path, query, UserSection::Posts).await
}

pub async fn user_comments(
	state: State<RedditService>,
	signer: State<MediaSigner>,
	media: State<MediaProxy>,
	path: Path<String>,
	query: Query<FeedQuery>,
) -> Result<UserTemplate, AppError> {
	user_page(state, signer, media, path, query, UserSection::Comments).await
}

async fn user_page(
	State(service): State<RedditService>,
	State(signer): State<MediaSigner>,
	State(media): State<MediaProxy>,
	Path(username): Path<String>,
	Query(query): Query<FeedQuery>,
	section: UserSection,
) -> Result<UserTemplate, AppError> {
	let (sort, sort_name) = parse_user_sort(query.sort.as_deref())?;
	let (time, time_name) = parse_user_time(sort, query.t.as_deref())?;
	let page = query.page.unwrap_or(1).max(1);
	let count = listing_count(page, query.before.is_some());
	let history_query = UserHistoryQuery {
		listing: ListingQuery {
			after: query.after,
			before: query.before,
			count: Some(count),
			limit: Some(PAGE_SIZE),
			time,
			..ListingQuery::default()
		},
		sort: Some(sort),
		..UserHistoryQuery::default()
	};
	let (activity, user) = tokio::try_join!(
		user_activity(&service, &signer, media.video_support(), &username, &history_query, section),
		service.user_about(&username, Access::Standard),
	)?;
	let posts_url = format!("/user/{username}");
	let comments_url = format!("/user/{username}/comments");
	let (base, posts_active, comments_active) = match section {
		UserSection::Posts => (&posts_url, true, false),
		UserSection::Comments => (&comments_url, false, true),
	};
	let pagination = feed_pagination(base, sort_name, time_name, activity.before.as_deref(), activity.after.as_deref(), page);

	Ok(UserTemplate {
		profile: user_profile(&user.data),
		results: activity.results,
		controls: user_controls(base, sort_name, time_name),
		posts_url,
		comments_url,
		posts_active,
		comments_active,
		pagination,
	})
}

async fn user_activity(
	service: &RedditService,
	signer: &MediaSigner,
	video_support: VideoSupport,
	username: &str,
	query: &UserHistoryQuery,
	section: UserSection,
) -> Result<UserActivityPage, ServiceError> {
	match section {
		UserSection::Posts => {
			let listing = service.user_submitted(username, query, Access::Standard).await?;
			let before = listing.data.before.clone().or_else(|| listing.data.children.first().map(|thing| thing.data.name.clone()));
			let results = listing.data.children.iter().map(|thing| post_result(&thing.data, signer, video_support)).collect();
			Ok(UserActivityPage {
				before,
				after: listing.data.after,
				results,
			})
		}
		UserSection::Comments => {
			let listing = service.user_comments(username, query, Access::Standard).await?;
			let before = listing
				.data
				.before
				.clone()
				.or_else(|| listing.data.children.first().map(public_fullname).map(str::to_owned));
			let results = listing.data.children.iter().filter_map(|item| search_result(item, signer, video_support)).collect();
			Ok(UserActivityPage {
				before,
				after: listing.data.after,
				results,
			})
		}
	}
}

fn public_fullname(item: &PublicThing) -> &str {
	match item {
		PublicThing::Comment(thing) => &thing.data.name,
		PublicThing::User(thing) => &thing.data.name,
		PublicThing::Post(thing) => &thing.data.name,
		PublicThing::Subreddit(thing) => &thing.data.name,
	}
}

fn listing_count(page: u32, before: bool) -> u32 {
	let traversed_pages = if before { page } else { page.saturating_sub(1) };
	traversed_pages.saturating_mul(u32::from(PAGE_SIZE))
}

pub async fn wiki_root(State(service): State<RedditService>, State(signer): State<MediaSigner>, Path(subreddit): Path<String>) -> Result<Response, AppError> {
	wiki_response(service, signer, subreddit, None, WikiQuery::default()).await
}

pub async fn wiki_page(
	State(service): State<RedditService>,
	State(signer): State<MediaSigner>,
	Path((subreddit, page)): Path<(String, String)>,
	Query(query): Query<WikiQuery>,
) -> Result<Response, AppError> {
	wiki_response(service, signer, subreddit, Some(page), query).await
}

async fn wiki_response(service: RedditService, signer: MediaSigner, subreddit: String, requested_page: Option<String>, query: WikiQuery) -> Result<Response, AppError> {
	let query = WikiPageQuery { v: query.v, v2: query.v2 };
	let (community, pages) = tokio::try_join!(service.subreddit_about(&subreddit, Access::Standard), service.wiki_pages(&subreddit, Access::Standard),)?;
	let has_wiki = has_public_wiki_pages(&pages);
	let community = community_view(&community.data, &signer, has_wiki);
	let requested_page = requested_page.map(|page| page.trim_end_matches('/').to_owned());

	let Some(page) = requested_page else {
		return Ok(if has_wiki {
			Redirect::temporary(&wiki_path(&subreddit, "index")).into_response()
		} else {
			crate::error::response(axum::http::StatusCode::NOT_FOUND)
		});
	};
	if page == "index" && has_wiki && !has_wiki_page(&pages, "index") {
		return Ok(
			WikiTemplate {
				community,
				wiki: generated_wiki_index(&subreddit, &pages),
			}
			.into_response(),
		);
	}

	if !has_wiki_page(&pages, &page) {
		return Ok(crate::error::response(axum::http::StatusCode::NOT_FOUND));
	}

	let wiki = service.wiki_page(&subreddit, &page, &query, Access::Standard).await?;

	Ok(
		WikiTemplate {
			community,
			wiki: wiki_view(&subreddit, &page, &wiki, &pages, &signer),
		}
		.into_response(),
	)
}

pub async fn post_comments(
	State(service): State<RedditService>,
	State(signer): State<MediaSigner>,
	State(media): State<MediaProxy>,
	Path(article): Path<String>,
	Query(query): Query<PostQuery>,
) -> Result<PostTemplate, AppError> {
	post_page(service, signer, media.video_support(), None, article, None, query).await
}

pub async fn subreddit_post_comments(
	State(service): State<RedditService>,
	State(signer): State<MediaSigner>,
	State(media): State<MediaProxy>,
	Path((subreddit, article)): Path<(String, String)>,
	Query(query): Query<PostQuery>,
) -> Result<PostTemplate, AppError> {
	post_page(service, signer, media.video_support(), Some(subreddit), article, None, query).await
}

pub async fn post_permalink(
	State(service): State<RedditService>,
	State(signer): State<MediaSigner>,
	State(media): State<MediaProxy>,
	Path((article, _slug)): Path<(String, String)>,
	Query(query): Query<PostQuery>,
) -> Result<PostTemplate, AppError> {
	post_page(service, signer, media.video_support(), None, article, None, query).await
}

pub async fn post_comment_permalink(
	State(service): State<RedditService>,
	State(signer): State<MediaSigner>,
	State(media): State<MediaProxy>,
	Path((article, _slug, comment)): Path<(String, String, String)>,
	Query(query): Query<PostQuery>,
) -> Result<PostTemplate, AppError> {
	post_page(service, signer, media.video_support(), None, article, Some(comment), query).await
}

pub async fn subreddit_post_permalink(
	State(service): State<RedditService>,
	State(signer): State<MediaSigner>,
	State(media): State<MediaProxy>,
	Path((subreddit, article, _slug)): Path<(String, String, String)>,
	Query(query): Query<PostQuery>,
) -> Result<PostTemplate, AppError> {
	post_page(service, signer, media.video_support(), Some(subreddit), article, None, query).await
}

pub async fn subreddit_post_comment_permalink(
	State(service): State<RedditService>,
	State(signer): State<MediaSigner>,
	State(media): State<MediaProxy>,
	Path((subreddit, article, _slug, comment)): Path<(String, String, String, String)>,
	Query(query): Query<PostQuery>,
) -> Result<PostTemplate, AppError> {
	post_page(service, signer, media.video_support(), Some(subreddit), article, Some(comment), query).await
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

pub async fn post_content(
	State(service): State<RedditService>,
	State(signer): State<MediaSigner>,
	State(media): State<MediaProxy>,
	Path(article): Path<String>,
) -> Result<PostContentTemplate, AppError> {
	let post = service
		.posts_by_id(&format!("t3_{article}"), Access::Standard)
		.await?
		.data
		.children
		.into_iter()
		.next()
		.ok_or(AppError::PostNotFound)?
		.data;
	let mut post = post_view(&post, &signer, media.video_support());
	post.hide_content = false;
	Ok(PostContentTemplate { post })
}

async fn post_page(
	service: RedditService,
	signer: MediaSigner,
	video_support: VideoSupport,
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
	let post = post_view(&post, &signer, video_support);
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

fn parse_user_sort(value: Option<&str>) -> Result<(UserHistorySort, &'static str), AppError> {
	match value.unwrap_or("new") {
		"new" => Ok((UserHistorySort::New, "new")),
		"hot" => Ok((UserHistorySort::Hot, "hot")),
		"top" => Ok((UserHistorySort::Top, "top")),
		"controversial" => Ok((UserHistorySort::Controversial, "controversial")),
		_ => Err(AppError::InvalidSort),
	}
}

fn parse_user_time(sort: UserHistorySort, value: Option<&str>) -> Result<(Option<ListingTime>, &'static str), AppError> {
	if sort != UserHistorySort::Top {
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

