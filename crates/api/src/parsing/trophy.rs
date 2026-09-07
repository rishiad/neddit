use crate::models::TrophyList;
use crate::parsing::error::ParseError;
use crate::parsing::thing::validate_thing_kind;
use serde::Deserialize;
use serde_json::Value;

pub fn parse_trophy_list(json: &Value) -> Result<TrophyList, ParseError> {
	let trophies = TrophyList::deserialize(json).map_err(|source| ParseError::InvalidPayload { entity: "trophy list", source })?;
	if trophies.kind != "TrophyList" {
		return Err(ParseError::UnsupportedKind {
			entity: "trophy list",
			kind: trophies.kind,
		});
	}
	for trophy in &trophies.data.trophies {
		validate_thing_kind(trophy, "t6", "trophy list child")?;
	}
	Ok(trophies)
}
