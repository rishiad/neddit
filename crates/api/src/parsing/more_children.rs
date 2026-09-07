use crate::models::MoreChildren;
use crate::parsing::comments::validate_comment_children;
use crate::parsing::error::ParseError;
use serde::Deserialize;
use serde_json::Value;

pub fn parse_more_children(json: &Value) -> Result<MoreChildren, ParseError> {
	let response = MoreChildren::deserialize(json).map_err(|source| ParseError::InvalidPayload { entity: "more children", source })?;
	validate_comment_children(&response.json.data.things)?;
	Ok(response)
}
