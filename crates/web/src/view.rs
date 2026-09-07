use std::time::{SystemTime, UNIX_EPOCH};

use askama::Template;
use askama_web::WebTemplate;
use neddit_api::models::Post;
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

pub fn next_url(sort: &str, after: Option<&str>, count: u32) -> String {
	let Some(after) = after else {
		return String::new();
	};
	let mut query = form_urlencoded::Serializer::new(String::new());
	if sort != "hot" {
		query.append_pair("sort", sort);
	}
	query.append_pair("after", after);
	query.append_pair("count", &count.to_string());
	format!("/?{}", query.finish())
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

