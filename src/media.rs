use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use regex::Regex;
use serde_json::Value;
use sha2::Sha256;
use std::sync::{Arc, LazyLock};
use thiserror::Error;
use url::Url;

type HmacSha256 = Hmac<Sha256>;

static ABSOLUTE_URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?i)(?:https?:)?//[^\s<>\"']+"#).unwrap());
static HLS_URI: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"URI="([^"]+)""#).unwrap());
static DASH_BASE_URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)<BaseURL([^>]*)>([^<]+)</BaseURL>").unwrap());

const MEDIA_ROUTE: &str = "/media";

#[derive(Clone)]
pub struct MediaSigner {
	key: Arc<[u8]>,
}

impl MediaSigner {
	pub fn random() -> Result<Self, std::io::Error> {
		let mut key = [0_u8; 32];
		getrandom::fill(&mut key).map_err(|error| std::io::Error::other(format!("failed to generate media signing key: {error}")))?;
		Ok(Self { key: Arc::from(key) })
	}

	pub fn from_secret(secret: &[u8]) -> Result<Self, MediaUrlError> {
		if secret.len() < 32 {
			return Err(MediaUrlError::ShortSecret);
		}
		Ok(Self { key: Arc::from(secret) })
	}

	pub fn rewrite_value(&self, value: &mut Value) {
		match value {
			Value::Array(values) => {
				for value in values {
					self.rewrite_value(value);
				}
			}
			Value::Object(values) => {
				for value in values.values_mut() {
					self.rewrite_value(value);
				}
			}
			Value::String(value) => {
				*value = self.rewrite_text(value);
			}
			_ => {}
		}
	}

	pub fn rewrite_text(&self, text: &str) -> String {
		if !text.contains("//") {
			return text.to_string();
		}
		if let Some(rewritten) = self.rewrite_absolute_url(text) {
			return rewritten;
		}

		ABSOLUTE_URL
			.replace_all(text, |captures: &regex::Captures<'_>| {
				let matched = &captures[0];
				let (candidate, suffix) = trim_url_punctuation(matched);
				self
					.rewrite_absolute_url(candidate)
					.map_or_else(|| matched.to_string(), |rewritten| format!("{rewritten}{suffix}"))
			})
			.into_owned()
	}

	pub fn media_url(&self, target: &Url) -> Result<String, MediaUrlError> {
		validate_media_target(target)?;
		let target = target.as_str();
		let signature = self.signature(target);
		let encoded = URL_SAFE_NO_PAD.encode(target);
		Ok(format!("{MEDIA_ROUTE}/{signature}/{encoded}"))
	}

	pub fn decode_target(&self, signature: &str, encoded: &str) -> Result<Url, MediaUrlError> {
		let bytes = URL_SAFE_NO_PAD.decode(encoded).map_err(|_| MediaUrlError::InvalidEncoding)?;
		let target = std::str::from_utf8(&bytes).map_err(|_| MediaUrlError::InvalidEncoding)?;
		let supplied = URL_SAFE_NO_PAD.decode(signature).map_err(|_| MediaUrlError::InvalidSignature)?;
		let mut verifier = HmacSha256::new_from_slice(&self.key).expect("HMAC accepts keys of any size");
		verifier.update(target.as_bytes());
		verifier.verify_slice(&supplied).map_err(|_| MediaUrlError::InvalidSignature)?;

		let target = Url::parse(target).map_err(|_| MediaUrlError::InvalidTarget)?;
		validate_media_target(&target)?;
		Ok(target)
	}

	pub fn rewrite_hls(&self, manifest: &str, source: &Url) -> Result<String, MediaUrlError> {
		let manifest = rewrite_captures(manifest, &HLS_URI, 1, |reference| self.rewrite_media_reference(reference, source))?;
		let mut rewritten = String::with_capacity(manifest.len());
		for line in manifest.split_inclusive('\n') {
			let (content, ending) = line
				.strip_suffix("\r\n")
				.map_or_else(|| line.strip_suffix('\n').map_or((line, ""), |content| (content, "\n")), |content| (content, "\r\n"));
			if content.is_empty() || content.starts_with('#') {
				rewritten.push_str(content);
			} else {
				rewritten.push_str(&self.rewrite_media_reference(content, source)?);
			}
			rewritten.push_str(ending);
		}
		Ok(rewritten)
	}

	pub fn rewrite_dash(&self, manifest: &str, source: &Url) -> Result<String, MediaUrlError> {
		rewrite_captures(manifest, &DASH_BASE_URL, 2, |reference| self.rewrite_media_reference(reference, source))
	}

	fn rewrite_absolute_url(&self, original: &str) -> Option<String> {
		let decoded = if original.starts_with("//") {
			format!("https:{original}")
		} else {
			original.to_string()
		}
		.replace("&amp;", "&");
		let mut target = Url::parse(&decoded).ok()?;
		if !matches!(target.scheme(), "http" | "https") || !target.username().is_empty() || target.password().is_some() {
			return None;
		}
		let host = target.host_str()?.to_ascii_lowercase();

		if is_reddit_navigation_host(&host) {
			return Some(local_path(&target));
		}
		if host == "redd.it" {
			let path = target.path().trim_start_matches('/');
			target.set_path(&format!("/comments/{path}"));
			return Some(local_path(&target));
		}
		if !is_reddit_media_host(&host) {
			return None;
		}

		let fragment = target.fragment().map(str::to_owned);
		target.set_fragment(None);
		target.set_scheme("https").ok()?;
		target.set_port(None).ok()?;
		let mut rewritten = self.media_url(&target).ok()?;
		if let Some(fragment) = fragment {
			rewritten.push('#');
			rewritten.push_str(&fragment);
		}
		Some(rewritten)
	}

	fn rewrite_media_reference(&self, reference: &str, source: &Url) -> Result<String, MediaUrlError> {
		if reference.is_empty() || reference.starts_with("data:") {
			return Ok(reference.to_string());
		}
		let reference = reference.replace("&amp;", "&");
		let target = source.join(&reference).map_err(|_| MediaUrlError::InvalidManifestUrl)?;
		self.media_url(&target)
	}

	fn signature(&self, target: &str) -> String {
		let mut signer = HmacSha256::new_from_slice(&self.key).expect("HMAC accepts keys of any size");
		signer.update(target.as_bytes());
		URL_SAFE_NO_PAD.encode(signer.finalize().into_bytes())
	}
}

