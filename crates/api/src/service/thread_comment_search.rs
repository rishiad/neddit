use std::collections::HashMap;

use url::form_urlencoded::Serializer;

use crate::{
	models::{Comment, PublicThing},
	parsing::thread_comment_search::parse_thread_comment_ids,
	service::{
		reddit::{validate_id36, validate_subreddit},
		CommentSort, InfoQuery, RedditService, ServiceError,
	},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadCommentSearchQuery {
	pub query: String,
	pub sort: CommentSort,
}

impl RedditService {
	pub async fn search_post_comments(&self, subreddit: &str, post_id: &str, query: &ThreadCommentSearchQuery) -> Result<Vec<Comment>, ServiceError> {
		validate_subreddit(subreddit)?;
		self.require_safe_subreddit(subreddit).await?;
		validate_id36("post_id", post_id)?;
		let search = query.query.trim();
		if search.is_empty() || search.chars().count() > 512 {
			return Err(super::reddit::invalid_parameter("q", &query.query));
		}
		let path = thread_comment_search_path(subreddit, post_id, search, query.sort);
		let html = self.client.html(path).await?;
		let ids = parse_thread_comment_ids(&html, post_id)?;
		if ids.is_empty() {
			return Ok(Vec::new());
		}

		let mut hydrated = HashMap::new();
		for batch in ids.chunks(100) {
			let listing = self
				.info(&InfoQuery {
					ids: batch.to_vec(),
					..InfoQuery::default()
				})
				.await?;
			for item in listing.data.children {
				if let PublicThing::Comment(comment) = item {
					if comment.data.link_id != format!("t3_{post_id}") {
						return Err(ServiceError::InvalidThreadCommentSearch);
					}
					hydrated.insert(comment.data.name.clone(), comment.data);
				}
			}
		}

		ids.into_iter().map(|id| hydrated.remove(&id).ok_or(ServiceError::InvalidThreadCommentSearch)).collect()
	}
}

fn thread_comment_search_path(subreddit: &str, post_id: &str, query: &str, sort: CommentSort) -> String {
	let mut parameters = Serializer::new(String::new());
	parameters
		.append_pair("q", query)
		.append_pair("type", "comments")
		.append_pair("sort", sort.as_str())
		.append_pair("render-mode", "partial");
	format!("/svc/shreddit/r/{subreddit}/{post_id}/pdp-comment-search-results?{}", parameters.finish())
}

