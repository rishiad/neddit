use super::{age, author_path, compact, display_author, signed_compact, CommentTreeEvent, CommentView};
use crate::markdown;
use neddit_api::models::{Comment, CommentChild, CommentReplies, More};
use std::collections::HashSet;
use url::form_urlencoded;

pub fn comment_tree(children: &[CommentChild], link_id: &str, sort: &str, renderer: markdown::Renderer<'_>) -> Vec<CommentTreeEvent> {
	let mut tree = Vec::new();
	append_comments(children, link_id, sort, renderer, &mut tree);
	tree
}

pub fn loaded_comment_tree(
	children: &[CommentChild],
	parent_id: &str,
	link_id: &str,
	sort: &str,
	remaining: &[String],
	renderer: markdown::Renderer<'_>,
) -> Vec<CommentTreeEvent> {
	let mut tree = Vec::new();
	let mut visited = HashSet::new();
	append_flat_comments(children, parent_id, link_id, sort, renderer, &mut visited, &mut tree);
	for child in children {
		if !visited.contains(child_name(child)) {
			append_flat_comment(child, children, link_id, sort, renderer, &mut visited, &mut tree);
		}
	}
	if !remaining.is_empty() {
		tree.push(CommentTreeEvent::Open(Box::new(CommentView {
			id: String::new(),
			author: String::new(),
			author_url: String::new(),
			body: format!("{} more replies", compact(remaining.len() as u64)),
			body_html: String::new(),
			score: String::new(),
			age: String::new(),
			permalink: String::new(),
			parent_url: String::new(),
			more_url: more_comments_url(remaining, link_id, parent_id, sort),
			more: true,
		})));
		tree.push(CommentTreeEvent::Close);
	}
	tree
}

pub fn search_comment_tree(comments: &[Comment], renderer: markdown::Renderer<'_>) -> Vec<CommentTreeEvent> {
	let mut tree = Vec::with_capacity(comments.len() * 2);
	for comment in comments {
		tree.push(CommentTreeEvent::Open(Box::new(comment_view(comment, renderer))));
		tree.push(CommentTreeEvent::Close);
	}
	tree
}

fn append_comments(children: &[CommentChild], link_id: &str, sort: &str, renderer: markdown::Renderer<'_>, tree: &mut Vec<CommentTreeEvent>) {
	for child in children {
		match child {
			CommentChild::Comment(comment) => {
				let data = &comment.data;
				tree.push(CommentTreeEvent::Open(Box::new(comment_view(data, renderer))));
				if let CommentReplies::Listing(replies) = &data.replies {
					if !replies.data.children.is_empty() {
						tree.push(CommentTreeEvent::OpenReplies);
						append_comments(&replies.data.children, link_id, sort, renderer, tree);
						tree.push(CommentTreeEvent::CloseReplies);
					}
				}
				tree.push(CommentTreeEvent::Close);
			}
			CommentChild::More(more) => {
				tree.push(CommentTreeEvent::Open(Box::new(more_view(&more.data, link_id, sort))));
				tree.push(CommentTreeEvent::Close);
			}
		}
	}
}

fn comment_view(comment: &Comment, renderer: markdown::Renderer<'_>) -> CommentView {
	CommentView {
		id: comment.id.clone(),
		author: display_author(&comment.author),
		author_url: author_path(&comment.author),
		body: String::new(),
		body_html: renderer.render(&comment.body, comment.extra.get("media_metadata")),
		score: signed_compact(comment.score),
		age: age(comment.created_utc),
		permalink: format!("#comment-{}", comment.id),
		parent_url: comment_parent_url(comment),
		more_url: String::new(),
		more: false,
	}
}

fn more_view(more: &More, link_id: &str, sort: &str) -> CommentView {
	CommentView {
		id: more.id.clone(),
		author: String::new(),
		author_url: String::new(),
		body: if more.count == 0 {
			"Continue this thread".into()
		} else {
			format!("{} more replies", compact(more.count))
		},
		body_html: String::new(),
		score: String::new(),
		age: String::new(),
		permalink: if more.children.is_empty() {
			continue_thread_url(link_id, &more.parent_id, sort)
		} else {
			String::new()
		},
		parent_url: String::new(),
		more_url: more_comments_url(&more.children, link_id, &more.parent_id, sort),
		more: true,
	}
}

