use crate::client::{error::ClientError, RedditClient};
use crate::media::{validate_media_target, MediaSigner, MediaUrlError};
use crate::video::{decode_video_target, rewrite_hls as rewrite_video_hls, validate_video_target, VideoError, VideoPlayback, VideoResolver};
use axum::{
	body::{to_bytes, Body},
	extract::{Path, Query, State},
	http::{header, Extensions, HeaderMap, HeaderValue, StatusCode, Version},
	middleware::{self, Next},
	response::{IntoResponse, Response},
	routing::get,
	Router,
};
use serde::Deserialize;
use std::{io, str};
use thiserror::Error;
use tower_http::compression::{
	predicate::{Predicate, SizeAbove},
	CompressionLayer,
};
use url::Url;

const COMPRESSION_MIN_SIZE: u16 = 1452;
const MANIFEST_SIZE_LIMIT: usize = 8 * 1024 * 1024;
const MAX_MEDIA_REDIRECTS: usize = 4;
const MEDIA_CACHE_CONTROL: &str = "public, max-age=604800, immutable";
const VIDEO_CACHE_CONTROL: &str = "public, max-age=900";

#[derive(Clone)]
pub struct MediaProxy {
	client: RedditClient,
	signer: MediaSigner,
	video: VideoResolver,
}

impl MediaProxy {
	pub fn new(client: RedditClient, signer: MediaSigner, video: VideoResolver) -> Self {
		Self { client, signer, video }
	}

	pub fn signer(&self) -> &MediaSigner {
		&self.signer
	}

	pub async fn resolve_video(&self, url: &str) -> Result<VideoPlayback, VideoError> {
		self.video.resolve(url, &self.signer).await
	}
}

pub fn router(media: MediaProxy) -> Router {
	Router::new()
		.route("/media/{signature}/{encoded}", get(proxy_media))
		.route("/video/resolve", get(resolve_video))
		.route("/video/media/{signature}/{encoded}", get(proxy_video))
		.with_state(media)
}

#[derive(Deserialize)]
pub(crate) struct VideoQuery {
	url: String,
}

#[utoipa::path(
	get,
	path = "/video/resolve",
	params(("url" = String, Query, description = "Supported HTTPS video page URL")),
	responses(
		(status = 200, description = "Resolved metadata and signed proxy sources", body = VideoPlayback),
		(status = 400, description = "Invalid or unsupported provider URL"),
		(status = 502, description = "Video extraction failed"),
		(status = 503, description = "Extractor concurrency limit reached")
	),
	tag = "media"
)]
pub(crate) async fn resolve_video(State(media): State<MediaProxy>, Query(query): Query<VideoQuery>) -> Result<axum::Json<VideoPlayback>, VideoError> {
	media.resolve_video(&query.url).await.map(axum::Json)
}

pub fn with_middleware(app: Router) -> Router {
	app
		.layer(CompressionLayer::new().compress_when(SizeAbove::new(COMPRESSION_MIN_SIZE).and(compressible_content)))
		.layer(middleware::from_fn(default_headers))
}

pub async fn listen(app: Router, address: &str) -> io::Result<()> {
	let listener = tokio::net::TcpListener::bind(address).await?;
	axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await
}

async fn proxy_media(State(media): State<MediaProxy>, Path((signature, encoded)): Path<(String, String)>, headers: HeaderMap) -> Result<Response, ProxyError> {
	let mut target = media.signer.decode_target(&signature, &encoded)?;
	let etag = format!("\"{signature}\"");
	if request_has_etag(&headers, &etag) {
		return not_modified(&etag, MEDIA_CACHE_CONTROL);
	}

	for redirect in 0..=MAX_MEDIA_REDIRECTS {
		let response = media.client.proxy(target.to_string(), &headers).await?;
		if response.status() == StatusCode::NOT_MODIFIED || !response.status().is_redirection() {
			return finish_media_response(response, &target, &media.signer, &etag).await;
		}
		if redirect == MAX_MEDIA_REDIRECTS {
			return Err(ProxyError::TooManyRedirects);
		}
		let location = response
			.headers()
			.get(header::LOCATION)
			.ok_or(ProxyError::MissingRedirectLocation)?
			.to_str()
			.map_err(|_| ProxyError::InvalidRedirect)?;
		let redirected = target.join(location).map_err(|_| ProxyError::InvalidRedirect)?;
		validate_media_target(&redirected).map_err(|_| ProxyError::ForbiddenRedirect)?;
		target = redirected;
	}

	Err(ProxyError::TooManyRedirects)
}

