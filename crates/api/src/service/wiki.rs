use crate::models::{Listing, Post, Thing, WikiPage, WikiPageListing, WikiRevision};
use crate::parsing::wiki::{parse_wiki_discussions, parse_wiki_page, parse_wiki_page_listing, parse_wiki_revisions};
use crate::service::query_codec;
use crate::service::reddit::{invalid_parameter, validate_listing_query, validate_subreddit, validate_wiki_listing_query, with_query};
use crate::service::sanitize::clear_listing_modhash;
use crate::service::{ListingQuery, RedditService, ServiceError};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use uuid::Uuid;

impl RedditService {
	pub async fn wiki_pages(&self, subreddit: &str) -> Result<WikiPageListing, ServiceError> {
		validate_subreddit(subreddit)?;
		self.require_safe_subreddit(subreddit).await?;
		let json = self.client.json(format!("/r/{subreddit}/wiki/pages")).await?;
		Ok(parse_wiki_page_listing(&json)?)
	}

	pub async fn wiki_page(&self, subreddit: &str, page: &str, query: &WikiPageQuery) -> Result<WikiPage, ServiceError> {
		validate_subreddit(subreddit)?;
		self.require_safe_subreddit(subreddit).await?;
		let page = encode_wiki_page(page)?;
		validate_wiki_page_query(query)?;
		let path = with_query(format!("/r/{subreddit}/wiki/{page}"), query_codec::encode(query));
		let json = self.client.json(path).await?;
		Ok(parse_wiki_page(&json)?)
	}

	pub async fn wiki_revisions(&self, subreddit: &str, page: Option<&str>, query: &ListingQuery) -> Result<Listing<WikiRevision>, ServiceError> {
		validate_subreddit(subreddit)?;
		self.require_safe_subreddit(subreddit).await?;
		validate_wiki_listing_query(query)?;
		let base = match page {
			Some(page) => format!("/r/{subreddit}/wiki/revisions/{}", encode_wiki_page(page)?),
			None => format!("/r/{subreddit}/wiki/revisions"),
		};
		let path = with_query(base, query_codec::encode(query));
		let json = self.client.json(path).await?;
		let mut listing = parse_wiki_revisions(&json)?;
		clear_listing_modhash(&mut listing);
		Ok(listing)
	}

	pub async fn wiki_discussions(&self, subreddit: &str, page: &str, query: &ListingQuery) -> Result<Listing<Thing<Post>>, ServiceError> {
		validate_subreddit(subreddit)?;
		self.require_safe_subreddit(subreddit).await?;
		validate_listing_query(query)?;
		let page = encode_wiki_page(page)?;
		let path = with_query(format!("/r/{subreddit}/wiki/discussions/{page}"), query_codec::encode(query));
		let json = self.client.json(path).await?;
		let mut listing = parse_wiki_discussions(&json)?;
		clear_listing_modhash(&mut listing);
		self.content.filter_posts(&mut listing);
		Ok(listing)
	}
}

fn encode_wiki_page(page: &str) -> Result<String, ServiceError> {
	let valid = !page.is_empty() && page.split('/').all(|segment| !segment.is_empty() && !matches!(segment, "." | ".."));
	if !valid {
		return Err(invalid_parameter("page", page));
	}
	Ok(
		page
			.split('/')
			.map(|segment| utf8_percent_encode(segment, NON_ALPHANUMERIC).to_string())
			.collect::<Vec<_>>()
			.join("/"),
	)
}

fn validate_wiki_page_query(query: &WikiPageQuery) -> Result<(), ServiceError> {
	for (name, value) in [("v", &query.v), ("v2", &query.v2)] {
		if let Some(value) = value {
			Uuid::parse_str(value).map_err(|_| invalid_parameter(name, value))?;
		}
	}
	Ok(())
}

mod wiki_page_query {
	use utoipa::IntoParams;

	#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize, IntoParams)]
	#[into_params(parameter_in = Query)]
	pub struct WikiPageQuery {
		pub v: Option<String>,
		pub v2: Option<String>,
	}
}

pub use wiki_page_query::WikiPageQuery;
