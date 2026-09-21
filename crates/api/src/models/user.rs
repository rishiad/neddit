use super::Thing;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct User {
	pub id: String,
	pub name: String,
	pub created: f64,
	pub created_utc: f64,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub total_karma: Option<i64>,
	pub comment_karma: i64,
	pub link_karma: i64,
	pub is_gold: bool,
	pub is_mod: bool,
	pub has_verified_email: bool,
	pub subreddit: Option<Value>,
	#[serde(flatten)]
	pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TrophyList {
	pub kind: String,
	pub data: TrophyListData,
	#[serde(flatten)]
	pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TrophyListData {
	pub trophies: Vec<Thing<Trophy>>,
	#[serde(flatten)]
	pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Trophy {
	pub icon_70: String,
	pub name: String,
	pub url: Option<String>,
	pub icon_40: String,
	pub award_id: Option<String>,
	pub id: Option<String>,
	pub description: Option<String>,
	pub granted_at: Option<f64>,
	#[serde(flatten)]
	pub extra: Map<String, Value>,
}
