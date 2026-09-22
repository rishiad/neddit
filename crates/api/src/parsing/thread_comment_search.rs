use std::collections::HashSet;

use scraper::{Html, Selector};
use serde_json::Value;

use crate::parsing::error::ParseError;

pub(crate) fn parse_thread_comment_ids(html: &str, post_id: &str) -> Result<Vec<String>, ParseError> {
	let document = Html::parse_fragment(html);
	let selector = Selector::parse(r#"[data-testid="search-comment"]"#).expect("static selector is valid");
	let mut seen = HashSet::new();
	let mut ids = Vec::new();

	for card in document.select(&selector) {
		let tracking = card.value().attr("data-faceplate-tracking-context").ok_or(ParseError::InvalidField {
			entity: "thread comment search result",
			field: "data-faceplate-tracking-context",
		})?;
		let tracking: Value = serde_json::from_str(tracking).map_err(|source| ParseError::InvalidPayload {
			entity: "thread comment search tracking context",
			source,
		})?;
		let comment = tracking.get("comment").ok_or(ParseError::InvalidField {
			entity: "thread comment search tracking context",
			field: "comment",
		})?;
		let id = comment.get("id").and_then(Value::as_str).ok_or(ParseError::InvalidField {
			entity: "thread comment search tracking context",
			field: "comment.id",
		})?;
		let tracked_post = comment.get("post_id").and_then(Value::as_str).ok_or(ParseError::InvalidField {
			entity: "thread comment search tracking context",
			field: "comment.post_id",
		})?;
		let id = id.strip_prefix("t1_").unwrap_or(id);
		let tracked_post = tracked_post.strip_prefix("t3_").unwrap_or(tracked_post);
		if tracked_post != post_id || id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
			return Err(ParseError::InvalidField {
				entity: "thread comment search tracking context",
				field: "comment identity",
			});
		}
		let fullname = format!("t1_{id}");
		if seen.insert(fullname.clone()) {
			ids.push(fullname);
		}
	}

	Ok(ids)
}
