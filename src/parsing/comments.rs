use crate::models::{CommentChild, CommentReplies, PostComments};
use crate::parsing::error::ParseError;
use crate::parsing::listing::validate_listing_kind;
use crate::parsing::posts::validate_post_listing;
use crate::parsing::thing::validate_thing_kind;
use serde::Deserialize;
use serde_json::Value;

pub fn parse_comments(json: &Value) -> Result<PostComments, ParseError> {
	let comments = PostComments::deserialize(json).map_err(|source| ParseError::InvalidPayload { entity: "post comments", source })?;

	validate_post_listing(&comments.0)?;
	validate_listing_kind(&comments.1, "comment listing")?;
	validate_comment_children(&comments.1.data.children)?;
	Ok(comments)
}

pub(super) fn validate_comment_children(children: &[CommentChild]) -> Result<(), ParseError> {
	for child in children {
		match child {
			CommentChild::Comment(comment) => {
				validate_thing_kind(comment, "t1", "comment")?;
				match &comment.data.replies {
					CommentReplies::Empty(value) if !value.is_empty() => {
						return Err(ParseError::InvalidField {
							entity: "comment",
							field: "replies",
						})
					}
					CommentReplies::Listing(listing) => {
						validate_listing_kind(listing, "comment replies")?;
						validate_comment_children(&listing.data.children)?;
					}
					CommentReplies::Empty(_) => {}
				}
			}
			CommentChild::More(more) => validate_thing_kind(more, "more", "more comments")?,
		}
	}

	Ok(())
}
