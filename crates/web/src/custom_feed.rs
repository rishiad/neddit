use crate::view::{custom_feed_item, pagination, CustomFeedTemplate, FeedPageMode, SelectChoice};
use askama::Template;
use askama_web::WebTemplate;
use axum::{
	extract::{Path, Query, State},
	http::{HeaderMap, StatusCode},
	response::{IntoResponse, Redirect, Response},
	Form,
};
use neddit_api::{
	feed::{expand_rank_preset, Continuation, FeedPage, Request},
	media::MediaSigner,
	server::MediaProxy,
	service::{ListingTime, RedditService},
};

#[derive(Template, WebTemplate)]
#[template(path = "feed_pool.html")]
pub struct FeedControls {
	pools: Vec<SelectChoice>,
	times: Vec<SelectChoice>,
}

// This fragment has no Reddit service or OAuth dependency.
pub async fn controls(Query(request): Query<Request>) -> FeedControls {
	FeedControls {
		pools: pool_choices(&request),
		times: time_choices(&request),
	}
}

pub async fn builder(State(service): State<RedditService>, State(signer): State<MediaSigner>, State(media): State<MediaProxy>, Query(request): Query<Request>) -> Response {
	render(service, signer, media, request, "/feeds").await
}

pub async fn page(State(service): State<RedditService>, State(signer): State<MediaSigner>, State(media): State<MediaProxy>, Query(request): Query<Request>) -> Response {
	render(service, signer, media, request, "/feed").await
}

pub async fn create(State(service): State<RedditService>, headers: HeaderMap, Form(request): Form<Request>) -> Response {
	if headers.get("sec-fetch-site").is_some_and(|site| site == "cross-site") {
		return (StatusCode::FORBIDDEN, "Cross-site feed creation is not allowed").into_response();
	}
	match service.save_feed(&request).await {
		Ok(feed) => Redirect::to(&format!("/f/{}", feed.id)).into_response(),
		Err(error) => (neddit_api::feed::status(&error), error.to_string()).into_response(),
	}
}

pub async fn saved(
	State(service): State<RedditService>,
	State(signer): State<MediaSigner>,
	State(media): State<MediaProxy>,
	Path(id): Path<String>,
	Query(continuation): Query<Continuation>,
) -> Response {
	match service.saved_feed(&id, continuation).await {
		Ok(request) => render(service, signer, media, request, &format!("/f/{id}")).await,
		Err(error) => (neddit_api::feed::status(&error), error.to_string()).into_response(),
	}
}

async fn render(service: RedditService, signer: MediaSigner, media: MediaProxy, request: Request, base: &str) -> Response {
	let show_builder = base == "/feeds";
	let mut view = form(&request, service.allows_nsfw(), show_builder);
	view.create_action = service.shortlinks_enabled().then_some("/feeds");
	view.share_url = if let Some(id) = base.strip_prefix("/f/") {
		service.feed_url(id).unwrap_or_else(|| base.into())
	} else if !request.q.trim().is_empty() {
		request.url("/feed", None)
	} else {
		String::new()
	};
	if request.q.trim().is_empty() {
		if !show_builder {
			view.diagnostic = "A shared feed URL requires a q parameter".into();
		}
		return (if show_builder { StatusCode::OK } else { StatusCode::BAD_REQUEST }, view).into_response();
	}
	let status = match service.custom_feed(&request).await {
		Ok(page) => {
			publish(&request, page, &signer, media.video_enabled(), base, &mut view);
			StatusCode::OK
		}
		Err(error) => {
			view.diagnostic = error.to_string();
			neddit_api::feed::status(&error)
		}
	};
	(status, view).into_response()
}

fn publish(request: &Request, page: FeedPage, signer: &MediaSigner, video_enabled: bool, base: &str, view: &mut CustomFeedTemplate) {
	view.items = page
		.items
		.iter()
		.map(|result| custom_feed_item(&result.post.data, signer, video_enabled, request.include_nsfw))
		.collect();
	view.result_count = view.items.len();
	view.searched = true;
	view.coverage = page.coverage;
	let link = |number| page.cursor.as_deref().map(|cursor| request.url(base, Some((number, cursor)))).unwrap_or_default();
	let previous = page.previous_page.map(link).unwrap_or_default();
	let next = page.next_page.map(link).unwrap_or_default();
	view.pagination = pagination(page.number, previous, next);
}

fn choices(values: &[(&'static str, &'static str)], active: &str) -> Vec<SelectChoice> {
	values
		.iter()
		.map(|&(value, label)| SelectChoice {
			value,
			label,
			checked: value == active,
		})
		.collect()
}

fn pool_choices(request: &Request) -> Vec<SelectChoice> {
	choices(&[("new", "New"), ("top", "Top")], request.pool())
}

fn time_choices(request: &Request) -> Vec<SelectChoice> {
	if request.pool() != "top" {
		return Vec::new();
	}
	choices(
		&[
			(ListingTime::Hour.as_str(), "Past hour"),
			(ListingTime::Day.as_str(), "Past 24 hours"),
			(ListingTime::Week.as_str(), "Past week"),
			(ListingTime::Month.as_str(), "Past month"),
			(ListingTime::Year.as_str(), "Past year"),
			(ListingTime::All.as_str(), "All time"),
		],
		request.time().as_str(),
	)
}

fn form(request: &Request, nsfw_available: bool, show_builder: bool) -> CustomFeedTemplate {
	CustomFeedTemplate {
		mode: if show_builder { FeedPageMode::Builder } else { FeedPageMode::Shared },
		share_url: String::new(),
		create_action: None,
		query: request.q.clone(),
		rank: expand_rank_preset(request.rank()).into(),
		include_nsfw: request.include_nsfw,
		nsfw_available,
		pools: pool_choices(request),
		times: time_choices(request),
		limits: choices(&[("25", "25"), ("50", "50"), ("100", "100")], &request.limit().to_string()),
		items: Vec::new(),
		feed_label: "Custom feed posts".into(),
		result_count: 0,
		searched: false,
		diagnostic: String::new(),
		coverage: Vec::new(),
		pagination: pagination(1, String::new(), String::new()),
	}
}

