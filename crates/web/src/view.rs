use std::{
	collections::HashSet,
	time::{SystemTime, UNIX_EPOCH},
};

use ammonia::{Builder, UrlRelative};
use askama::Template;
use askama_web::WebTemplate;

use neddit_api::models::{Comment, CommentChild, CommentReplies, More, Post, Subreddit, WikiPage, WikiPageListing};
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
}

#[derive(Clone, Debug)]
pub struct SortLink {
	pub label: &'static str,
	pub href: String,
	pub active: bool,
}

#[derive(Template, WebTemplate)]
#[template(path = "feed.html")]
pub struct FeedTemplate {
	pub items: Vec<FeedItem>,
	pub sorts: Vec<SortLink>,
	pub next_url: String,
	pub has_next: bool,
	pub feed_label: String,
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
	pub sorts: Vec<SortLink>,
	pub next_url: String,
	pub has_next: bool,
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
	pub selftext: String,
	pub show_selftext: bool,
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
	pub sorts: Vec<SortLink>,
	pub comments_heading: String,
	pub has_comments: bool,
}

#[derive(Template, WebTemplate)]
#[template(path = "more_comments.html")]
pub struct MoreCommentsTemplate {
	pub comment_tree: Vec<CommentTreeEvent>,
}

pub fn feed_item(post: &Post) -> FeedItem {
	let href = if post.is_self {
		post.permalink.clone()
	} else {
		safe_outbound(&post.url).unwrap_or_else(|| post.permalink.clone())
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
		permalink: post.permalink.clone(),
		age: age(post.created_utc),
		score: if post.hide_score { "—".into() } else { compact(post.score.max(0).unsigned_abs()) },
		comments: compact(post.num_comments),
		stickied: post.stickied,
		badges,
	}
}

pub fn community_view(subreddit: &Subreddit) -> CommunityView {
	let title = if subreddit.title.trim().is_empty() {
		subreddit.display_name_prefixed.clone()
	} else {
		subreddit.title.trim().to_owned()
	};
	let public_description = nonempty(subreddit.public_description.trim());
	let description = nonempty(subreddit.description.trim());
	let description_html = subreddit.description_html.as_deref().map(sanitize_html).and_then(|html| nonempty(&html));
	let posts_url = format!("/r/{}", subreddit.display_name);
	CommunityView {
		display_name_prefixed: subreddit.display_name_prefixed.clone(),
		title,
		public_description,
		description,
		description_html,
		members: compact(subreddit.subscribers),
		active: subreddit.accounts_active.map_or_else(|| "—".into(), compact),
		wiki_url: format!("{posts_url}/wiki/index"),
		posts_url,
		has_wiki: subreddit.wiki_enabled.unwrap_or(false),
		over_18: subreddit.over18,
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
	Builder::new().url_relative(UrlRelative::PassThrough).clean(value).to_string()
}

fn nonempty(value: &str) -> Option<String> {
	(!value.is_empty()).then(|| value.to_owned())
}

pub fn post_view(post: &Post) -> PostView {
	let item = feed_item(post);
	let selftext = post.selftext.trim().to_owned();
	PostView {
		show_selftext: post.is_self && !selftext.is_empty(),
		item,
		selftext,
	}
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

pub fn sort_links(active: &str) -> Vec<SortLink> {
	[("hot", "Hot"), ("new", "New"), ("rising", "Rising"), ("top", "Top")]
		.into_iter()
		.map(|(value, label)| SortLink {
			label,
			href: if value == "hot" { "/".into() } else { format!("/?sort={value}") },
			active: value == active,
		})
		.collect()
}

pub fn subreddit_sort_links(active: &str, subreddit: &str) -> Vec<SortLink> {
	let base = format!("/r/{subreddit}");
	[("hot", "Hot"), ("new", "New"), ("rising", "Rising"), ("top", "Top"), ("controversial", "Controversial")]
		.into_iter()
		.map(|(value, label)| SortLink {
			label,
			href: if value == "hot" { base.clone() } else { format!("{base}?sort={value}") },
			active: value == active,
		})
		.collect()
}

pub fn comment_sort_links(active: &str, permalink: &str) -> Vec<SortLink> {
	[("best", "Best"), ("top", "Top"), ("new", "New"), ("old", "Old"), ("controversial", "Controversial")]
		.into_iter()
		.map(|(value, label)| SortLink {
			label,
			href: if value == "best" { permalink.into() } else { format!("{permalink}?sort={value}") },
			active: value == active,
		})
		.collect()
}

pub fn next_url(sort: &str, after: Option<&str>, count: u32) -> String {
	listing_next_url("/", sort, after, count)
}

pub fn subreddit_next_url(subreddit: &str, sort: &str, after: Option<&str>, count: u32) -> String {
	listing_next_url(&format!("/r/{subreddit}"), sort, after, count)
}

fn listing_next_url(base: &str, sort: &str, after: Option<&str>, count: u32) -> String {
	let Some(after) = after else {
		return String::new();
	};
	let mut query = form_urlencoded::Serializer::new(String::new());
	if sort != "hot" {
		query.append_pair("sort", sort);
	}
	query.append_pair("after", after);
	query.append_pair("count", &count.to_string());
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
	matches!(url.scheme(), "http" | "https").then(|| url.to_string())
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

