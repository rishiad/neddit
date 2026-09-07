use crate::models::Listing;
use crate::parsing::error::ParseError;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

pub fn parse_listing<T>(json: &Value) -> Result<Listing<T>, ParseError>
where
	T: DeserializeOwned,
{
	let listing = Listing::<T>::deserialize(json).map_err(|source| ParseError::InvalidPayload { entity: "listing", source })?;
	validate_listing_kind(&listing, "listing")?;
	Ok(listing)
}

pub(super) fn validate_listing_kind<T>(listing: &Listing<T>, entity: &'static str) -> Result<(), ParseError> {
	if listing.kind == "Listing" {
		return Ok(());
	}

	Err(ParseError::UnsupportedKind {
		entity,
		kind: listing.kind.clone(),
	})
}
