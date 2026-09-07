use crate::service::ListingQuery;
use utoipa::{IntoParams, ToSchema};

#[derive(Clone, Copy, Debug, PartialEq, Eq, ToSchema)]
#[schema(rename_all = "snake_case")]
pub enum DuplicateSort {
	NumComments,
	New,
}

impl DuplicateSort {
	pub(super) const fn as_str(self) -> &'static str {
		match self {
			Self::NumComments => "num_comments",
			Self::New => "new",
		}
	}
}

#[derive(Clone, Debug, Default, PartialEq, Eq, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DuplicateQuery {
	#[param(ignore)]
	pub listing: ListingQuery,
	pub crossposts_only: Option<bool>,
	#[param(inline)]
	pub sort: Option<DuplicateSort>,
	#[param(rename = "sr")]
	pub subreddit: Option<String>,
}
