use crate::client::RedditClient;
use crate::models::{Listing, Post, PostComments, PostDuplicates, Subreddit, Thing, User};
use crate::parsing::comments::parse_comments;
use crate::parsing::posts::{parse_post_duplicates, parse_post_listing};
use crate::parsing::subreddit::{parse_subreddit, parse_subreddit_listing};
use crate::parsing::user::parse_user;
use crate::service::sanitize::{clear_listing_modhash, clear_post_comments_modhash};
use crate::service::{CommentQuery, ContentPolicy, DuplicateQuery, ListingQuery, PostSort, ServiceError, SubredditSearchQuery, SubredditSort};
use std::sync::Arc;
use tokio::sync::Semaphore;
use url::form_urlencoded::Serializer;
use uuid::Uuid;

#[derive(Clone)]
pub struct RedditService {
	pub(super) client: RedditClient,
	pub(super) more_children_gate: Arc<Semaphore>,
	pub(crate) search_sessions: Arc<crate::search::Sessions>,
	pub(crate) content: ContentPolicy,
	pub(crate) cache: Option<crate::storage::Cache>,
	pub(crate) shortlinks: Option<crate::storage::Shortlinks>,
	pub(crate) feed_flights: crate::storage::Flights,
	pub(crate) feed_gate: Arc<Semaphore>,
}

impl RedditService {
	pub fn new(client: RedditClient) -> Self {
		Self::with_content_policy(client, ContentPolicy::default())
	}

	pub fn with_content_policy(client: RedditClient, content: ContentPolicy) -> Self {
		Self {
			client,
			more_children_gate: Arc::new(Semaphore::new(1)),
			search_sessions: Arc::new(crate::search::Sessions::default()),
			content,
			cache: None,
			shortlinks: None,
			feed_flights: Default::default(),
			feed_gate: Arc::new(Semaphore::new(4)),
		}
	}

	pub fn with_storage(mut self, cache: crate::storage::Cache, shortlinks: Option<crate::storage::Shortlinks>) -> Self {
		self.cache = Some(cache);
		self.shortlinks = shortlinks;
		self
	}

	pub fn shortlinks_enabled(&self) -> bool {
		self.shortlinks.is_some()
	}

	pub const fn allows_nsfw(&self) -> bool {
		self.content.allows_nsfw()
	}

	pub async fn front_page_posts(&self, sort: PostSort, query: &ListingQuery) -> Result<Listing<Thing<Post>>, ServiceError> {
		self.post_listing(None, sort, query).await
	}

	pub(crate) async fn recent_ql_comments(&self, community: &str, query: &ListingQuery) -> Result<Listing<crate::models::PublicThing>, ServiceError> {
		validate_subreddit(community)?;
		validate_listing_query(query)?;
		let path = with_query(format!("/r/{community}/comments"), encode_listing_query(query, None));
		let json = self.client.json(path).await?;
		let mut listing = crate::parsing::public::parse_user_comment_listing(&json)?;
		self.content.filter_public(&mut listing);
		Ok(listing)
	}

	pub async fn subreddit_posts(&self, subreddit: &str, sort: PostSort, query: &ListingQuery) -> Result<Listing<Thing<Post>>, ServiceError> {
		validate_subreddit(subreddit)?;
		self.require_safe_subreddit(subreddit).await?;
		if sort == PostSort::Best {
			return Err(invalid_parameter("sort", sort.as_str()));
		}
		self.post_listing(Some(subreddit), sort, query).await
	}

	pub(crate) async fn feed_posts(&self, subreddits: &str, sort: PostSort, query: &ListingQuery) -> Result<Listing<Thing<Post>>, ServiceError> {
		validate_subreddit(subreddits)?;
		if sort == PostSort::Best {
			return Err(invalid_parameter("sort", sort.as_str()));
		}
		self.post_listing(Some(subreddits), sort, query).await
	}

	pub async fn posts_by_id(&self, names: &str) -> Result<Listing<Thing<Post>>, ServiceError> {
		let names = canonical_post_fullnames(names)?;
		let json = self.client.json(format!("/by_id/{names}")).await?;
		let mut listing = parse_post_listing(&json)?;
		clear_listing_modhash(&mut listing);
		self.content.filter_posts(&mut listing);
		Ok(listing)
	}

