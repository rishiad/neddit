use crate::client::Access;
use crate::models::{Sidebar, SubredditRules};
use crate::parsing::error::ParseError;
use crate::parsing::subreddit_rules::parse_subreddit_rules;
use crate::service::reddit::validate_subreddit;
use crate::service::{RedditService, ServiceError};

impl RedditService {
	pub async fn subreddit_rules(&self, subreddit: &str, access: Access) -> Result<SubredditRules, ServiceError> {
		validate_subreddit(subreddit)?;
		self.require_safe_subreddit(subreddit, access).await?;
		let json = self.client.json(format!("/r/{subreddit}/about/rules"), access).await?;
		Ok(parse_subreddit_rules(&json)?)
	}

	pub async fn subreddit_sidebar(&self, subreddit: &str, access: Access) -> Result<Sidebar, ServiceError> {
		let subreddit = self.subreddit_about(subreddit, access).await?;
		Ok(Sidebar {
			description: subreddit.data.description,
			description_html: subreddit.data.description_html.ok_or(ParseError::InvalidField {
				entity: "subreddit",
				field: "description_html",
			})?,
		})
	}
}
