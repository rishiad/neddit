use crate::models::MoreChildren;
use crate::parsing::more_children::parse_more_children;
use crate::service::query_codec;
use crate::service::reddit::{invalid_parameter, validate_id36, with_query};
use crate::service::sanitize::clear_more_children_modhash;
use crate::service::{RedditService, ServiceError};

impl RedditService {
	pub async fn more_children(&self, query: &MoreChildrenQuery) -> Result<MoreChildren, ServiceError> {
		validate_more_children_query(query)?;
		if !self.content.allows_nsfw() && self.posts_by_id(&query.link_id).await?.data.children.is_empty() {
			return Err(ServiceError::ContentBlocked);
		}
		let _permit = self.more_children_gate.acquire().await.expect("the more-children request semaphore is never closed");
		let path = with_query("/api/morechildren".to_string(), query_codec::encode(query));
		let json = self.client.json(path).await?;
		let mut response = parse_more_children(&json)?;
		clear_more_children_modhash(&mut response);
		Ok(response)
	}
}

fn validate_more_children_query(query: &MoreChildrenQuery) -> Result<(), ServiceError> {
	if query.children.is_empty() || query.children.len() > 100 {
		return Err(invalid_parameter("children", &query.children.join(",")));
	}
	for child in &query.children {
		validate_id36("children", child)?;
	}
	let Some(link_id) = query.link_id.strip_prefix("t3_") else {
		return Err(invalid_parameter("link_id", &query.link_id));
	};
	validate_id36("link_id", link_id)?;
	if let Some(id) = &query.id {
		if id != "_" {
			validate_id36("id", id)?;
		}
	}
	Ok(())
}

mod more_children_query {
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
}

pub use more_children_query::{MoreChildrenApiType, MoreChildrenQuery};
