use super::{reddit::validate_id36, CommentQuery, CommentSort, RedditService, ServiceError};
use crate::models::{CommentChild, Listing, PostComments};
use crate::parsing::{parse_comments, parse_listing, parse_post_listing};
use crate::service::sanitize::clear_listing_modhash;
use chrono::DateTime;
use serde_json::{json, Value};

const PAGE_SIZE: u32 = 200;
const MAX_DEPTH: u32 = 10;

impl RedditService {
	pub async fn graphql_more_comments(&self, article: &str, cursor: &str, sort: CommentSort) -> Result<Listing<CommentChild>, ServiceError> {
		let query = CommentQuery {
			cursor: Some(cursor.to_owned()),
			sort: Some(sort),
			..CommentQuery::default()
		};
		Ok(self.graphql_comments(article, &query).await?.1)
	}

	pub(super) async fn graphql_comments(&self, article: &str, query: &CommentQuery) -> Result<PostComments, ServiceError> {
		validate_id36("article", article)?;
		let json = self.client.json(format!("/by_id/t3_{article}")).await?;
		let mut posts = parse_post_listing(&json)?;
		if posts.data.children.is_empty() {
			return Err(ServiceError::InvalidGraphqlComments);
		}
		self.content.require_post_listing(&posts)?;
		clear_listing_modhash(&mut posts);
		let comments = self.graphql_comment_listing(article, query).await?;
		let result = json!([posts, comments]);
		Ok(parse_comments(&result)?)
	}

	async fn graphql_comment_listing(&self, article: &str, query: &CommentQuery) -> Result<Listing<CommentChild>, ServiceError> {
		if let Some(comment) = &query.comment {
			validate_id36("comment", comment)?;
		}
		if query.context.is_some()
			|| query.truncate.is_some()
			|| query.threaded.is_some()
			|| query.showmore.is_some()
			|| query.showedits.is_some()
			|| query.showmedia.is_some()
			|| query.showtitle.is_some()
			|| query.sr_detail.is_some()
			|| query.theme.is_some()
		{
			return Err(ServiceError::InvalidParameter {
				parameter: "comment options",
				value: "unsupported by GraphQL".into(),
			});
		}
		let sort = query.sort.unwrap_or_default();
		if matches!(sort, CommentSort::Random | CommentSort::Qa | CommentSort::Live) {
			return Err(ServiceError::InvalidParameter {
				parameter: "sort",
				value: sort.as_str().into(),
			});
		}
		for (name, value, maximum) in [("limit", query.limit, PAGE_SIZE), ("depth", query.depth, MAX_DEPTH)] {
			if value.is_some_and(|value| value == 0 || value > maximum) {
				return Err(ServiceError::InvalidParameter {
					parameter: name,
					value: value.unwrap_or_default().to_string(),
				});
			}
		}
		let mut variables = json!({
			"id": format!("t3_{article}"),
			"sortType": sort.as_str().to_ascii_uppercase(),
			"count": query.limit.unwrap_or(PAGE_SIZE),
			"maxDepth": query.depth.unwrap_or(MAX_DEPTH),
			"includeConsistencyToken": false,
			"includePremiumAvatarTreatment": false,
		});
		if let Some(comment) = &query.comment {
			variables["focusedCommentContext"] = json!({ "commentId": format!("t1_{comment}"), "isFocusedThreadOnly": true });
		}
		if let Some(cursor) = &query.cursor {
			if cursor.len() > 2048 || cursor.is_empty() {
				return Err(ServiceError::InvalidParameter {
					parameter: "cursor",
					value: "invalid cursor".into(),
				});
			}
			variables["after"] = cursor.as_str().into();
		}
		let response = self.client.post_comments_graphql(variables).await?;
		let post = &response["data"]["postInfoById"];
		if post["id"].as_str() != Some(&format!("t3_{article}")) {
			return Err(ServiceError::InvalidGraphqlComments);
		}
		let trees = post["commentForest"]["trees"].as_array().ok_or(ServiceError::InvalidGraphqlComments)?;
		let items = trees.iter().map(|tree| item(tree, article)).collect::<Result<Vec<_>, _>>()?;
		let result = json!({ "kind": "Listing", "data": { "children": nest(items) } });
		Ok(parse_listing(&result)?)
	}
}

