use crate::service::ListingQuery;
use utoipa::{IntoParams, ToSchema};

#[derive(Clone, Debug, Default, Eq, PartialEq, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct UserHistoryQuery {
	#[param(ignore)]
	pub listing: ListingQuery,
	#[param(minimum = 2, maximum = 10)]
	pub context: Option<u8>,
	#[param(inline)]
	pub show: Option<UserHistoryShow>,
	#[param(inline)]
	pub sort: Option<UserHistorySort>,
	#[param(rename = "type", inline)]
	pub content_type: Option<UserHistoryType>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ToSchema)]
#[schema(rename_all = "lowercase")]
pub enum UserHistoryShow {
	Given,
}

impl UserHistoryShow {
	pub(super) const fn as_str(self) -> &'static str {
		match self {
			Self::Given => "given",
		}
	}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ToSchema)]
#[schema(rename_all = "lowercase")]
pub enum UserHistorySort {
	Hot,
	New,
	Top,
	Controversial,
}

impl UserHistorySort {
	pub(super) const fn as_str(self) -> &'static str {
		match self {
			Self::Hot => "hot",
			Self::New => "new",
			Self::Top => "top",
			Self::Controversial => "controversial",
		}
	}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ToSchema)]
pub enum UserHistoryType {
	#[schema(rename = "links")]
	Posts,
	#[schema(rename = "comments")]
	Comments,
}

impl UserHistoryType {
	pub(super) const fn as_str(self) -> &'static str {
		match self {
			Self::Posts => "links",
			Self::Comments => "comments",
		}
	}
}