async fn proxy_video(State(media): State<MediaProxy>, Path((signature, encoded)): Path<(String, String)>, headers: HeaderMap) -> Result<Response, ProxyError> {
	let mut target = decode_video_target(&media.signer, &signature, &encoded)?;
	let user_agent = media.video.user_agent_for(&target).await;
	let etag = format!("\"{signature}\"");
	if request_has_etag(&headers, &etag) {
		return not_modified(&etag, VIDEO_CACHE_CONTROL);
	}

	for redirect in 0..=MAX_MEDIA_REDIRECTS {
		let response = media.client.proxy_external(target.to_string(), &headers, user_agent.as_deref()).await?;
		if response.status() == StatusCode::NOT_MODIFIED || !response.status().is_redirection() {
			return finish_video_response(response, &target, &media.signer, &etag).await;
		}
		if redirect == MAX_MEDIA_REDIRECTS {
			return Err(ProxyError::TooManyRedirects);
		}
		let location = response
			.headers()
			.get(header::LOCATION)
			.ok_or(ProxyError::MissingRedirectLocation)?
			.to_str()
			.map_err(|_| ProxyError::InvalidRedirect)?;
		let redirected = target.join(location).map_err(|_| ProxyError::InvalidRedirect)?;
		validate_video_target(&redirected).map_err(|_| ProxyError::ForbiddenRedirect)?;
		target = redirected;
	}

	Err(ProxyError::TooManyRedirects)
}

async fn finish_media_response(mut response: Response, target: &Url, signer: &MediaSigner, etag: &str) -> Result<Response, ProxyError> {
	if response.status().is_success() && is_hls(&response, target) {
		response = rewrite_manifest(response, |manifest| signer.rewrite_hls(manifest, target)).await?;
	} else if response.status().is_success() && is_dash(&response, target) {
		response = rewrite_manifest(response, |manifest| signer.rewrite_dash(manifest, target)).await?;
	}

	if response.status().is_success() || response.status() == StatusCode::NOT_MODIFIED {
		let headers = response.headers_mut();
		headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(MEDIA_CACHE_CONTROL));
		headers.insert(header::ETAG, HeaderValue::from_str(etag).map_err(|_| ProxyError::InvalidEtag)?);
		headers.remove(header::LOCATION);
	}
	Ok(response)
}

async fn finish_video_response(mut response: Response, target: &Url, signer: &MediaSigner, etag: &str) -> Result<Response, ProxyError> {
	if response.status().is_success() && is_hls(&response, target) {
		response = rewrite_manifest(response, |manifest| rewrite_video_hls(manifest, target, signer)).await?;
	}

	if response.status().is_success() || response.status() == StatusCode::NOT_MODIFIED {
		let headers = response.headers_mut();
		headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(VIDEO_CACHE_CONTROL));
		headers.insert(header::ETAG, HeaderValue::from_str(etag).map_err(|_| ProxyError::InvalidEtag)?);
		headers.remove(header::LOCATION);
	}
	Ok(response)
}

async fn rewrite_manifest<F, E>(response: Response, rewrite: F) -> Result<Response, ProxyError>
where
	F: FnOnce(&str) -> Result<String, E>,
	ProxyError: From<E>,
{
	let (mut parts, body) = response.into_parts();
	let body = to_bytes(body, MANIFEST_SIZE_LIMIT).await.map_err(ProxyError::ManifestBody)?;
	let manifest = str::from_utf8(&body).map_err(|_| ProxyError::InvalidManifest)?;
	let body = rewrite(manifest).map_err(ProxyError::from)?;
	parts.headers.remove(header::CONTENT_LENGTH);
	parts.headers.remove(header::CONTENT_ENCODING);
	Ok(Response::from_parts(parts, Body::from(body)))
}

