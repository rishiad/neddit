use crate::models::CommentChild;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MoreChildren {
	pub json: MoreChildrenJson,
	#[serde(flatten)]
	pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MoreChildrenJson {
	#[serde(default)]
	pub errors: Vec<Value>,
	pub data: MoreChildrenData,
	#[serde(flatten)]
	pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MoreChildrenData {
	#[serde(default)]
	pub things: Vec<CommentChild>,
	#[serde(flatten)]
	pub extra: Map<String, Value>,
}
