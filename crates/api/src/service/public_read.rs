use crate::client::Access;
use crate::models::{Listing, Post, PublicThing, Subreddit, Thing, TrophyList, User};

use crate::parsing::posts::parse_post_listing;
use crate::parsing::public::{parse_info_listing, parse_search_listing, parse_user_comment_listing, parse_user_listing, parse_user_overview_listing};
use crate::parsing::subreddit::parse_subreddit_listing;
use crate::parsing::trophy::parse_trophy_list;
use crate::service::reddit::{append_listing_query, bool_string, invalid_parameter, validate_listing_query, validate_subreddit, validate_username, with_query};
use crate::service::sanitize::{clear_listing_modhash, clear_public_listing_modhash};
use crate::service::{InfoQuery, RedditService, SearchQuery, ServiceError, UserDirectorySort, UserHistoryQuery, UserSearchQuery};
use url::{form_urlencoded::Serializer, Url};
use uuid::Uuid;

impl RedditService {
	pub async fn search(&self, query: &SearchQuery, access: Access) -> Result<Listing<PublicThing>, ServiceError> {
		self.search_listing(None, query, access).await
	}

	pub async fn search_subreddit(&self, subreddit: &str, query: &SearchQuery, access: Access) -> Result<Listing<PublicThing>, ServiceError> {
		validate_subreddit(subreddit)?;
		self.search_listing(Some(subreddit), query, access).await
	}

	pub async fn info(&self, query: &InfoQuery, access: Access) -> Result<Listing<PublicThing>, ServiceError> {
		self.info_listing(None, query, access).await
	}

	pub async fn subreddit_info(&self, subreddit: &str, query: &InfoQuery, access: Access) -> Result<Listing<PublicThing>, ServiceError> {
		validate_subreddit(subreddit)?;
		self.info_listing(Some(subreddit), query, access).await
	}

	pub async fn user_overview(&self, username: &str, query: &UserHistoryQuery, access: Access) -> Result<Listing<PublicThing>, ServiceError> {
		validate_username(username)?;
		validate_user_history_query(query)?;
		let path = with_query(format!("/user/{username}/overview"), encode_user_history_query(query));
		let json = self.client.json(path, access).await?;
		let mut listing = parse_user_overview_listing(&json)?;
		clear_public_listing_modhash(&mut listing);
		Ok(listing)
	}

	pub async fn user_submitted(&self, username: &str, query: &UserHistoryQuery, access: Access) -> Result<Listing<Thing<Post>>, ServiceError> {
		validate_username(username)?;
		validate_user_history_query(query)?;
		let path = with_query(format!("/user/{username}/submitted"), encode_user_history_query(query));
		let json = self.client.json(path, access).await?;
		let mut listing = parse_post_listing(&json)?;
		clear_listing_modhash(&mut listing);
		Ok(listing)
	}

	pub async fn user_comments(&self, username: &str, query: &UserHistoryQuery, access: Access) -> Result<Listing<PublicThing>, ServiceError> {
		validate_username(username)?;
		validate_user_history_query(query)?;
		let path = with_query(format!("/user/{username}/comments"), encode_user_history_query(query));
		let json = self.client.json(path, access).await?;
		let mut listing = parse_user_comment_listing(&json)?;
		clear_public_listing_modhash(&mut listing);
		Ok(listing)
	}

	pub async fn user_trophies(&self, username: &str, access: Access) -> Result<TrophyList, ServiceError> {
		validate_username(username)?;
		let json = self.client.json(format!("/api/v1/user/{username}/trophies"), access).await?;
		Ok(parse_trophy_list(&json)?)
	}

	pub async fn users(&self, sort: UserDirectorySort, query: &crate::service::ListingQuery, access: Access) -> Result<Listing<Thing<Subreddit>>, ServiceError> {
		validate_listing_query(query)?;
		let path = with_query(format!("/users/{}", sort.as_str()), encode_listing_query(query));
		let json = self.client.json(path, access).await?;
		let mut listing = parse_subreddit_listing(&json)?;
		clear_listing_modhash(&mut listing);
		Ok(listing)
	}

