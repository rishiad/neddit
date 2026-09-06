use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SubredditRules {
	pub rules: Vec<SubredditRule>,
	pub site_rules: Vec<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub site_rules_flow: Option<Vec<Value>>,
	#[serde(flatten)]
	pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SubredditRule {
	pub created_utc: i64,
	pub description: String,
	pub description_html: String,
	pub kind: String,
	pub priority: u64,
	pub short_name: String,
	pub violation_reason: String,
	#[serde(flatten)]
	pub extra: Map<String, Value>,
}
