use crate::models::{Listing, Thing};
use crate::parsing::error::ParseError;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

pub(crate) fn parse_listing<T>(json: &Value) -> Result<Listing<T>, ParseError>
where
	T: DeserializeOwned,
{
	let listing = Listing::<T>::deserialize(json).map_err(|source| ParseError::InvalidPayload { entity: "listing", source })?;
	validate_listing_kind(&listing, "listing")?;
	Ok(listing)
}

pub(crate) fn validate_listing_kind<T>(listing: &Listing<T>, entity: &'static str) -> Result<(), ParseError> {
	if listing.kind == "Listing" {
		Ok(())
	} else {
		Err(ParseError::UnsupportedKind {
			entity,
			kind: listing.kind.clone(),
		})
	}
}

pub(crate) fn parse_thing<T>(json: &Value, expected_kind: &str, entity: &'static str) -> Result<Thing<T>, ParseError>
where
	T: DeserializeOwned,
{
	let thing = Thing::<T>::deserialize(json).map_err(|source| ParseError::InvalidPayload { entity, source })?;
	validate_thing_kind(&thing, expected_kind, entity)?;
	Ok(thing)
}

pub(crate) fn validate_thing_kind<T>(thing: &Thing<T>, expected_kind: &str, entity: &'static str) -> Result<(), ParseError> {
	if thing.kind == expected_kind {
		Ok(())
	} else {
		Err(ParseError::UnsupportedKind { entity, kind: thing.kind.clone() })
	}
}
