use crate::media::{DomainSet, MediaSigner, MediaUrlError};
use axum::{
	http::StatusCode,
	response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::{
	collections::HashMap,
	io,
	path::{Path, PathBuf},
	process::Stdio,
	sync::Arc,
	time::{Duration, Instant},
};
use thiserror::Error;
use tokio::{
	io::{AsyncRead, AsyncReadExt},
	process::Command,
	sync::{Mutex, Semaphore},
	time::timeout,
};
use url::{Host, Url};
use utoipa::ToSchema;

const CACHE_TTL: Duration = Duration::from_secs(2 * 60 * 60);
const DNS_TIMEOUT: Duration = Duration::from_secs(5);
const EXTRACT_QUEUE_TIMEOUT: Duration = Duration::from_secs(2);
const EXTRACT_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_EXTRACTORS: usize = 2;
const MAX_STDOUT: usize = 1024 * 1024;
const MAX_STDERR: usize = 64 * 1024;
const VIDEO_ROUTE: &str = "/video/media";
const VIDEO_SCOPE: &str = "external-video";
const OUTPUT_TEMPLATE: &str = concat!(
	"{\"id\":%(id)j,\"extractor\":%(extractor_key)j,\"title\":%(title|null)j,\"duration\":%(duration|null)j,\"thumbnail\":%(thumbnail|null)j,\"format\":{",
	"\"format_id\":%(format_id|null)j,\"url\":%(url|null)j,\"ext\":%(ext|null)j,\"protocol\":%(protocol|null)j,",
	"\"width\":%(width|null)j,\"height\":%(height|null)j,\"fps\":%(fps|null)j,\"vcodec\":%(vcodec|null)j,",
	"\"acodec\":%(acodec|null)j,\"aspect_ratio\":%(aspect_ratio|null)j,\"http_headers\":%(http_headers|null)j}}",
);

#[derive(Clone)]
pub struct VideoResolver {
	executable: Arc<PathBuf>,
	excluded_domains: DomainSet,
	cache: Arc<Mutex<HashMap<String, CachedVideo>>>,
	permits: Arc<Semaphore>,
}

#[derive(Clone)]
struct CachedVideo {
	stored: Instant,
	video: ExtractedVideo,
}

#[derive(Clone, Debug, Deserialize)]
struct ExtractedVideo {
	id: String,
	extractor: String,
	title: Option<String>,
	duration: Option<f64>,
	thumbnail: Option<String>,
	formats: Vec<ExtractedFormat>,
}

#[derive(Debug, Deserialize)]
struct ExtractedLine {
	id: String,
	extractor: String,
	title: Option<String>,
	duration: Option<f64>,
	thumbnail: Option<String>,
	format: ExtractedFormat,
}

#[derive(Clone, Debug, Deserialize)]
struct ExtractedFormat {
	format_id: Option<String>,
	url: Option<String>,
	ext: Option<String>,
	protocol: Option<String>,
	width: Option<f64>,
	height: Option<f64>,
	fps: Option<f64>,
	aspect_ratio: Option<f64>,
	vcodec: Option<String>,
	acodec: Option<String>,
	#[serde(default)]
	http_headers: Option<HashMap<String, String>>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct VideoPlayback {
	pub id: String,
	pub title: Option<String>,
	pub duration: Option<f64>,
	pub poster: Option<String>,
	pub sources: Vec<VideoSource>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct VideoSource {
	pub url: String,
	pub format_id: Option<String>,
	pub mime: &'static str,
	pub width: Option<u32>,
	pub height: Option<u32>,
	pub fps: Option<f64>,
}

pub(crate) fn validate_video_target(url: &Url) -> Result<(), VideoError> {
	let Host::Domain(host) = url.host().ok_or(VideoError::InvalidTarget)? else {
		return Err(VideoError::ForbiddenTarget);
	};
	let host = host.trim_end_matches('.').to_ascii_lowercase();
	if url.scheme() != "https"
		|| !url.username().is_empty()
		|| url.password().is_some()
		|| url.port().is_some()
		|| host == "localhost"
		|| host.ends_with(".localhost")
		|| host.ends_with(".local")
		|| host.ends_with(".internal")
	{
		return Err(VideoError::ForbiddenTarget);
	}
	Ok(())
}

async fn validate_public_resolution(url: &Url) -> Result<(), VideoError> {
	let host = url.host_str().ok_or(VideoError::InvalidUrl)?;
	let addresses = timeout(DNS_TIMEOUT, tokio::net::lookup_host((host, 443)))
		.await
		.map_err(|_| VideoError::ResolutionTimeout)?
		.map_err(VideoError::ResolutionFailed)?;
	let mut found = false;
	for address in addresses {
		found = true;
		if !crate::client::is_public_ip(address.ip()) {
			return Err(VideoError::ForbiddenTarget);
		}
	}
	if !found {
		return Err(VideoError::InvalidUrl);
	}
	Ok(())
}

impl VideoResolver {
	pub fn new(executable: impl AsRef<Path>, excluded_domains: DomainSet) -> Self {
		Self {
			executable: Arc::new(executable.as_ref().to_owned()),
			excluded_domains,
			cache: Arc::new(Mutex::new(HashMap::new())),
			permits: Arc::new(Semaphore::new(MAX_EXTRACTORS)),
		}
	}

	pub fn allows(&self, input: &str) -> bool {
		let Ok(url) = Url::parse(input) else {
			return false;
		};
		validate_video_target(&url).is_ok() && url.host_str().is_some_and(|host| !self.excluded_domains.matches(host))
	}

	pub async fn resolve(&self, input: &str, signer: &MediaSigner) -> Result<VideoPlayback, VideoError> {
		let url = Url::parse(input).map_err(|_| VideoError::InvalidUrl)?;
		validate_video_target(&url).map_err(|_| VideoError::InvalidUrl)?;
		if url.host_str().is_some_and(|host| self.excluded_domains.matches(host)) {
			return Err(VideoError::ExcludedDomain);
		}
		validate_public_resolution(&url).await?;
		let cache_key = url.to_string();

		if let Some(video) = self.cached(&cache_key).await {
			return playback(video, signer);
		}

		let _permit = timeout(EXTRACT_QUEUE_TIMEOUT, self.permits.acquire())
			.await
			.map_err(|_| VideoError::Busy)?
			.map_err(|_| VideoError::Busy)?;

		if let Some(video) = self.cached(&cache_key).await {
			return playback(video, signer);
		}

		let video = self.extract(url.as_str()).await?;
		self.cache.lock().await.insert(
			cache_key,
			CachedVideo {
				stored: Instant::now(),
				video: video.clone(),
			},
		);
		playback(video, signer)
	}

	async fn cached(&self, key: &str) -> Option<ExtractedVideo> {
		let mut cache = self.cache.lock().await;
		cache.retain(|_, entry| entry.stored.elapsed() < CACHE_TTL);
		cache.get(key).map(|entry| entry.video.clone())
	}

	pub async fn user_agent_for(&self, target: &Url) -> Option<String> {
		let target = target.as_str();
		self.cache.lock().await.values().find_map(|entry| {
			entry
				.video
				.formats
				.iter()
				.find(|format| format.url.as_deref() == Some(target))?
				.http_headers
				.as_ref()?
				.get("User-Agent")
				.cloned()
		})
	}

	async fn extract(&self, url: &str) -> Result<ExtractedVideo, VideoError> {
		let mut command = Command::new(self.executable.as_ref());
		command
			.args([
				"--ignore-config",
				"--no-cache-dir",
				"--no-playlist",
				"--no-warnings",
				"--no-plugin-dirs",
				"--no-remote-components",
				"--extractor-args",
				"youtube:player_client=mweb",
				"--extractor-args",
				"youtube-ejs:jitless=true",
				"--socket-timeout",
				"10",
				"--force-ipv4",
				"--skip-download",
				"--check-formats",
				"--format",
				"all[vcodec!=?none][acodec!=?none]",
				"--print",
				OUTPUT_TEMPLATE,
				"--",
				url,
			])
			.stdin(Stdio::null())
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.kill_on_drop(true);

		let mut child = command.spawn().map_err(VideoError::Spawn)?;
		let stdout = child.stdout.take().ok_or(VideoError::MissingPipe)?;
		let stderr = child.stderr.take().ok_or(VideoError::MissingPipe)?;
		let operation = async {
			let (stdout, stderr, status) = tokio::try_join!(read_limited(stdout, MAX_STDOUT), read_limited(stderr, MAX_STDERR), child.wait())?;
			Ok::<_, io::Error>((stdout, stderr, status))
		};

		let result = match timeout(EXTRACT_TIMEOUT, operation).await {
			Ok(result) => result,
			Err(_) => {
				let _ = child.kill().await;
				return Err(VideoError::Timeout);
			}
		}
		.map_err(VideoError::ProcessIo)?;

		if !result.2.success() {
			let message = String::from_utf8_lossy(&result.1);
			return Err(VideoError::ExtractionFailed(message.trim().chars().take(512).collect()));
		}
		parse_output(&result.0)
	}
}

fn parse_output(output: &[u8]) -> Result<ExtractedVideo, VideoError> {
	let mut lines = output.split(|byte| *byte == b'\n').filter(|line| !line.is_empty());
	let first: ExtractedLine = serde_json::from_slice(lines.next().ok_or(VideoError::NoPlayableFormats)?).map_err(VideoError::InvalidOutput)?;
	let mut video = ExtractedVideo {
		id: first.id,
		extractor: first.extractor,
		title: first.title,
		duration: first.duration,
		thumbnail: first.thumbnail,
		formats: vec![first.format],
	};
	for line in lines {
		let line: ExtractedLine = serde_json::from_slice(line).map_err(VideoError::InvalidOutput)?;
		if line.id != video.id || line.extractor != video.extractor {
			return Err(VideoError::InvalidOutputShape);
		}
		video.formats.push(line.format);
	}
	Ok(video)
}

async fn read_limited(reader: impl AsyncRead + Unpin, limit: usize) -> io::Result<Vec<u8>> {
	let mut output = Vec::new();
	reader.take((limit + 1) as u64).read_to_end(&mut output).await?;
	if output.len() > limit {
		return Err(io::Error::other("yt-dlp output exceeded its limit"));
	}
	Ok(output)
}

fn playback(video: ExtractedVideo, signer: &MediaSigner) -> Result<VideoPlayback, VideoError> {
	let mut formats: Vec<(u32, VideoSource)> = video.formats.into_iter().filter_map(|format| source(format, signer)).collect();
	formats.sort_by_key(|format| std::cmp::Reverse(format.0));
	formats.dedup_by(|left, right| left.1.mime == right.1.mime && left.1.width == right.1.width && left.1.height == right.1.height);
	formats.truncate(12);
	let sources = formats.into_iter().map(|(_, source)| source).collect::<Vec<_>>();
	if sources.is_empty() {
		return Err(VideoError::NoPlayableFormats);
	}

	let poster = video.thumbnail.and_then(|value| Url::parse(&value).ok()).and_then(|url| {
		validate_video_target(&url).ok()?;
		Some(signer.scoped_url(VIDEO_ROUTE, VIDEO_SCOPE, &url))
	});

	Ok(VideoPlayback {
		id: video.id,
		title: video.title,
		duration: video.duration,
		poster,
		sources,
	})
}

fn source(format: ExtractedFormat, signer: &MediaSigner) -> Option<(u32, VideoSource)> {
	if format.vcodec.as_deref() == Some("none") || format.acodec.as_deref() == Some("none") {
		return None;
	}
	let upstream = Url::parse(format.url.as_deref()?).ok()?;
	validate_video_target(&upstream).ok()?;
	let hls = format.protocol.as_deref().is_some_and(|value| value.contains("m3u8")) || upstream.path().ends_with(".m3u8");
	let mime = if hls {
		"application/vnd.apple.mpegurl"
	} else {
		match format.ext.as_deref() {
			Some("mp4" | "m4v") => "video/mp4",
			Some("webm") => "video/webm",
			_ => return None,
		}
	};
	let height = finite_u32(format.height);
	let width = finite_u32(format.width).or_else(|| {
		let ratio = format.aspect_ratio?;
		finite_u32(Some(f64::from(height?) * ratio))
	});
	Some((
		height.unwrap_or_default(),
		VideoSource {
			url: signer.scoped_url(VIDEO_ROUTE, VIDEO_SCOPE, &upstream),
			format_id: format.format_id,
			mime,
			width,
			height,
			fps: format.fps.filter(|value| value.is_finite() && *value > 0.0),
		},
	))
}

fn finite_u32(value: Option<f64>) -> Option<u32> {
	let value = value?;
	(value.is_finite() && value > 0.0 && value <= f64::from(u32::MAX)).then(|| value.round() as u32)
}

pub fn decode_video_target(signer: &MediaSigner, signature: &str, encoded: &str) -> Result<Url, VideoError> {
	let target = signer.decode_scoped_target(VIDEO_SCOPE, signature, encoded)?;
	validate_video_target(&target)?;
	Ok(target)
}

pub fn rewrite_hls(manifest: &str, source: &Url, signer: &MediaSigner) -> Result<String, VideoError> {
	signer.rewrite_hls_with(manifest, |reference| rewrite_reference(reference, source, signer))
}

fn rewrite_reference(reference: &str, source: &Url, signer: &MediaSigner) -> Result<String, VideoError> {
	let target = source.join(reference).map_err(|_| VideoError::InvalidManifestUrl)?;
	validate_video_target(&target)?;
	Ok(signer.scoped_url(VIDEO_ROUTE, VIDEO_SCOPE, &target))
}

#[derive(Debug, Error)]
pub enum VideoError {
	#[error("hosted video is disabled")]
	Disabled,
	#[error("invalid video URL")]
	InvalidUrl,
	#[error("video site is excluded by server policy")]
	ExcludedDomain,
	#[error("video extraction is busy")]
	Busy,
	#[error("video host resolution timed out")]
	ResolutionTimeout,
	#[error("failed to resolve video host")]
	ResolutionFailed(#[source] io::Error),
	#[error("video extraction timed out")]
	Timeout,
	#[error("failed to start yt-dlp")]
	Spawn(#[source] io::Error),
	#[error("failed to capture yt-dlp output")]
	MissingPipe,
	#[error("yt-dlp failed: {0}")]
	ExtractionFailed(String),
	#[error("failed while running yt-dlp")]
	ProcessIo(#[source] io::Error),
	#[error("yt-dlp returned invalid JSON")]
	InvalidOutput(#[source] serde_json::Error),
	#[error("yt-dlp returned formats for multiple videos")]
	InvalidOutputShape,
	#[error("yt-dlp returned no supported combined video formats")]
	NoPlayableFormats,
	#[error("invalid extracted video target")]
	InvalidTarget,
	#[error("extracted video target is not a public HTTPS URL")]
	ForbiddenTarget,
	#[error("invalid URL in video manifest")]
	InvalidManifestUrl,
	#[error(transparent)]
	Signature(#[from] MediaUrlError),
}

impl IntoResponse for VideoError {
	fn into_response(self) -> Response {
		let status = match self {
			Self::Disabled => StatusCode::NOT_FOUND,
			Self::InvalidUrl => StatusCode::BAD_REQUEST,
			Self::ExcludedDomain => StatusCode::FORBIDDEN,
			Self::Busy => StatusCode::SERVICE_UNAVAILABLE,
			Self::Signature(MediaUrlError::InvalidSignature | MediaUrlError::ForbiddenTarget) | Self::ForbiddenTarget => StatusCode::FORBIDDEN,
			Self::Signature(MediaUrlError::InvalidEncoding | MediaUrlError::InvalidTarget) | Self::InvalidTarget => StatusCode::BAD_REQUEST,
			_ => StatusCode::BAD_GATEWAY,
		};
		(status, self.to_string()).into_response()
	}
}

