use crate::models::{Listing, Subreddit, Thing};
use crate::parsing::error::ParseError;
use crate::parsing::listing::parse_listing;
use crate::parsing::thing::{parse_thing, validate_thing_kind};
use serde_json::Value;

pub fn parse_subreddit(json: &Value) -> Result<Thing<Subreddit>, ParseError> {
	parse_thing(json, "t5", "subreddit")
}

pub fn parse_subreddit_listing(json: &Value) -> Result<Listing<Thing<Subreddit>>, ParseError> {
	let listing = parse_listing(json)?;
	for child in &listing.data.children {
		validate_thing_kind(child, "t5", "subreddit listing child")?;
	}
	Ok(listing)
}
