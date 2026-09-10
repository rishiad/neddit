use crate::view::{custom_feed_item, pagination, CustomFeedTemplate, FeedPageMode, SelectChoice};
use askama::Template;
use askama_web::WebTemplate;
use axum::{
	extract::{Query, State},
	http::{uri::Authority, HeaderMap},
};
use neddit_api::{
	feed::{expand_rank_preset, FeedPage, Request},
	media::MediaSigner,
	server::MediaProxy,
	service::{ListingTime, RedditService},
};
use url::form_urlencoded;

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

pub async fn builder(
	headers: HeaderMap,
	State(service): State<RedditService>,
	State(signer): State<MediaSigner>,
	State(media): State<MediaProxy>,
	Query(request): Query<Request>,
) -> CustomFeedTemplate {
	let authority = request_authority(&headers);
	render(service, signer, media, request, true, authority.as_ref()).await
}

pub async fn page(
	State(service): State<RedditService>,
	State(signer): State<MediaSigner>,
	State(media): State<MediaProxy>,
	Query(request): Query<Request>,
) -> CustomFeedTemplate {
	render(service, signer, media, request, false, None).await
}

async fn render(service: RedditService, signer: MediaSigner, media: MediaProxy, request: Request, show_builder: bool, authority: Option<&Authority>) -> CustomFeedTemplate {
	let mut view = form(&request, service.allows_nsfw(), show_builder);
	if request.q.trim().is_empty() {
		if !show_builder {
			view.diagnostic = "A shared feed URL requires a q parameter".into();
		}
		return view;
	}
	match service.custom_feed(&request).await {
		Ok(page) => {
			let base = if show_builder { "/feeds" } else { "/feed" };
			publish(&request, page, &signer, media.video_enabled(), base, &mut view);
			view.share_url = qualify_share_url(share_url(&request), authority);
		}
		Err(error) => view.diagnostic = error.to_string(),
	}
	view
}

fn request_authority(headers: &HeaderMap) -> Option<Authority> {
	headers.get("host")?.to_str().ok()?.parse().ok()
}

fn qualify_share_url(path: String, authority: Option<&Authority>) -> String {
	match authority {
		Some(authority) => format!("{authority}{path}"),
		None => path,
	}
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
	let previous = page.previous_page.map(|number| feed_page_url(request, number, page.frozen, base)).unwrap_or_default();
	let next = page.next_page.map(|number| feed_page_url(request, number, page.frozen, base)).unwrap_or_default();
	view.pagination = pagination(page.number, previous, next);
}

fn feed_page_url(request: &Request, page: u32, frozen: i64, base: &str) -> String {
	let mut query = form_urlencoded::Serializer::new(String::new());
	query.append_pair("q", &request.q).append_pair("frozen", &frozen.to_string());
	if request.pool() != "new" {
		query.append_pair("pool", request.pool());
	}
	if request.pool() == "top" {
		query.append_pair("t", request.time().as_str());
	}
	query.append_pair("rank", expand_rank_preset(request.rank()));
	if request.limit() != 25 {
		query.append_pair("limit", &request.limit().to_string());
	}
	if request.include_nsfw {
		query.append_pair("include_nsfw", "true");
	}
	if page > 1 {
		query.append_pair("page", &page.to_string());
	}
	format!("{base}?{}", query.finish())
}

fn share_url(request: &Request) -> String {
	let mut query = form_urlencoded::Serializer::new(String::new());
	query.append_pair("q", request.q.trim());
	if request.pool() != "new" {
		query.append_pair("pool", request.pool());
	}
	if request.pool() == "top" {
		query.append_pair("t", request.time().as_str());
	}
	query.append_pair("rank", expand_rank_preset(request.rank()));
	if request.limit() != 25 {
		query.append_pair("limit", &request.limit().to_string());
	}
	if request.include_nsfw {
		query.append_pair("include_nsfw", "true");
	}
	format!("/feed?{}", query.finish())
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

