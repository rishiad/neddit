use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Thing<T> {
	pub kind: String,
	pub data: T,
	#[serde(flatten)]
	pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ListingData<T> {
	pub after: Option<String>,
	pub dist: Option<u64>,
	#[serde(default)]
	pub modhash: Option<String>,
	pub geo_filter: Option<String>,
	pub children: Vec<T>,
	pub before: Option<String>,
	#[serde(flatten)]
	pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Listing<T> {
	pub kind: String,
	pub data: ListingData<T>,
	#[serde(flatten)]
	pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PublicThing {
	Comment(Thing<super::Comment>),
	User(Thing<super::User>),
	Post(Thing<super::Post>),
	Subreddit(Thing<super::Subreddit>),
}
