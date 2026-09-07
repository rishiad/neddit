use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct More {
	pub id: String,
	pub name: String,
	pub parent_id: String,
	pub count: u64,
	pub children: Vec<String>,
	pub depth: Option<u64>,
	#[serde(flatten)]
	pub extra: Map<String, Value>,
}
