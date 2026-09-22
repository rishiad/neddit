use crate::models::{Thing, TrophyList, User};
use crate::parsing::core::{parse_thing, validate_thing_kind};
use crate::parsing::error::ParseError;
use serde::Deserialize;
use serde_json::Value;

pub(crate) fn parse_user(json: &Value) -> Result<Thing<User>, ParseError> {
	parse_thing(json, "t2", "user")
}

pub(crate) fn parse_trophy_list(json: &Value) -> Result<TrophyList, ParseError> {
	let list = TrophyList::deserialize(json).map_err(|source| ParseError::InvalidPayload { entity: "trophy list", source })?;
	for trophy in &list.data.trophies {
		validate_thing_kind(trophy, "t6", "trophy")?;
	}
	Ok(list)
}
