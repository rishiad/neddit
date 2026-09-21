use crate::service::ListingQuery;
use utoipa::{IntoParams, ToSchema};

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct UserHistoryQuery {
	#[param(ignore)]
	#[serde(flatten)]
	pub listing: ListingQuery,
	#[param(minimum = 2, maximum = 10)]
	pub context: Option<u8>,
	#[param(inline)]
	pub show: Option<UserHistoryShow>,
	#[param(inline)]
	pub sort: Option<UserHistorySort>,
	#[param(rename = "type", inline)]
	#[serde(rename = "type")]
	pub content_type: Option<UserHistoryType>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize, ToSchema)]
#[schema(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum UserHistoryShow {
	Given,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize, ToSchema)]
#[schema(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum UserHistorySort {
	Hot,
	New,
	Top,
	Controversial,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize, ToSchema)]
pub enum UserHistoryType {
	#[schema(rename = "links")]
	#[serde(rename = "links")]
	Posts,
	#[schema(rename = "comments")]
	#[serde(rename = "comments")]
	Comments,
}
