use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Subreddit {
	pub id: String,
	pub name: String,
	pub display_name: String,
	pub display_name_prefixed: String,
	pub title: String,
	pub public_description: String,
	pub description: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub description_html: Option<String>,
	pub community_icon: String,
	pub icon_img: Option<String>,
	pub subscribers: Option<u64>,
	pub accounts_active: Option<u64>,
	pub wiki_enabled: Option<bool>,
	pub over18: Option<bool>,
	pub subreddit_type: String,
	pub url: String,
	pub created_utc: f64,
	#[serde(flatten)]
	pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Sidebar {
	pub description: Option<String>,
	pub description_html: String,
}

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
