use crate::models::SubredditRules;
use crate::parsing::error::ParseError;
use serde::Deserialize;
use serde_json::Value;

pub fn parse_subreddit_rules(json: &Value) -> Result<SubredditRules, ParseError> {
	let rules = SubredditRules::deserialize(json).map_err(|source| ParseError::InvalidPayload {
		entity: "subreddit rules",
		source,
	})?;
	for rule in &rules.rules {
		if !matches!(rule.kind.as_str(), "all" | "link" | "comment") {
			return Err(ParseError::UnsupportedKind {
				entity: "subreddit rule",
				kind: rule.kind.clone(),
			});
		}
	}
	Ok(rules)
}
