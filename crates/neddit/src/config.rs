use std::{
	fs,
	net::SocketAddr,
	path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use neddit_api::media::{DomainPolicy, MediaUrlError};
use neddit_api::storage::{CacheConfig, ShortlinkConfig};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
	pub server: ServerConfig,
	pub web: SurfaceConfig,
	pub api: SurfaceConfig,
	pub content: ContentConfig,
	pub domains: DomainConfig,
	pub media: MediaConfig,
	pub video: VideoConfig,
	pub logging: LoggingConfig,
	pub cache: CacheConfig,
	pub shortlinks: ShortlinkConfig,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
	pub listen: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceConfig {
	pub enabled: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContentConfig {
	pub allow_nsfw: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DomainConfig {
	pub navigation: Vec<String>,
	pub shortlinks: Vec<String>,
	pub media: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MediaConfig {
	#[serde(default)]
	pub signing_key_file: Option<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VideoConfig {
	pub enabled: bool,
	pub executable: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LoggingConfig {
	pub filter: String,
}

impl Config {
	/// Load the built-in defaults and merge an optional TOML file over them.
	///
	/// # Errors
	///
	/// Returns an error when the file cannot be read, parsed, or validated.
	pub fn load(path: Option<&Path>) -> Result<Self, ConfigError> {
		let mut document = defaults_document()?;
		if let Some(path) = path {
			let source = fs::read_to_string(path).map_err(|source| ConfigError::Read { path: path.to_owned(), source })?;
			let overrides = parse(&source, &path.display().to_string())?;
			merge(&mut document, overrides);
		}
		let config: Self = document.try_into().map_err(|source| ConfigError::Invalid { source })?;
		config.validate()?;
		Ok(config)
	}

	fn validate(&self) -> Result<(), ConfigError> {
		self.shortlinks.validate().map_err(ConfigError::Storage)?;
		if self.cache.max_bytes < 4 * 1024 * 1024 || self.cache.max_bytes > 1024 * 1024 * 1024 * 1024 {
			return Err(ConfigError::Storage("cache.max_bytes must be between 4 MiB and 1 TiB"));
		}
		if self.cache.path.as_os_str().is_empty() || self.shortlinks.path.as_os_str().is_empty() || self.cache.path == self.shortlinks.path {
			return Err(ConfigError::Storage("cache.path and shortlinks.path must be nonempty and distinct"));
		}
		for path in [&self.cache.path, &self.shortlinks.path] {
			if path.to_string_lossy() == ":memory:" || path.to_string_lossy().starts_with("file:") {
				return Err(ConfigError::Storage("storage paths must name local files, not SQLite memory databases or URIs"));
			}
		}
		if !self.web.enabled && !self.api.enabled {
			return Err(ConfigError::NoSurface);
		}
		self
			.server
			.listen
			.parse::<SocketAddr>()
			.map_err(|_| ConfigError::InvalidListenAddress(self.server.listen.clone()))?;
		if self.video.executable.as_os_str().is_empty() {
			return Err(ConfigError::EmptyVideoExecutable);
		}
		if !self.content.allow_nsfw && self.video.enabled {
			return Err(ConfigError::NsfwVideo);
		}
		self.domain_policy()?;
		tracing_subscriber::EnvFilter::try_new(&self.logging.filter).map_err(|source| ConfigError::LogFilter { source })?;
		Ok(())
	}

	/// Compile the configured host patterns into the shared rewrite policy.
	///
	/// # Errors
	///
	/// Returns an error when a domain contains a scheme, wildcard, port, or IP address.
	pub fn domain_policy(&self) -> Result<DomainPolicy, ConfigError> {
		DomainPolicy::new(&self.domains.navigation, &self.domains.shortlinks, &self.domains.media).map_err(ConfigError::Domains)
	}
}

impl Default for Config {
	fn default() -> Self {
		Self {
			server: ServerConfig { listen: "[::]:8080".into() },
			web: SurfaceConfig { enabled: true },
			api: SurfaceConfig { enabled: true },
			content: ContentConfig { allow_nsfw: true },
			domains: DomainConfig {
				navigation: Vec::new(),
				shortlinks: Vec::new(),
				media: Vec::new(),
			},
			media: MediaConfig { signing_key_file: None },
			video: VideoConfig {
				enabled: true,
				executable: "yt-dlp".into(),
			},
			logging: LoggingConfig { filter: "info".into() },
			cache: CacheConfig::default(),
			shortlinks: ShortlinkConfig::default(),
		}
	}
}

fn defaults_document() -> Result<toml::Value, ConfigError> {
	toml::Value::try_from(Config::default()).map_err(ConfigError::Defaults)
}

fn parse(source: &str, name: &str) -> Result<toml::Value, ConfigError> {
	toml::from_str(source).map_err(|source| ConfigError::Parse { name: name.to_owned(), source })
}

fn merge(base: &mut toml::Value, overrides: toml::Value) {
	match (base, overrides) {
		(toml::Value::Table(base), toml::Value::Table(overrides)) => {
			for (key, value) in overrides {
				if let Some(base) = base.get_mut(&key) {
					merge(base, value);
				} else {
					base.insert(key, value);
				}
			}
		}
		(base, value) => *base = value,
	}
}

#[derive(Debug, Error)]
pub enum ConfigError {
	#[error("{0}")]
	Storage(&'static str),
	#[error("failed to construct built-in configuration defaults")]
	Defaults(#[source] toml::ser::Error),
	#[error("failed to read configuration file `{}`", path.display())]
	Read {
		path: PathBuf,
		#[source]
		source: std::io::Error,
	},
	#[error("failed to parse {name}")]
	Parse {
		name: String,
		#[source]
		source: toml::de::Error,
	},
	#[error("invalid configuration")]
	Invalid {
		#[source]
		source: toml::de::Error,
	},
	#[error("web and API routes cannot both be disabled")]
	NoSurface,
	#[error("server.listen is not a socket address: `{0}`")]
	InvalidListenAddress(String),
	#[error("video.executable cannot be empty")]
	EmptyVideoExecutable,
	#[error("video.enabled must be false when content.allow_nsfw is false")]
	NsfwVideo,
	#[error("logging.filter is invalid")]
	LogFilter {
		#[source]
		source: tracing_subscriber::filter::ParseError,
	},
	#[error("domains contain an invalid entry")]
	Domains(#[source] MediaUrlError),
}

