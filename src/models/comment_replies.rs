use crate::models::{CommentChild, Listing};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum CommentReplies {
	Empty(String),
	#[schema(no_recursion)]
	Listing(Box<Listing<CommentChild>>),
}