pub fn rewrite_reddit_navigation(url: &str) -> Option<String> {
	let mut target = Url::parse(url).ok()?;
	let host = target.host_str()?.to_ascii_lowercase();
	if is_reddit_navigation_host(&host) {
		Some(local_path(&target))
	} else if host == "redd.it" {
		let path = target.path().trim_start_matches('/');
		target.set_path(&format!("/comments/{path}"));
		Some(local_path(&target))
	} else {
		None
	}
}

pub fn validate_media_target(target: &Url) -> Result<(), MediaUrlError> {
	let host = target.host_str().ok_or(MediaUrlError::InvalidTarget)?.to_ascii_lowercase();
	if target.scheme() != "https" || !target.username().is_empty() || target.password().is_some() || target.port().is_some() || !is_reddit_media_host(&host) {
		return Err(MediaUrlError::ForbiddenTarget);
	}
	Ok(())
}

fn is_reddit_navigation_host(host: &str) -> bool {
	host == "reddit.com" || host.ends_with(".reddit.com")
}

fn is_reddit_media_host(host: &str) -> bool {
	(host != "redd.it" && host.ends_with(".redd.it"))
		|| host == "redditmedia.com"
		|| host.ends_with(".redditmedia.com")
		|| host == "redditstatic.com"
		|| host.ends_with(".redditstatic.com")
		|| matches!(
			host,
			"reddit-econ-prod-assets-permanent.s3.amazonaws.com"
				| "reddit-image-uploads.s3.amazonaws.com"
				| "reddit-uploaded-media.s3-accelerate.amazonaws.com"
				| "reddit-uploaded-video.s3-accelerate.amazonaws.com"
		)
}

fn local_path(target: &Url) -> String {
	let mut path = target
		.path()
		.strip_prefix("/gallery/")
		.map_or_else(|| target.path().to_string(), |id| format!("/comments/{id}"));
	if let Some(query) = target.query() {
		path.push('?');
		path.push_str(query);
	}
	if let Some(fragment) = target.fragment() {
		path.push('#');
		path.push_str(fragment);
	}
	path
}

fn trim_url_punctuation(url: &str) -> (&str, &str) {
	let mut split = url.len();
	while let Some(character) = url[..split].chars().next_back() {
		let unmatched_closer = match character {
			')' => url[..split].matches(')').count() > url[..split].matches('(').count(),
			']' => url[..split].matches(']').count() > url[..split].matches('[').count(),
			'}' => url[..split].matches('}').count() > url[..split].matches('{').count(),
			_ => false,
		};
		if unmatched_closer || matches!(character, '.' | ',' | ';' | '!') {
			split -= character.len_utf8();
		} else {
			break;
		}
	}
	url.split_at(split)
}

fn rewrite_captures<F>(input: &str, pattern: &Regex, capture: usize, mut rewrite: F) -> Result<String, MediaUrlError>
where
	F: FnMut(&str) -> Result<String, MediaUrlError>,
{
	let mut output = String::with_capacity(input.len());
	let mut end = 0;
	for captures in pattern.captures_iter(input) {
		let matched = captures.get(capture).expect("the media pattern contains the requested capture");
		output.push_str(&input[end..matched.start()]);
		output.push_str(&rewrite(matched.as_str())?);
		end = matched.end();
	}
	output.push_str(&input[end..]);
	Ok(output)
}

#[derive(Debug, Error)]
pub enum MediaUrlError {
	#[error("media signing secrets must contain at least 32 bytes")]
	ShortSecret,
	#[error("invalid media URL encoding")]
	InvalidEncoding,
	#[error("invalid media URL signature")]
	InvalidSignature,
	#[error("invalid media target URL")]
	InvalidTarget,
	#[error("media target is not an allowed Reddit host")]
	ForbiddenTarget,
	#[error("invalid URL in Reddit media manifest")]
	InvalidManifestUrl,
}

