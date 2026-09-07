use crate::models::ListingData;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Listing<T> {
	pub kind: String,
	pub data: ListingData<T>,
	#[serde(flatten)]
	pub extra: Map<String, Value>,
}
