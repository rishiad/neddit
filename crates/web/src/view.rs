use std::{
	borrow::Cow,
	collections::HashSet,
	time::{SystemTime, UNIX_EPOCH},
};

use ammonia::{Builder, UrlRelative};
use askama::Template;
use askama_web::WebTemplate;

use neddit_api::media::{rewrite_reddit_navigation, MediaSigner};
use neddit_api::models::{Comment, CommentChild, CommentReplies, More, Post, PublicThing, Subreddit, WikiPage, WikiPageListing};
use neddit_api::video::{VideoPlayback, VideoResolver};
use url::{form_urlencoded, Url};

#[derive(Clone, Debug)]
pub struct FeedItem {
	pub title: String,
	pub href: String,
	pub domain: String,
	pub show_domain: bool,
	pub author: String,
	pub subreddit: String,
	pub permalink: String,
	pub age: String,
	pub score: String,
	pub comments: String,
	pub stickied: bool,
	pub badges: Vec<&'static str>,
	pub video: Option<VideoView>,
}

#[derive(Clone, Debug)]
pub struct VideoView {
	pub player_url: String,
	pub poster: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SelectChoice {
	pub value: &'static str,
	pub label: &'static str,
	pub checked: bool,
}

#[derive(Clone, Debug)]
pub struct SortControls {
	pub action: String,
	pub label: &'static str,
	pub choice_label: &'static str,
	pub active_sort: &'static str,
	pub query: String,
	pub sorts: Vec<SelectChoice>,
	pub times: Vec<SelectChoice>,
}

#[derive(Template, WebTemplate)]
#[template(path = "feed.html")]
pub struct FeedTemplate {
	pub items: Vec<FeedItem>,
	pub controls: SortControls,
	pub pagination: Pagination,
	pub feed_label: String,
}

#[derive(Clone, Debug)]
pub struct Pagination {
	pub page: u32,
	pub previous_url: String,
	pub has_previous: bool,
	pub next_url: String,
	pub has_next: bool,
}

#[derive(Clone, Debug)]
pub struct SearchResultView {
	pub fullname: String,
	pub metric: String,
	pub metric_label: String,
	pub title: String,
	pub href: String,
	pub domain: Option<String>,
	pub summary: Option<String>,
	pub metadata: Vec<SearchMetadata>,
}

#[derive(Clone, Debug)]
pub struct SearchMetadata {
	pub text: String,
	pub href: Option<String>,
}

#[derive(Template, WebTemplate)]
#[template(path = "search.html")]
pub struct SearchTemplate {
	pub query: String,
	pub include_nsfw: bool,
	pub kinds: Vec<SelectChoice>,
	pub sorts: Vec<SelectChoice>,
	pub times: Vec<SelectChoice>,
	pub limits: Vec<SelectChoice>,
	pub results: Vec<SearchResultView>,
	pub result_count: usize,
	pub searched: bool,
	pub diagnostic: String,
	pub error_prefix: String,
	pub error_text: String,
	pub error_suffix: String,
	pub pagination: Pagination,
}

#[derive(Clone, Debug)]
pub struct CommunityView {
	pub display_name_prefixed: String,
	pub title: String,
	pub public_description: Option<String>,
	pub description: Option<String>,
	pub description_html: Option<String>,
	pub members: String,
	pub active: String,
	pub posts_url: String,
	pub wiki_url: String,
	pub has_wiki: bool,
	pub over_18: bool,
}

#[derive(Template, WebTemplate)]
#[template(path = "subreddit.html")]
pub struct SubredditTemplate {
	pub community: CommunityView,
	pub items: Vec<FeedItem>,
	pub controls: SortControls,
	pub pagination: Pagination,
	pub feed_label: String,
}

#[derive(Clone, Debug)]
pub struct WikiLink {
	pub label: String,
	pub href: String,
	pub active: bool,
}

#[derive(Clone, Debug)]
pub struct WikiView {
	pub title: String,
	pub content_html: String,
	pub revision_age: String,
	pub show_revision: bool,
	pub links: Vec<WikiLink>,
}

#[derive(Template, WebTemplate)]
#[template(path = "wiki.html")]
pub struct WikiTemplate {
	pub community: CommunityView,
	pub wiki: WikiView,
}

#[derive(Clone, Debug)]
pub struct PostView {
	pub item: FeedItem,
	pub gallery: Option<GalleryView>,
	pub image_url: Option<String>,
	pub selftext: String,
	pub show_selftext: bool,
	pub hide_content: bool,
	pub content_url: String,
}

#[derive(Clone, Debug)]
pub struct GalleryView {
	pub image: GalleryImageView,
	pub position: usize,
	pub total: usize,
	pub has_previous: bool,
	pub previous_url: String,
	pub has_next: bool,
	pub next_url: String,
}

#[derive(Clone, Debug)]
pub struct GalleryImageView {
	pub url: String,
	pub alt: String,
	pub caption: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CommentView {
	pub id: String,
	pub author: String,
	pub body: String,
	pub score: String,
	pub age: String,
	pub permalink: String,
	pub more_url: String,
	pub more: bool,
}

#[derive(Clone, Debug)]
pub enum CommentTreeEvent {
	Open(CommentView),
	OpenReplies,
	CloseReplies,
	Close,
}

#[derive(Template, WebTemplate)]
#[template(path = "post.html")]
pub struct PostTemplate {
	pub post: PostView,
	pub comment_tree: Vec<CommentTreeEvent>,
	pub controls: SortControls,
	pub comments_heading: String,
	pub empty_message: &'static str,
	pub has_comments: bool,
}

#[derive(Template, WebTemplate)]
#[template(path = "partials/gallery.html")]
pub struct GalleryTemplate {
	pub gallery: GalleryView,
}

#[derive(Template, WebTemplate)]
#[template(path = "partials/post_content.html")]
pub struct PostContentTemplate {
	pub post: PostView,
}

#[derive(Template, WebTemplate)]
#[template(path = "partials/video_player.html")]
pub struct VideoPlayerTemplate {
	pub playback: VideoPlayback,
}

#[derive(Template, WebTemplate)]
#[template(path = "more_comments.html")]
pub struct MoreCommentsTemplate {
	pub comment_tree: Vec<CommentTreeEvent>,
}

pub fn feed_item(post: &Post) -> FeedItem {
	let permalink = local_web_navigation(&post.permalink).unwrap_or_else(|| post.permalink.clone());
	let href = if post.is_self {
		permalink.clone()
	} else {
		safe_outbound(&post.url).unwrap_or_else(|| permalink.clone())
	};
	let domain = outbound_domain(&href).unwrap_or_default();
	let mut badges = Vec::with_capacity(4);
	if post.stickied {
		badges.push("pinned");
	}
	if post.over_18 {
		badges.push("NSFW");
	}
	if post.spoiler {
		badges.push("spoiler");
	}
	if post.locked {
		badges.push("locked");
	}

	FeedItem {
		title: if post.title.is_empty() { "untitled".into() } else { post.title.clone() },
		href,
		show_domain: !domain.is_empty(),
		domain,
		author: display_author(&post.author),
		subreddit: post.subreddit.clone(),
		permalink,
		age: age(post.created_utc),
		score: if post.hide_score { "—".into() } else { compact(post.score.max(0).unsigned_abs()) },
		comments: compact(post.num_comments),
		stickied: post.stickied,
		badges,
		video: None,
	}
}

pub fn feed_item_with_media(post: &Post, signer: &MediaSigner) -> FeedItem {
	let mut item = feed_item(post);
	if let Some(rewritten) = post_media_url(post, signer).filter(|_| !post.spoiler) {
		item.href = rewritten;
		item.domain.clear();
		item.show_domain = false;
	}
	if VideoResolver::supports(&post.url) {
		let mut query = form_urlencoded::Serializer::new(String::new());
		query.append_pair("url", &post.url);
		item.video = Some(VideoView {
			player_url: format!("/video/player?{}", query.finish()),
			poster: preview_url(post).and_then(|url| {
				let rewritten = signer.rewrite_text(url);
				rewritten.starts_with("/media/").then_some(rewritten)
			}),
		});
	}
	item
}

fn post_media_url(post: &Post, signer: &MediaSigner) -> Option<String> {
	let rewritten = signer.rewrite_text(&post.url);
	rewritten.starts_with("/media/").then_some(rewritten)
}

fn preview_url(post: &Post) -> Option<&str> {
	let image = post.extra.get("preview")?.get("images")?.as_array()?.first()?;
	let preview = if post.spoiler { image.pointer("/variants/obfuscated").unwrap_or(image) } else { image };
	let sources = preview
		.get("resolutions")
		.and_then(|value| value.as_array())
		.into_iter()
		.flatten()
		.chain(preview.get("source"));
	let best = sources
		.filter_map(|source| Some((source.get("width")?.as_u64()?, source.get("url")?.as_str()?)))
		.min_by_key(|(width, _)| width.abs_diff(640))
		.map(|(_, url)| url);
	best.or_else(|| post.extra.get("thumbnail")?.as_str().filter(|url| url.starts_with("https://") || url.starts_with("//")))
}

pub fn search_result(item: &PublicThing, signer: &MediaSigner) -> Option<SearchResultView> {
	match item {
		PublicThing::Post(thing) => {
			let item = feed_item_with_media(&thing.data, signer);
			Some(SearchResultView {
				fullname: thing.data.name.clone(),
				metric: item.score,
				metric_label: "points".into(),
				title: item.title,
				href: item.href,
				domain: item.show_domain.then_some(item.domain),
				summary: None,
				metadata: vec![
					linked_metadata(format!("by {}", item.author), reddit_user_url(&item.author)),
					plain_metadata(item.age),
					linked_metadata(format!("{} comments", item.comments), item.permalink),
					linked_metadata(format!("in r/{}", thing.data.subreddit), format!("/r/{}", thing.data.subreddit)),
				],
			})
		}
		PublicThing::Subreddit(thing) => {
			let subreddit = &thing.data;
			Some(SearchResultView {
				fullname: subreddit.name.clone(),
				metric: subreddit.subscribers.map_or_else(|| "—".into(), compact),
				metric_label: "members".into(),
				title: subreddit.display_name_prefixed.clone(),
				href: format!("/r/{}", subreddit.display_name),
				domain: None,
				summary: nonempty(subreddit.public_description.trim()),
				metadata: subreddit
					.accounts_active
					.map(|active| linked_metadata(format!("{} active", compact(active)), format!("/r/{}", subreddit.display_name)))
					.into_iter()
					.collect(),
			})
		}
		PublicThing::Comment(thing) => {
			let c = &thing.data;
			let mut metadata = vec![linked_metadata(format!("by {}", c.author), reddit_user_url(&c.author)), plain_metadata(age(c.created_utc))];
			if let Some(subreddit) = c.extra.get("subreddit").and_then(|value| value.as_str()) {
				metadata.push(linked_metadata(format!("in r/{subreddit}"), format!("/r/{subreddit}")));
			}
			Some(SearchResultView {
				fullname: c.name.clone(),
				metric: c.score.to_string(),
				metric_label: "points".into(),
				title: c.body.chars().take(160).collect(),
				href: format!("/comments/{}/_/{}", c.link_id.trim_start_matches("t3_"), c.id),
				domain: None,
				summary: None,
				metadata,
			})
		}
		PublicThing::User(_) => None,
	}
}

fn linked_metadata(text: String, href: String) -> SearchMetadata {
	SearchMetadata { text, href: Some(href) }
}

fn plain_metadata(text: String) -> SearchMetadata {
	SearchMetadata { text, href: None }
}

fn reddit_user_url(author: &str) -> String {
	let mut url = Url::parse("https://www.reddit.com").expect("static Reddit URL is valid");
	url.path_segments_mut().expect("HTTPS URLs support path segments").extend(["user", author]);
	url.to_string()
}

pub fn search_choices(active_kind: &str, active_sort: &str, active_limit: u8) -> (Vec<SelectChoice>, Vec<SelectChoice>, Vec<SelectChoice>) {
	use neddit_api::search::Mode;
	let choices = |values: &[(&'static str, &'static str)], active: &str| {
		values
			.iter()
			.map(|&(value, label)| SelectChoice {
				value,
				label,
				checked: value == active,
			})
			.collect()
	};
	let mut sorts: Vec<SelectChoice> = choices(
		&[("relevance", "Relevance"), ("new", "New"), ("top", "Top"), ("hot", "Hot"), ("activity", "Activity")],
		active_sort,
	);
	let enabled = [Mode::Posts, Mode::Comments, Mode::Communities]
		.into_iter()
		.find(|mode| mode.as_str() == active_kind)
		.map(Mode::sorts)
		.unwrap_or_default();
	sorts.retain(|choice| enabled.contains(&choice.value));
	let selected = sorts.iter().find(|s| s.checked).or_else(|| sorts.first()).map(|s| s.value);
	for choice in &mut sorts {
		choice.checked = Some(choice.value) == selected;
	}
	(
		choices(&[("posts", "Posts"), ("comments", "Comments"), ("communities", "Communities")], active_kind),
		sorts,
		choices(&[("25", "25"), ("50", "50"), ("100", "100")], &active_limit.to_string()),
	)
}

pub fn community_view(subreddit: &Subreddit) -> CommunityView {
	let title = if subreddit.title.trim().is_empty() {
		subreddit.display_name_prefixed.clone()
	} else {
		subreddit.title.trim().to_owned()
	};
	let public_description = nonempty(subreddit.public_description.trim());
	let description = subreddit.description.as_deref().and_then(|s| nonempty(s.trim()));
	let description_html = subreddit.description_html.as_deref().map(sanitize_html).and_then(|html| nonempty(&html));
	let posts_url = format!("/r/{}", subreddit.display_name);
	CommunityView {
		display_name_prefixed: subreddit.display_name_prefixed.clone(),
		title,
		public_description,
		description,
		description_html,
		members: subreddit.subscribers.map_or_else(|| "—".into(), compact),
		active: subreddit.accounts_active.map_or_else(|| "—".into(), compact),
		wiki_url: format!("{posts_url}/wiki/index"),
		posts_url,
		has_wiki: subreddit.wiki_enabled.unwrap_or(false),
		over_18: subreddit.over18 == Some(true),
	}
}

pub fn wiki_view(subreddit: &str, page: &str, wiki: &WikiPage, pages: &WikiPageListing) -> WikiView {
	let title = if page == "index" { "Wiki".into() } else { page.replace(['_', '-'], " ") };
	let mut links: Vec<_> = pages
		.data
		.iter()
		.filter(|label| !label.starts_with("config/"))
		.map(|label| WikiLink {
			label: if label == "index" { "Home".into() } else { label.replace('_', " ") },
			href: wiki_path(subreddit, label),
			active: label == page,
		})
		.collect();
	links.sort_by_key(|link| (link.label != "Home", link.label.clone()));
	WikiView {
		title,
		content_html: sanitize_html(&wiki.data.content_html),
		revision_age: wiki.data.revision_date.map_or_else(String::new, age),
		show_revision: wiki.data.revision_date.is_some(),
		links,
	}
}

fn sanitize_html(value: &str) -> String {
	Builder::new()
		.url_relative(UrlRelative::PassThrough)
		.attribute_filter(|element, attribute, value| {
			if element == "a" && attribute == "href" {
				return Some(local_web_navigation(value).map_or_else(|| Cow::Borrowed(value), Cow::Owned));
			}
			Some(Cow::Borrowed(value))
		})
		.clean(value)
		.to_string()
}

fn nonempty(value: &str) -> Option<String> {
	(!value.is_empty()).then(|| value.to_owned())
}

pub fn post_view(post: &Post, signer: &MediaSigner) -> PostView {
	let item = feed_item_with_media(post, signer);
	let gallery = gallery_view(post, signer, 0);
	let image_url = if gallery.is_none() && item.video.is_none() && is_image_post(post) {
		post_media_url(post, signer)
	} else {
		None
	};
	let selftext = post.selftext.trim().to_owned();
	PostView {
		show_selftext: !selftext.is_empty(),
		item,
		gallery,
		image_url,
		selftext,
		hide_content: post.spoiler,
		content_url: format!("/post-content/{}", post.id),
	}
}

pub fn gallery_view(post: &Post, signer: &MediaSigner, index: usize) -> Option<GalleryView> {
	let metadata = post.extra.get("media_metadata")?.as_object()?;
	let images: Vec<_> = post
		.extra
		.get("gallery_data")?
		.get("items")?
		.as_array()?
		.iter()
		.filter_map(|item| {
			let media_id = item.get("media_id")?.as_str()?;
			let media = metadata.get(media_id)?;
			let source = media.get("s")?;
			let upstream = source
				.get("gif")
				.or_else(|| source.get("u"))
				.and_then(|value| value.as_str())
				.or_else(|| media.get("p")?.as_array()?.last()?.get("u")?.as_str())?;
			let url = signer.rewrite_text(upstream);
			if !url.starts_with("/media/") {
				return None;
			}
			let caption = item
				.get("caption")
				.and_then(|value| value.as_str())
				.map(str::trim)
				.filter(|caption| !caption.is_empty())
				.map(str::to_owned);
			Some((url, caption))
		})
		.collect();
	let total = images.len();
	let (url, caption) = images.get(index)?.clone();
	let position = index + 1;
	Some(GalleryView {
		image: GalleryImageView {
			alt: caption.clone().unwrap_or_else(|| format!("Gallery image {position} of {total}")),
			url,
			caption,
		},
		position,
		total,
		has_previous: index > 0,
		previous_url: format!("/gallery/{}/{}", post.id, index.saturating_sub(1)),
		has_next: position < total,
		next_url: format!("/gallery/{}/{position}", post.id),
	})
}

fn is_image_post(post: &Post) -> bool {
	if post.extra.get("post_hint").and_then(|value| value.as_str()) == Some("image") {
		return true;
	}
	let decoded = if post.url.starts_with("//") { format!("https:{}", post.url) } else { post.url.clone() };
	let Ok(url) = Url::parse(&decoded) else {
		return false;
	};
	matches!(
		url.path().rsplit('.').next().map(str::to_ascii_lowercase).as_deref(),
		Some("avif" | "gif" | "jpeg" | "jpg" | "png" | "webp")
	)
}

pub fn comment_tree(children: &[CommentChild], link_id: &str, sort: &str) -> Vec<CommentTreeEvent> {
	let mut tree = Vec::new();
	append_comments(children, link_id, sort, &mut tree);
	tree
}

pub fn loaded_comment_tree(children: &[CommentChild], parent_id: &str, link_id: &str, sort: &str, remaining: &[String]) -> Vec<CommentTreeEvent> {
	let mut tree = Vec::new();
	let mut visited = HashSet::new();
	append_flat_comments(children, parent_id, link_id, sort, &mut visited, &mut tree);
	for child in children {
		if !visited.contains(child_name(child)) {
			append_flat_comment(child, children, link_id, sort, &mut visited, &mut tree);
		}
	}
	if !remaining.is_empty() {
		tree.push(CommentTreeEvent::Open(CommentView {
			id: String::new(),
			author: String::new(),
			body: format!("{} more replies", compact(remaining.len() as u64)),
			score: String::new(),
			age: String::new(),
			permalink: String::new(),
			more_url: more_comments_url(remaining, link_id, parent_id, sort),
			more: true,
		}));
		tree.push(CommentTreeEvent::Close);
	}
	tree
}

pub fn feed_controls(action: &str, active_sort: &'static str, active_time: &str, include_controversial: bool) -> SortControls {
	let mut sorts = vec![("hot", "Hot"), ("new", "New"), ("rising", "Rising"), ("top", "Top")];
	if include_controversial {
		sorts.push(("controversial", "Controversial"));
	}
	SortControls {
		action: action.into(),
		label: "Post sorting",
		choice_label: "Sort posts",
		active_sort,
		query: String::new(),
		sorts: select_choices(sorts, active_sort),
		times: if active_sort == "top" {
			select_choices(
				vec![
					("hour", "Past hour"),
					("day", "Past 24 hours"),
					("week", "Past week"),
					("month", "Past month"),
					("year", "Past year"),
					("all", "All time"),
				],
				active_time,
			)
		} else {
			Vec::new()
		},
	}
}

pub fn comment_sort_controls(active: &'static str, permalink: &str, query: &str) -> SortControls {
	SortControls {
		action: permalink.into(),
		label: "Comment sorting",
		choice_label: "Sort comments",
		active_sort: active,
		query: query.into(),
		sorts: select_choices(
			[("best", "Best"), ("top", "Top"), ("new", "New"), ("old", "Old"), ("controversial", "Controversial")],
			active,
		),
		times: Vec::new(),
	}
}

pub fn search_comment_tree(comments: &[Comment]) -> Vec<CommentTreeEvent> {
	let mut tree = Vec::with_capacity(comments.len() * 2);
	for comment in comments {
		tree.push(CommentTreeEvent::Open(comment_view(comment)));
		tree.push(CommentTreeEvent::Close);
	}
	tree
}

fn select_choices(values: impl IntoIterator<Item = (&'static str, &'static str)>, active: &str) -> Vec<SelectChoice> {
	values
		.into_iter()
		.map(|(value, label)| SelectChoice {
			value,
			label,
			checked: value == active,
		})
		.collect()
}

pub fn pagination(page: u32, previous_url: String, next_url: String) -> Pagination {
	Pagination {
		page,
		has_previous: !previous_url.is_empty(),
		previous_url,
		has_next: !next_url.is_empty(),
		next_url,
	}
}

pub fn feed_pagination(base: &str, sort: &str, time: &str, before: Option<&str>, after: Option<&str>, page: u32) -> Pagination {
	let previous_url = if page > 1 {
		listing_page_url(base, sort, time, "before", before, page.saturating_sub(1))
	} else {
		String::new()
	};
	let next_url = listing_page_url(base, sort, time, "after", after, page.saturating_add(1));
	pagination(page, previous_url, next_url)
}

fn listing_page_url(base: &str, sort: &str, time: &str, cursor_name: &str, cursor: Option<&str>, page: u32) -> String {
	let Some(cursor) = cursor else {
		return String::new();
	};
	let mut query = form_urlencoded::Serializer::new(String::new());
	if sort != "hot" {
		query.append_pair("sort", sort);
	}
	if sort == "top" {
		query.append_pair("t", time);
	}
	query.append_pair(cursor_name, cursor);
	if page > 1 {
		query.append_pair("page", &page.to_string());
	}
	format!("{base}?{}", query.finish())
}

fn wiki_path(subreddit: &str, page: &str) -> String {
	let mut url = Url::parse("http://neddit.local").expect("static base URL is valid");
	{
		let mut segments = url.path_segments_mut().expect("HTTP URLs support path segments");
		segments.extend(["r", subreddit, "wiki"]);
		segments.extend(page.split('/'));
	}
	url.path().to_owned()
}

fn safe_outbound(value: &str) -> Option<String> {
	let url = Url::parse(value).ok()?;
	if !matches!(url.scheme(), "http" | "https") {
		return None;
	}
	Some(local_web_navigation(url.as_str()).unwrap_or_else(|| url.to_string()))
}

fn local_web_navigation(value: &str) -> Option<String> {
	let local = rewrite_reddit_navigation(value)?;
	let base = Url::parse("http://neddit.local").expect("static base URL is valid");
	let url = base.join(&local).ok()?;
	let segments: Vec<_> = url.path_segments()?.filter(|segment| !segment.is_empty()).collect();

	let supported = matches!(segments.as_slice(), [] | ["search"] | ["comments", _, ..] | ["r", _] | ["r", _, "comments" | "wiki", ..]);
	if !supported {
		return None;
	}

	let mut local = url.path().to_owned();
	if let Some(query) = url.query() {
		local.push('?');
		local.push_str(query);
	}
	if let Some(fragment) = url.fragment() {
		local.push('#');
		local.push_str(fragment);
	}
	Some(local)
}

fn outbound_domain(value: &str) -> Option<String> {
	let url = Url::parse(value).ok()?;
	let host = url.host_str()?;
	Some(host.strip_prefix("www.").unwrap_or(host).to_owned())
}

fn display_author(author: &str) -> String {
	if author.is_empty() {
		"[deleted]".into()
	} else {
		author.into()
	}
}

fn append_comments(children: &[CommentChild], link_id: &str, sort: &str, tree: &mut Vec<CommentTreeEvent>) {
	for child in children {
		match child {
			CommentChild::Comment(comment) => {
				let data = &comment.data;
				tree.push(CommentTreeEvent::Open(comment_view(data)));
				if let CommentReplies::Listing(replies) = &data.replies {
					if !replies.data.children.is_empty() {
						tree.push(CommentTreeEvent::OpenReplies);
						append_comments(&replies.data.children, link_id, sort, tree);
						tree.push(CommentTreeEvent::CloseReplies);
					}
				}
				tree.push(CommentTreeEvent::Close);
			}
			CommentChild::More(more) => {
				tree.push(CommentTreeEvent::Open(more_view(&more.data, link_id, sort)));
				tree.push(CommentTreeEvent::Close);
			}
		}
	}
}

fn comment_view(comment: &Comment) -> CommentView {
	CommentView {
		id: comment.id.clone(),
		author: display_author(&comment.author),
		body: comment.body.clone(),
		score: signed_compact(comment.score),
		age: age(comment.created_utc),
		permalink: format!("#comment-{}", comment.id),
		more_url: String::new(),
		more: false,
	}
}

fn more_view(more: &More, link_id: &str, sort: &str) -> CommentView {
	CommentView {
		id: more.id.clone(),
		author: String::new(),
		body: if more.count == 0 {
			"Continue this thread".into()
		} else {
			format!("{} more replies", compact(more.count))
		},
		score: String::new(),
		age: String::new(),
		permalink: String::new(),
		more_url: more_comments_url(&more.children, link_id, &more.parent_id, sort),
		more: true,
	}
}

fn more_comments_url(children: &[String], link_id: &str, parent_id: &str, sort: &str) -> String {
	if children.is_empty() {
		return String::new();
	}
	let mut query = form_urlencoded::Serializer::new(String::new());
	query.append_pair("children", &children.join(","));
	query.append_pair("link_id", link_id);
	query.append_pair("parent_id", parent_id);
	if sort != "best" {
		query.append_pair("sort", sort);
	}
	format!("/more-comments?{}", query.finish())
}

fn append_flat_comments(children: &[CommentChild], parent_id: &str, link_id: &str, sort: &str, visited: &mut HashSet<String>, tree: &mut Vec<CommentTreeEvent>) {
	for child in children {
		if child_parent_id(child) == parent_id && !visited.contains(child_name(child)) {
			append_flat_comment(child, children, link_id, sort, visited, tree);
		}
	}
}

fn append_flat_comment(child: &CommentChild, children: &[CommentChild], link_id: &str, sort: &str, visited: &mut HashSet<String>, tree: &mut Vec<CommentTreeEvent>) {
	if !visited.insert(child_name(child).to_owned()) {
		return;
	}

	match child {
		CommentChild::Comment(comment) => {
			tree.push(CommentTreeEvent::Open(comment_view(&comment.data)));
			let mut replies = Vec::new();
			if let CommentReplies::Listing(listing) = &comment.data.replies {
				append_comments(&listing.data.children, link_id, sort, &mut replies);
			}
			append_flat_comments(children, &comment.data.name, link_id, sort, visited, &mut replies);
			if !replies.is_empty() {
				tree.push(CommentTreeEvent::OpenReplies);
				tree.extend(replies);
				tree.push(CommentTreeEvent::CloseReplies);
			}
			tree.push(CommentTreeEvent::Close);
		}
		CommentChild::More(more) => {
			tree.push(CommentTreeEvent::Open(more_view(&more.data, link_id, sort)));
			tree.push(CommentTreeEvent::Close);
		}
	}
}

fn child_name(child: &CommentChild) -> &str {
	match child {
		CommentChild::Comment(comment) => &comment.data.name,
		CommentChild::More(more) => &more.data.name,
	}
}

fn child_parent_id(child: &CommentChild) -> &str {
	match child {
		CommentChild::Comment(comment) => &comment.data.parent_id,
		CommentChild::More(more) => &more.data.parent_id,
	}
}

fn signed_compact(value: i64) -> String {
	if value < 0 {
		format!("−{}", compact(value.unsigned_abs()))
	} else {
		compact(value.unsigned_abs())
	}
}

fn compact(value: u64) -> String {
	const UNITS: [(u64, &str); 7] = [
		(1, ""),
		(1_000, "k"),
		(1_000_000, "m"),
		(1_000_000_000, "b"),
		(1_000_000_000_000, "t"),
		(1_000_000_000_000_000, "q"),
		(1_000_000_000_000_000_000, "e"),
	];
	if value < 1_000 {
		return value.to_string();
	}

	let mut unit = UNITS.partition_point(|(divisor, _)| *divisor <= value) - 1;
	loop {
		let (divisor, suffix) = UNITS[unit];
		let value = u128::from(value);
		let divisor = u128::from(divisor);

		if value < divisor * 10 {
			let tenths = (value * 10 + divisor / 2) / divisor;
			let whole = tenths / 10;
			let decimal = tenths % 10;
			return if decimal == 0 {
				format!("{whole}{suffix}")
			} else {
				format!("{whole}.{decimal}{suffix}")
			};
		}

		let rounded = (value + divisor / 2) / divisor;
		if rounded >= 1_000 && unit + 1 < UNITS.len() {
			unit += 1;
			continue;
		}
		return format!("{rounded}{suffix}");
	}
}

fn age(timestamp: f64) -> String {
	let elapsed = if timestamp.is_finite() {
		(unix_now() - timestamp).clamp(0.0, 1_000_000_000_000.0)
	} else {
		0.0
	};
	let seconds = std::time::Duration::from_secs_f64(elapsed).as_secs();
	for (name, size) in [("year", 31_536_000), ("month", 2_592_000), ("day", 86_400), ("hour", 3_600), ("minute", 60), ("second", 1)] {
		if seconds >= size || size == 1 {
			let count = seconds / size;
			return format!("{count} {name}{} ago", if count == 1 { "" } else { "s" });
		}
	}
	"now".into()
}

fn unix_now() -> f64 {
	SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |duration| duration.as_secs_f64())
}