	pub async fn post_duplicates(&self, article: &str, query: &DuplicateQuery) -> Result<PostDuplicates, ServiceError> {
		validate_id36("article", article)?;
		validate_listing_query(&query.listing)?;
		if let Some(subreddit) = &query.subreddit {
			validate_subreddit(subreddit)?;
		}
		let path = with_query(format!("/duplicates/{article}"), encode_duplicate_query(query));
		let json = self.client.json(path).await?;
		let mut duplicates = parse_post_duplicates(&json)?;
		clear_listing_modhash(&mut duplicates.0);
		clear_listing_modhash(&mut duplicates.1);
		self.content.require_post_listing(&duplicates.0)?;
		self.content.filter_posts(&mut duplicates.0);
		self.content.filter_posts(&mut duplicates.1);
		Ok(duplicates)
	}

	pub async fn post_comments(&self, article: &str, query: &CommentQuery) -> Result<PostComments, ServiceError> {
		self.comments(None, article, query).await
	}

	pub async fn subreddit_post_comments(&self, subreddit: &str, article: &str, query: &CommentQuery) -> Result<PostComments, ServiceError> {
		validate_subreddit(subreddit)?;
		self.comments(Some(subreddit), article, query).await
	}

	pub async fn subreddit_about(&self, subreddit: &str) -> Result<Thing<Subreddit>, ServiceError> {
		validate_subreddit(subreddit)?;
		let json = self.client.json(format!("/r/{subreddit}/about")).await?;
		let subreddit = parse_subreddit(&json)?;
		self.content.require_subreddit(&subreddit)?;
		Ok(subreddit)
	}

	pub(super) async fn require_safe_subreddit(&self, subreddit: &str) -> Result<(), ServiceError> {
		if self.content.allows_nsfw() {
			return Ok(());
		}
		for name in subreddit.split('+') {
			self.subreddit_about(name).await?;
		}
		Ok(())
	}

	pub async fn user_about(&self, username: &str) -> Result<Thing<User>, ServiceError> {
		validate_username(username)?;
		let json = self.client.json(format!("/user/{username}/about")).await?;
		let user = parse_user(&json)?;
		self.content.require_user(&user)?;
		Ok(user)
	}

	pub async fn subreddits(&self, sort: SubredditSort, query: &ListingQuery) -> Result<Listing<Thing<Subreddit>>, ServiceError> {
		validate_listing_query(query)?;
		let path = with_query(format!("/subreddits/{}", sort.as_str()), encode_listing_query(query, None));
		let json = self.client.json(path).await?;
		let mut listing = parse_subreddit_listing(&json)?;
		clear_listing_modhash(&mut listing);
		self.content.filter_subreddits(&mut listing);
		Ok(listing)
	}

	pub async fn search_subreddits(&self, query: &SubredditSearchQuery) -> Result<Listing<Thing<Subreddit>>, ServiceError> {
		validate_subreddit_search_query(query)?;
		if !self.content.allows_nsfw() && query.include_over_18 == Some(true) {
			return Err(ServiceError::ContentBlocked);
		}
		let mut query = query.clone();
		if !self.content.allows_nsfw() {
			query.include_over_18 = Some(false);
		}
		let path = with_query("/subreddits/search".to_string(), encode_subreddit_search_query(&query));
		let json = self.client.json(path).await?;
		let mut listing = parse_subreddit_listing(&json)?;
		clear_listing_modhash(&mut listing);
		self.content.filter_subreddits(&mut listing);
		Ok(listing)
	}

	async fn post_listing(&self, subreddit: Option<&str>, sort: PostSort, query: &ListingQuery) -> Result<Listing<Thing<Post>>, ServiceError> {
		validate_listing_query(query)?;
		let base = match subreddit {
			Some(subreddit) => format!("/r/{}/{}", subreddit.replace('+', "%2B"), sort.as_str()),
			None => format!("/{}", sort.as_str()),
		};
		let path = with_query(base, encode_listing_query(query, None));
		let json = self.client.json(path).await?;
		let mut listing = parse_post_listing(&json)?;
		clear_listing_modhash(&mut listing);
		self.content.filter_posts(&mut listing);
		Ok(listing)
	}

	async fn comments(&self, subreddit: Option<&str>, article: &str, query: &CommentQuery) -> Result<PostComments, ServiceError> {
		validate_id36("article", article)?;
		validate_comment_query(query)?;
		let base = match subreddit {
			Some(subreddit) => format!("/r/{subreddit}/comments/{article}"),
			None => format!("/comments/{article}"),
		};
		let path = with_query(base, encode_comment_query(query));
		let json = self.client.json(path).await?;
		let mut comments = parse_comments(&json)?;
		clear_post_comments_modhash(&mut comments);
		self.content.require_post_comments(&comments)?;
		Ok(comments)
	}
}

pub(super) fn with_query(base: String, query: String) -> String {
	if query.is_empty() {
		base
	} else {
		format!("{base}?{query}")
	}
}

