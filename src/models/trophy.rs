use crate::models::Thing;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use utoipa::ToSchema;

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