fn item(tree: &Value, article: &str) -> Result<(u64, Value), ServiceError> {
	let depth = tree["depth"].as_u64().ok_or(ServiceError::InvalidGraphqlComments)?;
	let post_name = format!("t3_{article}");
	let parent = tree["parentId"].as_str().unwrap_or(&post_name);
	if !tree["more"].is_null() {
		let count = tree["more"]["count"].as_u64().unwrap_or_default();
		let cursor = if tree["parentId"].is_null() {
			Some(tree["more"]["cursor"].as_str().ok_or(ServiceError::InvalidGraphqlComments)?)
		} else {
			None
		};
		return Ok((
			depth,
			json!({ "kind": "more", "data": {
			"id": "", "name": "", "parent_id": parent, "count": count, "children": [], "gql_cursor": cursor
		} }),
		));
	}
	let node = &tree["node"];
	let name = node["id"].as_str().ok_or(ServiceError::InvalidGraphqlComments)?;
	let id = name.strip_prefix("t1_").ok_or(ServiceError::InvalidGraphqlComments)?;
	let author = node["authorInfo"]["name"].as_str().unwrap_or("[deleted]");
	let body = if node["__typename"] == "DeletedComment" {
		"[deleted]"
	} else if node["isAdminTakedown"] == true {
		"[ Removed by Reddit ]"
	} else if node["isRemoved"] == true {
		"[removed]"
	} else {
		node["content"]["markdown"].as_str().unwrap_or("")
	};
	let created = node["createdAt"]
		.as_str()
		.and_then(|time| DateTime::parse_from_rfc3339(time).ok())
		.map_or(0.0, |time| time.timestamp_millis() as f64 / 1000.0);
	let edited = node["editedAt"]
		.as_str()
		.and_then(|time| DateTime::parse_from_rfc3339(time).ok())
		.map_or(Value::Bool(false), |time| json!(time.timestamp_millis() as f64 / 1000.0));
	Ok((
		depth,
		json!({ "kind": "t1", "data": {
		"id": id, "name": name, "parent_id": parent, "link_id": post_name,
		"author": author, "body": body, "body_html": null,
		"score": node["score"].as_i64().unwrap_or_default(),
		"created_utc": created, "edited": edited, "replies": "",
		"stickied": node["isStickied"], "score_hidden": node["isScoreHidden"],
		"distinguished": node["distinguishedAs"],
		"author_flair_text": node["authorFlair"]["text"]
	} }),
	))
}

fn nest(items: Vec<(u64, Value)>) -> Vec<Value> {
	let mut top = Vec::new();
	let mut open: Vec<(u64, Value)> = Vec::new();
	for (depth, item) in items {
		while open.last().is_some_and(|(level, _)| *level >= depth) {
			close(&mut open, &mut top);
		}
		open.push((depth, item));
	}
	while !open.is_empty() {
		close(&mut open, &mut top);
	}
	top
}

fn close(open: &mut Vec<(u64, Value)>, top: &mut Vec<Value>) {
	let (_, item) = open.pop().expect("only close open comments");
	let Some((_, parent)) = open.last_mut() else {
		top.push(item);
		return;
	};
	let replies = &mut parent["data"]["replies"];
	if !replies.is_object() {
		*replies = json!({ "kind": "Listing", "data": { "children": [] } });
	}
	replies["data"]["children"].as_array_mut().expect("comment replies are a listing").push(item);
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn keeps_reply_order_and_cursor() {
		let a = json!({"depth":0,"parentId":null,"more":null,"node":{"id":"t1_a","authorInfo":{"name":"alice"},"content":{"markdown":"hello"},"score":2,"createdAt":"2026-10-24T01:02:03Z","editedAt":null}});
		let b = json!({"depth":1,"parentId":"t1_a","more":null,"node":{"id":"t1_b","content":{"markdown":"reply"}}});
		let c = json!({"depth":0,"parentId":null,"more":{"count":25,"cursor":"next-page"},"node":null});
		let items = [a, b, c].iter().map(|tree| item(tree, "post").unwrap()).collect();
		let list = json!({"kind":"Listing","data":{"children":nest(items)}});
		let listing: Listing<CommentChild> = parse_listing(&list).unwrap();
		let CommentChild::Comment(first) = &listing.data.children[0] else {
			panic!("expected comment")
		};
		let crate::models::CommentReplies::Listing(replies) = &first.data.replies else {
			panic!("expected replies")
		};
		assert_eq!(replies.data.children.len(), 1);
		let CommentChild::More(more) = &listing.data.children[1] else {
			panic!("expected cursor")
		};
		assert_eq!(more.data.extra["gql_cursor"], "next-page");
	}
}
