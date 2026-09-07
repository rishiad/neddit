use crate::client::Access;
use crate::models::{Listing, Post, Thing, WikiPage, WikiPageListing, WikiRevision};
use crate::parsing::wiki::{parse_wiki_discussions, parse_wiki_page, parse_wiki_page_listing, parse_wiki_revisions};
use crate::service::reddit::{append_listing_query, invalid_parameter, validate_listing_query, validate_subreddit, validate_wiki_listing_query, with_query};
use crate::service::sanitize::clear_listing_modhash;
use crate::service::{ListingQuery, RedditService, ServiceError, WikiPageQuery};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use url::form_urlencoded::Serializer;
use uuid::Uuid;

impl RedditService {
	pub async fn wiki_pages(&self, subreddit: &str, access: Access) -> Result<WikiPageListing, ServiceError> {
		validate_subreddit(subreddit)?;
		let json = self.client.json(format!("/r/{subreddit}/wiki/pages"), access).await?;
		Ok(parse_wiki_page_listing(&json)?)
	}

	pub async fn wiki_page(&self, subreddit: &str, page: &str, query: &WikiPageQuery, access: Access) -> Result<WikiPage, ServiceError> {
		validate_subreddit(subreddit)?;
		let page = encode_wiki_page(page)?;
		validate_wiki_page_query(query)?;
		let path = with_query(format!("/r/{subreddit}/wiki/{page}"), encode_wiki_page_query(query));
		let json = self.client.json(path, access).await?;
		Ok(parse_wiki_page(&json)?)
	}

	pub async fn wiki_revisions(&self, subreddit: &str, page: Option<&str>, query: &ListingQuery, access: Access) -> Result<Listing<WikiRevision>, ServiceError> {
		validate_subreddit(subreddit)?;
		validate_wiki_listing_query(query)?;
		let base = match page {
			Some(page) => format!("/r/{subreddit}/wiki/revisions/{}", encode_wiki_page(page)?),
			None => format!("/r/{subreddit}/wiki/revisions"),
		};
		let path = with_query(base, encode_listing_query(query));
		let json = self.client.json(path, access).await?;
		let mut listing = parse_wiki_revisions(&json)?;
		clear_listing_modhash(&mut listing);
		Ok(listing)
	}

	pub async fn wiki_discussions(&self, subreddit: &str, page: &str, query: &ListingQuery, access: Access) -> Result<Listing<Thing<Post>>, ServiceError> {
		validate_subreddit(subreddit)?;
		validate_listing_query(query)?;
		let page = encode_wiki_page(page)?;
		let path = with_query(format!("/r/{subreddit}/wiki/discussions/{page}"), encode_listing_query(query));
		let json = self.client.json(path, access).await?;
		let mut listing = parse_wiki_discussions(&json)?;
		clear_listing_modhash(&mut listing);
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

fn encode_wiki_page_query(query: &WikiPageQuery) -> String {
	let mut serializer = Serializer::new(String::new());
	if let Some(revision) = &query.v {
		serializer.append_pair("v", revision);
	}
	if let Some(revision) = &query.v2 {
		serializer.append_pair("v2", revision);
	}
	serializer.finish()
}

fn encode_listing_query(query: &ListingQuery) -> String {
	let mut serializer = Serializer::new(String::new());
	append_listing_query(&mut serializer, query);
	serializer.finish()
}
