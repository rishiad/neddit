use crate::models::{Listing, Post, PostDuplicates, Thing};
use crate::parsing::error::ParseError;
use crate::parsing::listing::{parse_listing, validate_listing_kind};
use crate::parsing::thing::{parse_thing, validate_thing_kind};
use serde::Deserialize;
use serde_json::Value;

pub fn parse_post(json: &Value) -> Result<Thing<Post>, ParseError> {
	parse_thing(json, "t3", "post")
}

pub fn parse_post_listing(json: &Value) -> Result<Listing<Thing<Post>>, ParseError> {
	let listing = parse_listing(json)?;
	validate_post_listing(&listing)?;
	Ok(listing)
}

pub fn parse_post_duplicates(json: &Value) -> Result<PostDuplicates, ParseError> {
	let duplicates = PostDuplicates::deserialize(json).map_err(|source| ParseError::InvalidPayload {
		entity: "post duplicates",
		source,
	})?;
	validate_post_listing(&duplicates.0)?;
	validate_post_listing(&duplicates.1)?;
	Ok(duplicates)
}

pub(super) fn validate_post_listing(listing: &Listing<Thing<Post>>) -> Result<(), ParseError> {
	validate_listing_kind(listing, "post listing")?;
	for child in &listing.data.children {
		validate_thing_kind(child, "t3", "post listing child")?;
	}

	Ok(())
}