fn continue_thread_url(link_id: &str, parent_id: &str, sort: &str) -> String {
	let (Some(article), Some(parent)) = (link_id.strip_prefix("t3_"), parent_id.strip_prefix("t1_")) else {
		return String::new();
	};
	let mut url = format!("/comments/{article}/_/{parent}");
	if sort != "best" {
		let mut query = form_urlencoded::Serializer::new(String::new());
		query.append_pair("sort", sort);
		url.push('?');
		url.push_str(&query.finish());
	}
	url.push_str("#comment-");
	url.push_str(parent);
	url
}

pub(super) fn comment_parent_url(comment: &Comment) -> String {
	let article = comment.link_id.trim_start_matches("t3_");
	comment
		.parent_id
		.strip_prefix("t1_")
		.map_or_else(|| format!("/comments/{article}#post"), |parent| format!("/comments/{article}/_/{parent}#comment-{parent}"))
}

pub(super) fn comment_url(comment: &Comment) -> String {
	let article = comment.link_id.trim_start_matches("t3_");
	format!("/comments/{article}/_/{}#comment-{}", comment.id, comment.id)
}

fn more_comments_url(children: &[String], link_id: &str, parent_id: &str, sort: &str) -> String {
	if children.is_empty() {
		return String::new();
	}
	let mut query = form_urlencoded::Serializer::new(String::new());
	query.append_pair("children", &children.join(","));
	query.append_pair("link_id", link_id);
	query.append_pair("parent_id", parent_id);
	if sort != "best" {
		query.append_pair("sort", sort);
	}
	format!("/more-comments?{}", query.finish())
}

fn append_flat_comments(
	children: &[CommentChild],
	parent_id: &str,
	link_id: &str,
	sort: &str,
	renderer: markdown::Renderer<'_>,
	visited: &mut HashSet<String>,
	tree: &mut Vec<CommentTreeEvent>,
) {
	for child in children {
		if child_parent_id(child) == parent_id && !visited.contains(child_name(child)) {
			append_flat_comment(child, children, link_id, sort, renderer, visited, tree);
		}
	}
}

fn append_flat_comment(
	child: &CommentChild,
	children: &[CommentChild],
	link_id: &str,
	sort: &str,
	renderer: markdown::Renderer<'_>,
	visited: &mut HashSet<String>,
	tree: &mut Vec<CommentTreeEvent>,
) {
	if !visited.insert(child_name(child).to_owned()) {
		return;
	}

	match child {
		CommentChild::Comment(comment) => {
			tree.push(CommentTreeEvent::Open(Box::new(comment_view(&comment.data, renderer))));
			let mut replies = Vec::new();
			if let CommentReplies::Listing(listing) = &comment.data.replies {
				append_comments(&listing.data.children, link_id, sort, renderer, &mut replies);
			}
			append_flat_comments(children, &comment.data.name, link_id, sort, renderer, visited, &mut replies);
			if !replies.is_empty() {
				tree.push(CommentTreeEvent::OpenReplies);
				tree.extend(replies);
				tree.push(CommentTreeEvent::CloseReplies);
			}
			tree.push(CommentTreeEvent::Close);
		}
		CommentChild::More(more) => {
			tree.push(CommentTreeEvent::Open(Box::new(more_view(&more.data, link_id, sort))));
			tree.push(CommentTreeEvent::Close);
		}
	}
}

fn child_name(child: &CommentChild) -> &str {
	match child {
		CommentChild::Comment(comment) => &comment.data.name,
		CommentChild::More(more) => &more.data.name,
	}
}

fn child_parent_id(child: &CommentChild) -> &str {
	match child {
		CommentChild::Comment(comment) => &comment.data.parent_id,
		CommentChild::More(more) => &more.data.parent_id,
	}
}