	pub async fn search_users(&self, query: &UserSearchQuery, access: Access) -> Result<Listing<Thing<User>>, ServiceError> {
		validate_user_search_query(query)?;
		let path = with_query("/users/search".to_string(), encode_user_search_query(query));
		let json = self.client.json(path, access).await?;
		let mut listing = parse_user_listing(&json)?;
		clear_listing_modhash(&mut listing);
		Ok(listing)
	}

	async fn search_listing(&self, subreddit: Option<&str>, query: &SearchQuery, access: Access) -> Result<Listing<PublicThing>, ServiceError> {
		validate_search_query(query)?;
		let base = match subreddit {
			Some(subreddit) => format!("/r/{subreddit}/search"),
			None => "/search".to_string(),
		};
		let json = self.client.json(with_query(base, encode_search_query(query)), access).await?;
		let mut listing = parse_search_listing(&json)?;
		clear_listing_modhash(&mut listing);
		Ok(listing)
	}

	async fn info_listing(&self, subreddit: Option<&str>, query: &InfoQuery, access: Access) -> Result<Listing<PublicThing>, ServiceError> {
		validate_info_query(query)?;
		let base = match subreddit {
			Some(subreddit) => format!("/r/{subreddit}/api/info"),
			None => "/api/info".to_string(),
		};
		let json = self.client.json(with_query(base, encode_info_query(query)), access).await?;
		let mut listing = parse_info_listing(&json)?;
		clear_public_listing_modhash(&mut listing);
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

fn encode_search_query(query: &SearchQuery) -> String {
	let mut serializer = Serializer::new(String::new());
	append_listing_query(&mut serializer, &query.listing);
	serializer.append_pair("q", &query.query);
	if let Some(category) = &query.category {
		serializer.append_pair("category", category);
	}
	if let Some(include_facets) = query.include_facets {
		serializer.append_pair("include_facets", bool_string(include_facets));
	}
	if let Some(include) = query.include_over_18 {
		serializer.append_pair("include_over_18", if include { "on" } else { "off" });
	}
	if let Some(restrict_sr) = query.restrict_sr {
		serializer.append_pair("restrict_sr", bool_string(restrict_sr));
	}
	if let Some(sort) = query.sort {
		serializer.append_pair("sort", sort.as_str());
	}
	if !query.result_types.is_empty() {
		let result_types = query.result_types.iter().map(|result_type| result_type.as_str()).collect::<Vec<_>>().join(",");
		serializer.append_pair("type", &result_types);
	}
	serializer.finish()
}

fn encode_listing_query(query: &crate::service::ListingQuery) -> String {
	let mut serializer = Serializer::new(String::new());
	append_listing_query(&mut serializer, query);
	serializer.finish()
}

fn encode_info_query(query: &InfoQuery) -> String {
	let mut serializer = Serializer::new(String::new());
	if !query.ids.is_empty() {
		serializer.append_pair("id", &query.ids.join(","));
	}
	if !query.subreddit_names.is_empty() {
		serializer.append_pair("sr_name", &query.subreddit_names.join(","));
	}
	if let Some(url) = &query.url {
		serializer.append_pair("url", url);
	}
	serializer.finish()
}

fn encode_user_history_query(query: &UserHistoryQuery) -> String {
	let mut serializer = Serializer::new(String::new());
	append_listing_query(&mut serializer, &query.listing);
	if let Some(context) = query.context {
		serializer.append_pair("context", &context.to_string());
	}
	if let Some(show) = query.show {
		serializer.append_pair("show", show.as_str());
	}
	if let Some(sort) = query.sort {
		serializer.append_pair("sort", sort.as_str());
	}
	if let Some(content_type) = query.content_type {
		serializer.append_pair("type", content_type.as_str());
	}
	serializer.finish()
}

fn encode_user_search_query(query: &UserSearchQuery) -> String {
	let mut serializer = Serializer::new(String::new());
	append_listing_query(&mut serializer, &query.listing);
	serializer.append_pair("q", &query.query);
	if let Some(search_query_id) = &query.search_query_id {
		serializer.append_pair("search_query_id", search_query_id);
	}
	if let Some(sort) = query.sort {
		serializer.append_pair("sort", sort.as_str());
	}
	if let Some(typeahead_active) = query.typeahead_active {
		serializer.append_pair("typeahead_active", typeahead_active.as_str());
	}
	serializer.finish()
}
