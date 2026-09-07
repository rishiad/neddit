use crate::models::{Comment, Post, Subreddit, Thing, User};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum PublicThing {
	Comment(Thing<Comment>),
	User(Thing<User>),
	Post(Thing<Post>),
	Subreddit(Thing<Subreddit>),
}
