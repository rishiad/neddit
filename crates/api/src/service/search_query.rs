use crate::service::ListingQuery;
use utoipa::{IntoParams, ToSchema};

#[derive(Clone, Debug, Default, Eq, PartialEq, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SearchQuery {
	#[param(ignore)]
	pub listing: ListingQuery,
	#[param(max_length = 5)]
	pub category: Option<String>,
	pub include_facets: Option<bool>,
	pub include_over_18: Option<bool>,
	#[param(rename = "q", max_length = 512)]
	pub query: String,
	pub restrict_sr: Option<bool>,
	#[param(inline)]
	pub sort: Option<SearchSort>,
	#[param(rename = "type", explode = false, inline, required = false)]
	pub result_types: Vec<SearchResultType>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ToSchema)]
#[schema(rename_all = "lowercase")]
pub enum SearchSort {
	Relevance,
	Hot,
	Top,
	New,
	Comments,
}

impl SearchSort {
	pub(super) const fn as_str(self) -> &'static str {
		match self {
			Self::Relevance => "relevance",
			Self::Hot => "hot",
			Self::Top => "top",
			Self::New => "new",
			Self::Comments => "comments",
		}
	}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ToSchema)]
pub enum SearchResultType {
	#[schema(rename = "sr")]
	Subreddit,
	#[schema(rename = "link")]
	Post,
	#[schema(rename = "user")]
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
