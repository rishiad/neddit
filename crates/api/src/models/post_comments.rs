use crate::models::{CommentChild, Listing, Post, Thing};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PostComments(pub Listing<Thing<Post>>, pub Listing<CommentChild>);
