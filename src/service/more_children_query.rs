use crate::service::CommentSort;
use utoipa::{IntoParams, ToSchema};

#[derive(Clone, Copy, Debug, Eq, PartialEq, ToSchema)]
#[schema(rename_all = "lowercase")]
pub enum MoreChildrenApiType {
	Json,
}

impl MoreChildrenApiType {
	pub(super) const fn as_str(self) -> &'static str {
		match self {
			Self::Json => "json",
		}
	}
}

#[derive(Clone, Debug, Default, Eq, PartialEq, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct MoreChildrenQuery {
	#[param(inline)]
	pub api_type: Option<MoreChildrenApiType>,
	#[param(explode = false)]
	pub children: Vec<String>,
	pub depth: Option<u32>,
	pub id: Option<String>,
	pub limit_children: Option<bool>,
	pub link_id: String,
	#[param(inline)]
	pub sort: Option<CommentSort>,
}
