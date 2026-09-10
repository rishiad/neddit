use crate::client::Access;
use crate::models::MoreChildren;
use crate::parsing::more_children::parse_more_children;
use crate::service::reddit::{bool_string, invalid_parameter, validate_id36, with_query};
use crate::service::sanitize::clear_more_children_modhash;
use crate::service::{MoreChildrenQuery, RedditService, ServiceError};
use url::form_urlencoded::Serializer;

impl RedditService {
	pub async fn more_children(&self, query: &MoreChildrenQuery, access: Access) -> Result<MoreChildren, ServiceError> {
		validate_more_children_query(query)?;
		if !self.content.allows_nsfw() && self.posts_by_id(&query.link_id, access).await?.data.children.is_empty() {
			return Err(ServiceError::ContentBlocked);
		}
		let _permit = self.more_children_gate.acquire().await.expect("the more-children request semaphore is never closed");
		let path = with_query("/api/morechildren".to_string(), encode_more_children_query(query));
		let json = self.client.json(path, access).await?;
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

fn encode_more_children_query(query: &MoreChildrenQuery) -> String {
	let mut serializer = Serializer::new(String::new());
	serializer.append_pair("api_type", query.api_type.unwrap_or(crate::service::MoreChildrenApiType::Json).as_str());
	serializer.append_pair("children", &query.children.join(","));
	if let Some(depth) = query.depth {
		serializer.append_pair("depth", &depth.to_string());
	}
	if let Some(id) = &query.id {
		serializer.append_pair("id", id);
	}
	if let Some(limit_children) = query.limit_children {
		serializer.append_pair("limit_children", bool_string(limit_children));
	}
	serializer.append_pair("link_id", &query.link_id);
	if let Some(sort) = query.sort {
		serializer.append_pair("sort", sort.as_str());
	}
	serializer.finish()
}
