use crate::models::{Listing, Post, PublicThing, Subreddit, Thing, TrophyList, User};

use crate::parsing::posts::parse_post_listing;
use crate::parsing::public::{parse_info_listing, parse_search_listing, parse_user_comment_listing, parse_user_listing, parse_user_overview_listing};
use crate::parsing::subreddit::parse_subreddit_listing;
use crate::parsing::trophy::parse_trophy_list;
use crate::service::query_codec;
use crate::service::reddit::{invalid_parameter, validate_listing_query, validate_subreddit, validate_username, with_query};
use crate::service::sanitize::{clear_listing_modhash, clear_public_listing_modhash};
use crate::service::{InfoQuery, RedditService, SearchQuery, ServiceError, UserDirectorySort, UserHistoryQuery, UserSearchQuery};
use url::Url;
use uuid::Uuid;

impl RedditService {
	pub async fn search(&self, query: &SearchQuery) -> Result<Listing<PublicThing>, ServiceError> {
		self.search_listing(None, query).await
	}

	pub async fn search_subreddit(&self, subreddit: &str, query: &SearchQuery) -> Result<Listing<PublicThing>, ServiceError> {
		validate_subreddit(subreddit)?;
		self.require_safe_subreddit(subreddit).await?;
		self.search_listing(Some(subreddit), query).await
	}

	pub async fn info(&self, query: &InfoQuery) -> Result<Listing<PublicThing>, ServiceError> {
		self.info_listing(None, query).await
	}

	pub async fn subreddit_info(&self, subreddit: &str, query: &InfoQuery) -> Result<Listing<PublicThing>, ServiceError> {
		validate_subreddit(subreddit)?;
		self.require_safe_subreddit(subreddit).await?;
		self.info_listing(Some(subreddit), query).await
	}

	pub async fn user_overview(&self, username: &str, query: &UserHistoryQuery) -> Result<Listing<PublicThing>, ServiceError> {
		validate_username(username)?;
		validate_user_history_query(query)?;
		let path = with_query(format!("/user/{username}/overview"), query_codec::encode(query));
		let json = self.client.json(path).await?;
		let mut listing = parse_user_overview_listing(&json)?;
		clear_public_listing_modhash(&mut listing);
		self.content.filter_public(&mut listing);
		Ok(listing)
	}

	pub async fn user_submitted(&self, username: &str, query: &UserHistoryQuery) -> Result<Listing<Thing<Post>>, ServiceError> {
		validate_username(username)?;
		validate_user_history_query(query)?;
		let path = with_query(format!("/user/{username}/submitted"), query_codec::encode(query));
		let json = self.client.json(path).await?;
		let mut listing = parse_post_listing(&json)?;
		clear_listing_modhash(&mut listing);
		self.content.filter_posts(&mut listing);
		Ok(listing)
	}

	pub async fn user_comments(&self, username: &str, query: &UserHistoryQuery) -> Result<Listing<PublicThing>, ServiceError> {
		validate_username(username)?;
		validate_user_history_query(query)?;
		let path = with_query(format!("/user/{username}/comments"), query_codec::encode(query));
		let json = self.client.json(path).await?;
		let mut listing = parse_user_comment_listing(&json)?;
		clear_public_listing_modhash(&mut listing);
		self.content.filter_public(&mut listing);
		Ok(listing)
	}

	pub async fn user_trophies(&self, username: &str) -> Result<TrophyList, ServiceError> {
		validate_username(username)?;
		let json = self.client.json(format!("/api/v1/user/{username}/trophies")).await?;
		Ok(parse_trophy_list(&json)?)
	}

	pub async fn users(&self, sort: UserDirectorySort, query: &crate::service::ListingQuery) -> Result<Listing<Thing<Subreddit>>, ServiceError> {
		validate_listing_query(query)?;
		let path = with_query(format!("/users/{}", sort.as_str()), query_codec::encode(query));
		let json = self.client.json(path).await?;
		let mut listing = parse_subreddit_listing(&json)?;
		clear_listing_modhash(&mut listing);
		self.content.filter_subreddits(&mut listing);
		Ok(listing)
	}

