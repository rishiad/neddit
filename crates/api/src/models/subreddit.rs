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
	pub description: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub description_html: Option<String>,
	pub community_icon: String,
	pub icon_img: String,
	pub subscribers: u64,
	pub accounts_active: Option<u64>,
	pub wiki_enabled: Option<bool>,
	pub over18: bool,
	pub subreddit_type: String,
	pub url: String,
	pub created_utc: f64,
	#[serde(flatten)]
	pub extra: Map<String, Value>,
}