pub(super) fn validate_subreddit(subreddit: &str) -> Result<(), ServiceError> {
	let valid = !subreddit.is_empty()
		&& subreddit
			.split('+')
			.all(|name| !name.is_empty() && name.len() <= 21 && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_'));

	if valid {
		Ok(())
	} else {
		Err(ServiceError::InvalidSubreddit { subreddit: subreddit.to_string() })
	}
}

pub(super) fn validate_username(username: &str) -> Result<(), ServiceError> {
	let valid = !username.is_empty() && username.len() <= 20 && username.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'));
	if valid {
		Ok(())
	} else {
		Err(invalid_parameter("username", username))
	}
}

pub(super) fn validate_listing_query(query: &ListingQuery) -> Result<(), ServiceError> {
	validate_listing_query_with(query, validate_cursor)
}

pub(super) fn validate_wiki_listing_query(query: &ListingQuery) -> Result<(), ServiceError> {
	validate_listing_query_with(query, validate_wiki_cursor)
}

fn validate_listing_query_with(query: &ListingQuery, cursor_validator: fn(&'static str, &str) -> Result<(), ServiceError>) -> Result<(), ServiceError> {
	if query.after.is_some() && query.before.is_some() {
		return Err(ServiceError::ConflictingCursors);
	}
	if let Some(cursor) = &query.after {
		cursor_validator("after", cursor)?;
	}
	if let Some(cursor) = &query.before {
		cursor_validator("before", cursor)?;
	}
	if query.limit.is_some_and(|limit| !(1..=100).contains(&limit)) {
		return Err(ServiceError::InvalidLimit {
			limit: query.limit.unwrap_or_default(),
		});
	}
	if let Some(geo_filter) = &query.geo_filter {
		let valid = !geo_filter.is_empty() && geo_filter.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
		if !valid {
			return Err(invalid_parameter("g", geo_filter));
		}
	}
	Ok(())
}

fn validate_comment_query(query: &CommentQuery) -> Result<(), ServiceError> {
	if let Some(comment) = &query.comment {
		validate_id36("comment", comment)?;
	}
	if query.context.is_some_and(|context| context > 8) {
		return Err(invalid_parameter("context", &query.context.unwrap_or_default().to_string()));
	}
	if query.truncate.is_some_and(|truncate| truncate > 50) {
		return Err(invalid_parameter("truncate", &query.truncate.unwrap_or_default().to_string()));
	}
	Ok(())
}

fn validate_subreddit_search_query(query: &SubredditSearchQuery) -> Result<(), ServiceError> {
	validate_listing_query(&query.listing)?;
	if query.query.is_empty() {
		return Err(invalid_parameter("q", ""));
	}
	if let Some(search_query_id) = &query.search_query_id {
		Uuid::parse_str(search_query_id).map_err(|_| invalid_parameter("search_query_id", search_query_id))?;
	}
	Ok(())
}

fn validate_cursor(parameter: &'static str, cursor: &str) -> Result<(), ServiceError> {
	let valid = cursor.split_once('_').is_some_and(|(prefix, id)| {
		prefix
			.strip_prefix('t')
			.is_some_and(|number| !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()))
			&& !id.is_empty()
			&& id.bytes().all(|byte| byte.is_ascii_alphanumeric())
	});
	if valid {
		Ok(())
	} else {
		Err(ServiceError::InvalidCursor {
			parameter,
			cursor: cursor.to_string(),
		})
	}
}

fn validate_wiki_cursor(parameter: &'static str, cursor: &str) -> Result<(), ServiceError> {
	let valid = cursor.strip_prefix("WikiRevision_").is_some_and(|id| Uuid::parse_str(id).is_ok());
	if valid {
		Ok(())
	} else {
		Err(ServiceError::InvalidCursor {
			parameter,
			cursor: cursor.to_string(),
		})
	}
}

pub(super) fn validate_id36(parameter: &'static str, value: &str) -> Result<(), ServiceError> {
	if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
		Ok(())
	} else {
		Err(invalid_parameter(parameter, value))
	}
}

fn canonical_post_fullnames(names: &str) -> Result<String, ServiceError> {
	let names = names
		.split(|character: char| character == ',' || character.is_ascii_whitespace())
		.filter(|name| !name.is_empty())
		.map(|name| {
			let valid = name
				.strip_prefix("t3_")
				.is_some_and(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_alphanumeric()));
			if valid {
				Ok(name)
			} else {
				Err(invalid_parameter("names", name))
			}
		})
		.collect::<Result<Vec<_>, _>>()?;
	if names.is_empty() {
		return Err(invalid_parameter("names", ""));
	}
	Ok(names.join(","))
}

