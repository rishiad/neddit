use crate::models::CommentReplies;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Comment {
	pub id: String,
	pub name: String,
	pub parent_id: String,
	pub link_id: String,
	pub author: String,
	pub body: String,
	pub body_html: Option<String>,
	pub score: i64,
	pub created_utc: f64,
	pub edited: Value,
	pub replies: CommentReplies,
	#[serde(flatten)]
	pub extra: Map<String, Value>,
}
