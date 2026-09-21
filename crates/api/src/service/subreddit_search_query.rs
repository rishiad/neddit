use crate::service::ListingQuery;
use utoipa::{IntoParams, ToSchema};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize, ToSchema)]
#[schema(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum SubredditSearchSort {
	Relevance,
	Activity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize, ToSchema)]
pub enum Typeahead {
	#[serde(rename = "true")]
	True,
	#[serde(rename = "false")]
	False,
	#[serde(rename = "None")]
	None,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SubredditSearchQuery {
	#[param(ignore)]
	#[serde(flatten)]
	pub listing: ListingQuery,
	#[param(rename = "q")]
	#[serde(rename = "q")]
	pub query: String,
	pub search_query_id: Option<String>,
	pub show_users: Option<bool>,
	#[serde(default, with = "crate::service::query_codec::option_on_off", skip_serializing_if = "Option::is_none")]
	pub include_over_18: Option<bool>,
	#[param(inline)]
	pub sort: Option<SubredditSearchSort>,
	#[param(value_type = String)]
	pub typeahead_active: Option<Typeahead>,
}