fn is_hls(response: &Response, target: &Url) -> bool {
	target.path().ends_with(".m3u8") || content_type(response).is_some_and(|value| value.contains("mpegurl"))
}

fn is_dash(response: &Response, target: &Url) -> bool {
	target.path().ends_with(".mpd") || content_type(response).is_some_and(|value| value.contains("dash+xml"))
}

fn content_type(response: &Response) -> Option<&str> {
	response.headers().get(header::CONTENT_TYPE)?.to_str().ok()
}

fn request_has_etag(headers: &HeaderMap, etag: &str) -> bool {
	headers
		.get(header::IF_NONE_MATCH)
		.and_then(|value| value.to_str().ok())
		.is_some_and(|value| value.split(',').any(|value| value.trim() == etag))
}

fn not_modified(etag: &str, cache_control: &'static str) -> Result<Response, ProxyError> {
	let mut response = Response::new(Body::empty());
	*response.status_mut() = StatusCode::NOT_MODIFIED;
	response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static(cache_control));
	response
		.headers_mut()
		.insert(header::ETAG, HeaderValue::from_str(etag).map_err(|_| ProxyError::InvalidEtag)?);
	Ok(response)
}

async fn default_headers(request: axum::extract::Request, next: Next) -> Response {
	let mut response = next.run(request).await;
	response.headers_mut().insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
	response.headers_mut().insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
	response
}

fn compressible_content(_: StatusCode, _: Version, headers: &HeaderMap, _: &Extensions) -> bool {
	headers
		.get(header::CONTENT_TYPE)
		.and_then(|value| value.to_str().ok())
		.is_some_and(|value| value.starts_with("text/") || value.starts_with("application/json"))
}

#[derive(Debug, Error)]
enum ProxyError {
	#[error(transparent)]
	MediaUrl(#[from] MediaUrlError),
	#[error(transparent)]
	Video(#[from] VideoError),
	#[error("Reddit media request failed")]
	Client(#[from] ClientError),
	#[error("Reddit media redirect omitted its destination")]
	MissingRedirectLocation,
	#[error("Reddit returned an invalid media redirect")]
	InvalidRedirect,
	#[error("Reddit redirected media outside its allowed hosts")]
	ForbiddenRedirect,
	#[error("Reddit media exceeded the redirect limit")]
	TooManyRedirects,
	#[error("Reddit returned an invalid media manifest")]
	InvalidManifest,
	#[error("failed to read Reddit media manifest")]
	ManifestBody(#[source] axum::Error),
	#[error("failed to construct media ETag")]
	InvalidEtag,
}

impl IntoResponse for ProxyError {
	fn into_response(self) -> Response {
		let status = match self {
			Self::MediaUrl(MediaUrlError::InvalidSignature) | Self::MediaUrl(MediaUrlError::ForbiddenTarget) | Self::ForbiddenRedirect => StatusCode::FORBIDDEN,
			Self::MediaUrl(MediaUrlError::InvalidEncoding | MediaUrlError::InvalidTarget) => StatusCode::BAD_REQUEST,
			Self::Video(error) => return error.into_response(),
			_ => StatusCode::BAD_GATEWAY,
		};
		(status, self.to_string()).into_response()
	}
}

async fn shutdown_signal() {
	#[cfg(windows)]
	{
		if let Err(error) = tokio::signal::ctrl_c().await {
			log::error!("failed to install CTRL+C signal handler: {error}");
		}
	}

	#[cfg(unix)]
	{
		let terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate());
		match terminate {
			Ok(mut terminate) => {
				tokio::select! {
					result = tokio::signal::ctrl_c() => {
						if let Err(error) = result {
							log::error!("failed to install CTRL+C signal handler: {error}");
						}
					}
					_ = terminate.recv() => {}
				}
			}
			Err(error) => {
				log::error!("failed to install SIGTERM signal handler: {error}");
				if let Err(error) = tokio::signal::ctrl_c().await {
					log::error!("failed to install CTRL+C signal handler: {error}");
				}
			}
		}
	}
}
