use crate::service::ListingQuery;
use std::{fmt, str::FromStr};
use utoipa::{IntoParams, ToSchema};

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SearchQuery {
	#[param(ignore)]
	#[serde(flatten)]
	pub listing: ListingQuery,
	#[param(max_length = 5)]
	pub category: Option<String>,
	pub include_facets: Option<bool>,
	#[serde(default, with = "crate::service::query_codec::option_on_off", skip_serializing_if = "Option::is_none")]
	pub include_over_18: Option<bool>,
	#[param(rename = "q", max_length = 512)]
	#[serde(rename = "q")]
	pub query: String,
	pub restrict_sr: Option<bool>,
	#[param(inline)]
	pub sort: Option<SearchSort>,
	#[param(rename = "type", explode = false, inline, required = false)]
	#[serde(rename = "type", default, with = "crate::service::query_codec::comma", skip_serializing_if = "Vec::is_empty")]
	pub result_types: Vec<SearchResultType>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize, ToSchema)]
#[schema(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum SearchSort {
	Relevance,
	Hot,
	Top,
	New,
	Comments,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize, ToSchema)]
pub enum SearchResultType {
	#[schema(rename = "sr")]
	#[serde(rename = "sr")]
	Subreddit,
	#[schema(rename = "link")]
	#[serde(rename = "link")]
	Post,
	#[schema(rename = "user")]
	#[serde(rename = "user")]
	User,
}

impl SearchResultType {
	pub(super) const fn as_str(self) -> &'static str {
		match self {
			Self::Subreddit => "sr",
			Self::Post => "link",
			Self::User => "user",
		}
	}
}

impl fmt::Display for SearchResultType {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(self.as_str())
	}
}

impl FromStr for SearchResultType {
	type Err = &'static str;

	fn from_str(value: &str) -> Result<Self, Self::Err> {
		match value {
			"sr" => Ok(Self::Subreddit),
			"link" => Ok(Self::Post),
			"user" => Ok(Self::User),
			_ => Err("unknown search result type"),
		}
	}
}
