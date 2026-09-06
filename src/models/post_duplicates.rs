use crate::models::{Listing, Post, Thing};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PostDuplicates(pub Listing<Thing<Post>>, pub Listing<Thing<Post>>);
