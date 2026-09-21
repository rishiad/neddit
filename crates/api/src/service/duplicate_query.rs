use crate::service::ListingQuery;
use utoipa::{IntoParams, ToSchema};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize, ToSchema)]
#[schema(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum DuplicateSort {
	NumComments,
	New,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DuplicateQuery {
	#[param(ignore)]
	#[serde(flatten)]
	pub listing: ListingQuery,
	pub crossposts_only: Option<bool>,
	#[param(inline)]
	pub sort: Option<DuplicateSort>,
	#[param(rename = "sr")]
	#[serde(rename = "sr")]
	pub subreddit: Option<String>,
}