	pub async fn search_users(&self, query: &UserSearchQuery) -> Result<Listing<Thing<User>>, ServiceError> {
		validate_user_search_query(query)?;
		let path = with_query("/users/search".to_string(), query_codec::encode(query));
		let json = self.client.json(path).await?;
		let mut listing = parse_user_listing(&json)?;
		clear_listing_modhash(&mut listing);
		self.content.filter_users(&mut listing);
		Ok(listing)
	}

	async fn search_listing(&self, subreddit: Option<&str>, query: &SearchQuery) -> Result<Listing<PublicThing>, ServiceError> {
		validate_search_query(query)?;
		if !self.content.allows_nsfw() && query.include_over_18 == Some(true) {
			return Err(ServiceError::ContentBlocked);
		}
		let mut query = query.clone();
		if !self.content.allows_nsfw() {
			query.include_over_18 = Some(false);
		}
		let base = match subreddit {
			Some(subreddit) => format!("/r/{subreddit}/search"),
			None => "/search".to_string(),
		};
		let json = self.client.json(with_query(base, query_codec::encode(&query))).await?;
		let mut listing = parse_search_listing(&json)?;
		clear_listing_modhash(&mut listing);
		self.content.filter_public(&mut listing);
		Ok(listing)
	}

	async fn info_listing(&self, subreddit: Option<&str>, query: &InfoQuery) -> Result<Listing<PublicThing>, ServiceError> {
		validate_info_query(query)?;
		let base = match subreddit {
			Some(subreddit) => format!("/r/{subreddit}/api/info"),
			None => "/api/info".to_string(),
		};
		let json = self.client.json(with_query(base, query_codec::encode(query))).await?;
		let mut listing = parse_info_listing(&json)?;
		clear_public_listing_modhash(&mut listing);
		self.content.filter_public(&mut listing);
		Ok(listing)
	}
}

fn validate_search_query(query: &SearchQuery) -> Result<(), ServiceError> {
	validate_listing_query(&query.listing)?;
	if query.query.is_empty() || query.query.chars().count() > 512 {
		return Err(invalid_parameter("q", &query.query));
	}
	if query.category.as_ref().is_some_and(|category| category.chars().count() > 5) {
		return Err(invalid_parameter("category", query.category.as_deref().unwrap_or_default()));
	}
	Ok(())
}

fn validate_info_query(query: &InfoQuery) -> Result<(), ServiceError> {
	for id in &query.ids {
		let valid = ["t1_", "t3_", "t5_"].iter().any(|prefix| {
			id.strip_prefix(prefix)
				.is_some_and(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_alphanumeric()))
		});
		if !valid {
			return Err(invalid_parameter("id", id));
		}
	}
	for subreddit in &query.subreddit_names {
		validate_subreddit(subreddit)?;
	}
	if let Some(value) = &query.url {
		let url = Url::parse(value).map_err(|_| invalid_parameter("url", value))?;
		if !matches!(url.scheme(), "http" | "https") {
			return Err(invalid_parameter("url", value));
		}
	}
	Ok(())
}

fn validate_user_history_query(query: &UserHistoryQuery) -> Result<(), ServiceError> {
	validate_listing_query(&query.listing)?;
	if query.context.is_some_and(|context| !(2..=10).contains(&context)) {
		return Err(invalid_parameter("context", &query.context.unwrap_or_default().to_string()));
	}
	Ok(())
}

fn validate_user_search_query(query: &UserSearchQuery) -> Result<(), ServiceError> {
	validate_listing_query(&query.listing)?;
	if query.query.is_empty() {
		return Err(invalid_parameter("q", ""));
	}
	if let Some(search_query_id) = &query.search_query_id {
		Uuid::parse_str(search_query_id).map_err(|_| invalid_parameter("search_query_id", search_query_id))?;
	}
	Ok(())
}
