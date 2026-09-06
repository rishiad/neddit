use crate::service::{ListingQuery, SubredditSearchSort, Typeahead};
use utoipa::IntoParams;

#[derive(Clone, Debug, Default, Eq, PartialEq, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct UserSearchQuery {
	#[param(ignore)]
	pub listing: ListingQuery,
	#[param(rename = "q")]
	pub query: String,
	pub search_query_id: Option<String>,
	#[param(inline)]
	pub sort: Option<SubredditSearchSort>,
	#[param(value_type = String)]
	pub typeahead_active: Option<Typeahead>,
}
