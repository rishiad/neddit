use crate::service::CommentSort;
use utoipa::{IntoParams, ToSchema};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize, ToSchema)]
#[schema(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum MoreChildrenApiType {
	#[default]
	Json,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct MoreChildrenQuery {
	#[param(inline)]
	#[serde(default)]
	pub api_type: MoreChildrenApiType,
	#[param(explode = false)]
	#[serde(default, with = "crate::service::query_codec::comma", skip_serializing_if = "Vec::is_empty")]
	pub children: Vec<String>,
	pub depth: Option<u32>,
	pub id: Option<String>,
	pub limit_children: Option<bool>,
	pub link_id: String,
	#[param(inline)]
	pub sort: Option<CommentSort>,
}
