use crate::models::{Listing, PublicThing, Thing, User};
use crate::parsing::error::ParseError;
use crate::parsing::listing::parse_listing;
use crate::parsing::thing::validate_thing_kind;
use serde_json::Value;

pub fn parse_public_listing(json: &Value) -> Result<Listing<PublicThing>, ParseError> {
	let listing: Listing<PublicThing> = parse_listing(json)?;
	for child in &listing.data.children {
		if child.kind() != child.expected_kind() {
			return Err(ParseError::UnsupportedKind {
				entity: "public listing child",
				kind: child.kind().to_string(),
			});
		}
	}
	Ok(listing)
}

pub fn parse_info_listing(json: &Value) -> Result<Listing<PublicThing>, ParseError> {
	let listing = parse_public_listing(json)?;
	ensure_children(&listing, "info listing child", |child| !matches!(child, PublicThing::User(_)))?;
	Ok(listing)
}

pub fn parse_search_listing(json: &Value) -> Result<Listing<PublicThing>, ParseError> {
	let listing = parse_public_listing(json)?;
	ensure_children(&listing, "search listing child", |child| {
		matches!(child, PublicThing::User(_) | PublicThing::Post(_) | PublicThing::Subreddit(_))
	})?;
	Ok(listing)
}

pub fn parse_user_overview_listing(json: &Value) -> Result<Listing<PublicThing>, ParseError> {
	let listing = parse_public_listing(json)?;
	ensure_children(&listing, "user overview child", |child| matches!(child, PublicThing::Comment(_) | PublicThing::Post(_)))?;
	Ok(listing)
}

pub fn parse_user_comment_listing(json: &Value) -> Result<Listing<PublicThing>, ParseError> {
	let listing = parse_public_listing(json)?;
	ensure_children(&listing, "user comment child", |child| matches!(child, PublicThing::Comment(_)))?;
	Ok(listing)
}

pub fn parse_user_listing(json: &Value) -> Result<Listing<Thing<User>>, ParseError> {
	let listing = parse_listing(json)?;
	for child in &listing.data.children {
		validate_thing_kind(child, "t2", "user listing child")?;
	}
	Ok(listing)
}

fn ensure_children(listing: &Listing<PublicThing>, entity: &'static str, allowed: impl Fn(&PublicThing) -> bool) -> Result<(), ParseError> {
	for child in &listing.data.children {
		if !allowed(child) {
			return Err(ParseError::UnsupportedKind {
				entity,
				kind: child.kind().to_string(),
			});
		}
	}
	Ok(())
}

impl PublicThing {
	fn kind(&self) -> &str {
		match self {
			Self::Comment(thing) => &thing.kind,
			Self::User(thing) => &thing.kind,
			Self::Post(thing) => &thing.kind,
			Self::Subreddit(thing) => &thing.kind,
		}
	}

	fn expected_kind(&self) -> &'static str {
		match self {
			Self::Comment(_) => "t1",
			Self::User(_) => "t2",
			Self::Post(_) => "t3",
			Self::Subreddit(_) => "t5",
		}
	}
}
