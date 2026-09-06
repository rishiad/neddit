use crate::models::{Thing, User};
use crate::parsing::error::ParseError;
use crate::parsing::thing::parse_thing;
use serde_json::Value;

pub fn parse_user(json: &Value) -> Result<Thing<User>, ParseError> {
	parse_thing(json, "t2", "user")
}
