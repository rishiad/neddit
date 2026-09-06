use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ListingData<T> {
	pub after: Option<String>,
	pub dist: Option<u64>,
	#[serde(default)]
	pub modhash: Option<String>,
	pub geo_filter: Option<String>,
	pub children: Vec<T>,
	pub before: Option<String>,
	#[serde(flatten)]
	pub extra: Map<String, Value>,
}
