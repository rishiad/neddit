use crate::view::{search_choices, search_result, SearchTemplate, SelectChoice};
use askama::Template;
use askama_web::WebTemplate;
use axum::extract::{Query, State};
use neddit_api::{
	search::Request,
	service::{ListingTime, RedditService},
};
use url::form_urlencoded;

#[derive(Template, WebTemplate)]
#[template(path = "search_sorts.html")]
pub struct SearchControls {
	sorts: Vec<SelectChoice>,
	times: Vec<SelectChoice>,
}

fn time_choices(request: &Request, sorts: &[SelectChoice]) -> Vec<SelectChoice> {
	if !sorts.iter().any(|s| s.checked && s.value == "top") {
		return Vec::new();
	}
	[
		(ListingTime::Hour, "Past hour"),
		(ListingTime::Day, "Past 24 hours"),
		(ListingTime::Week, "Past week"),
		(ListingTime::Month, "Past month"),
		(ListingTime::Year, "Past year"),
		(ListingTime::All, "All time"),
	]
	.into_iter()
	.map(|(time, label)| SelectChoice {
		value: time.as_str(),
		label,
		checked: request.top_time() == time,
	})
	.collect()
}

// This route has no Reddit service or OAuth dependency.
pub async fn controls(Query(request): Query<Request>) -> SearchControls {
	let (_, sorts, _) = search_choices(request.kind.as_str(), request.sort(), request.limit());
	SearchControls {
		times: time_choices(&request, &sorts),
		sorts,
	}
}

fn excerpt(source: &str, start: usize, end: usize) -> (String, String, String) {
	match (source.get(..start), source.get(start..end), source.get(end..)) {
		(Some(prefix), Some(text), Some(suffix)) => (prefix.into(), text.into(), suffix.into()),
		_ => (source.into(), String::new(), String::new()),
	}
}

pub async fn page(State(service): State<RedditService>, Query(request): Query<Request>) -> SearchTemplate {
	let kind = request.kind.as_str();
	let mut view = form(&request);
	if request.q.trim().is_empty() {
		return view;
	}
	match service.search_ql(&request).await {
		Ok(page) => {
			view.results = page.items.iter().filter_map(search_result).collect();
			view.result_count = view.results.len();
			view.searched = true;
			if let Some(cursor) = page.cursor {
				let mut params = form_urlencoded::Serializer::new(String::new());
				params
					.append_pair("q", &request.q)
					.append_pair("kind", kind)
					.append_pair("limit", &request.limit().to_string())
					.append_pair("include_nsfw", if request.include_nsfw { "true" } else { "false" })
					.append_pair("cursor", &cursor);
				if let Some(sort) = &request.sort {
					params.append_pair("sort", sort);
				}
				if request.sort() == "top" {
					params.append_pair("t", request.top_time().as_str());
				}
				view.next_url = format!("/search?{}", params.finish());
				view.has_next = true;
			}
		}
		Err(error) => {
			view.diagnostic = error.to_string();
			(view.error_prefix, view.error_text, view.error_suffix) = excerpt(&request.q, error.start, error.end);
		}
	}
	view
}

fn form(request: &Request) -> SearchTemplate {
	let (kinds, sorts, limits) = search_choices(request.kind.as_str(), request.sort(), request.limit());
	SearchTemplate {
		query: request.q.clone(),
		include_nsfw: request.include_nsfw,
		kinds,
		times: time_choices(request, &sorts),
		sorts,
		limits,
		results: Vec::new(),
		result_count: 0,
		searched: false,
		next_url: String::new(),
		has_next: false,
		diagnostic: String::new(),
		error_prefix: String::new(),
		error_text: String::new(),
		error_suffix: String::new(),
	}
}

