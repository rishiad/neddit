use crate::models::{Comment, More, Thing};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum CommentChild {
	Comment(Thing<Comment>),
	More(Thing<More>),
}
