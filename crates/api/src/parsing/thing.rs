use crate::models::Thing;
use crate::parsing::error::ParseError;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

pub(super) fn parse_thing<T>(json: &Value, expected_kind: &str, entity: &'static str) -> Result<Thing<T>, ParseError>
where
	T: DeserializeOwned,
{
	let thing = Thing::<T>::deserialize(json).map_err(|source| ParseError::InvalidPayload { entity, source })?;
	validate_thing_kind(&thing, expected_kind, entity)?;
	Ok(thing)
}

pub(super) fn validate_thing_kind<T>(thing: &Thing<T>, expected_kind: &str, entity: &'static str) -> Result<(), ParseError> {
	if thing.kind == expected_kind {
		return Ok(());
	}

	Err(ParseError::UnsupportedKind { entity, kind: thing.kind.clone() })
}
