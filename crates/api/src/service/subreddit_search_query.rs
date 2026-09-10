use crate::service::ListingQuery;
use utoipa::{IntoParams, ToSchema};

#[derive(Clone, Copy, Debug, PartialEq, Eq, ToSchema)]
#[schema(rename_all = "lowercase")]
pub enum SubredditSearchSort {
	Relevance,
	Activity,
}

impl SubredditSearchSort {
	pub(super) const fn as_str(self) -> &'static str {
		match self {
			Self::Relevance => "relevance",
			Self::Activity => "activity",
		}
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ToSchema)]
pub enum Typeahead {
	Boolean(bool),
	None,
}

impl Typeahead {
	pub(super) const fn as_str(self) -> &'static str {
		match self {
			Self::Boolean(true) => "true",
			Self::Boolean(false) => "false",
			Self::None => "None",
		}
	}
}

#[derive(Clone, Debug, PartialEq, Eq, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SubredditSearchQuery {
	#[param(ignore)]
	pub listing: ListingQuery,
	#[param(rename = "q")]
	pub query: String,
	pub search_query_id: Option<String>,
	pub show_users: Option<bool>,
	pub include_over_18: Option<bool>,
	#[param(inline)]
	pub sort: Option<SubredditSearchSort>,
	#[param(value_type = String)]
	pub typeahead_active: Option<Typeahead>,
}