pub(super) fn invalid_parameter(parameter: &'static str, value: &str) -> ServiceError {
	ServiceError::InvalidParameter {
		parameter,
		value: value.to_string(),
	}
}

fn encode_listing_query(query: &ListingQuery, sort: Option<PostSort>) -> String {
	let mut serializer = Serializer::new(String::new());
	append_listing_query(&mut serializer, query);
	if let Some(sort) = sort {
		serializer.append_pair("sort", sort.as_str());
	}
	serializer.finish()
}

pub(super) fn append_listing_query(serializer: &mut Serializer<'_, String>, query: &ListingQuery) {
	if let Some(after) = &query.after {
		serializer.append_pair("after", after);
	}
	if let Some(before) = &query.before {
		serializer.append_pair("before", before);
	}
	if let Some(limit) = query.limit {
		serializer.append_pair("limit", &limit.to_string());
	}
	if let Some(count) = query.count {
		serializer.append_pair("count", &count.to_string());
	}
	if let Some(show) = query.show {
		serializer.append_pair("show", show.as_str());
	}
	if let Some(time) = query.time {
		serializer.append_pair("t", time.as_str());
	}
	if let Some(sr_detail) = query.sr_detail {
		serializer.append_pair("sr_detail", bool_string(sr_detail));
	}
	if let Some(geo_filter) = &query.geo_filter {
		serializer.append_pair("g", geo_filter);
	}
}

fn encode_duplicate_query(query: &DuplicateQuery) -> String {
	let mut serializer = Serializer::new(String::new());
	append_listing_query(&mut serializer, &query.listing);
	if let Some(crossposts_only) = query.crossposts_only {
		serializer.append_pair("crossposts_only", bool_string(crossposts_only));
	}
	if let Some(sort) = query.sort {
		serializer.append_pair("sort", sort.as_str());
	}
	if let Some(subreddit) = &query.subreddit {
		serializer.append_pair("sr", subreddit);
	}
	serializer.finish()
}

fn encode_subreddit_search_query(query: &SubredditSearchQuery) -> String {
	let mut serializer = Serializer::new(String::new());
	append_listing_query(&mut serializer, &query.listing);
	serializer.append_pair("q", &query.query);
	if let Some(search_query_id) = &query.search_query_id {
		serializer.append_pair("search_query_id", search_query_id);
	}
	if let Some(show_users) = query.show_users {
		serializer.append_pair("show_users", bool_string(show_users));
	}
	if let Some(include) = query.include_over_18 {
		serializer.append_pair("include_over_18", if include { "on" } else { "off" });
	}
	if let Some(sort) = query.sort {
		serializer.append_pair("sort", sort.as_str());
	}
	if let Some(typeahead_active) = query.typeahead_active {
		serializer.append_pair("typeahead_active", typeahead_active.as_str());
	}
	serializer.finish()
}

fn encode_comment_query(query: &CommentQuery) -> String {
	let mut serializer = Serializer::new(String::new());
	if let Some(comment) = &query.comment {
		serializer.append_pair("comment", comment);
	}
	if let Some(context) = query.context {
		serializer.append_pair("context", &context.to_string());
	}
	if let Some(depth) = query.depth {
		serializer.append_pair("depth", &depth.to_string());
	}
	if let Some(limit) = query.limit {
		serializer.append_pair("limit", &limit.to_string());
	}
	if let Some(value) = query.showedits {
		serializer.append_pair("showedits", bool_string(value));
	}
	if let Some(value) = query.showmedia {
		serializer.append_pair("showmedia", bool_string(value));
	}
	if let Some(value) = query.showmore {
		serializer.append_pair("showmore", bool_string(value));
	}
	if let Some(value) = query.showtitle {
		serializer.append_pair("showtitle", bool_string(value));
	}
	if let Some(sort) = query.sort {
		serializer.append_pair("sort", sort.as_str());
	}
	if let Some(value) = query.sr_detail {
		serializer.append_pair("sr_detail", bool_string(value));
	}
	if let Some(theme) = query.theme {
		serializer.append_pair("theme", theme.as_str());
	}
	if let Some(value) = query.threaded {
		serializer.append_pair("threaded", bool_string(value));
	}
	if let Some(truncate) = query.truncate {
		serializer.append_pair("truncate", &truncate.to_string());
	}
	serializer.finish()
}

pub(super) const fn bool_string(value: bool) -> &'static str {
	if value {
		"true"
	} else {
		"false"
	}
}
