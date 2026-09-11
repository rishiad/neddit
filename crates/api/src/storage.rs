//! Local SQLite stores. Each bounded queue owns one database thread.
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
	path::PathBuf,
	sync::Arc,
	time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{mpsc, oneshot, Mutex, OwnedMutexGuard};

const ALPHABET: &[char] = &[
	'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h',
	'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9',
];
const MAX_ENTRY_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("storage is unavailable")]
	Unavailable,
	#[error("storage capacity reached")]
	Capacity,
	#[error("shortlink creation rate exceeded")]
	RateLimited,
	#[error("unsupported database schema version")]
	Schema,
	#[error(transparent)]
	Sql(#[from] rusqlite::Error),
	#[error(transparent)]
	Io(#[from] std::io::Error),
}

type Job = Box<dyn FnOnce(&mut Connection) + Send>;

#[derive(Clone)]
struct Database(mpsc::Sender<Job>);

impl Database {
	async fn open(path: PathBuf, durable: bool) -> Result<Self, Error> {
		if path.as_os_str().is_empty() || path.to_string_lossy() == ":memory:" || path.to_string_lossy().starts_with("file:") {
			return Err(Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, "storage requires a local file path")));
		}
		let (sender, mut receiver) = mpsc::channel::<Job>(64);
		let (ready, opened) = oneshot::channel();
		std::thread::Builder::new().name("neddit-sqlite".into()).spawn(move || {
			let connection = (|| {
				if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
					std::fs::create_dir_all(parent)?;
				}
				private_file(&path, false)?;
				let mut db = Connection::open(path)?;
				db.busy_timeout(Duration::from_millis(250))?;
				db.execute_batch(if durable {
					"PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;"
				} else {
					"PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;"
				})?;
				let version: u32 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
				let kind: u32 = db.query_row("PRAGMA application_id", [], |r| r.get(0))?;
				let expected = if durable { 0x4E444646 } else { 0x4E444343 };
				if version > 1 {
					return Err(Error::Schema);
				}
				if version != 0 && kind != expected {
					return Err(Error::Schema);
				}
				if version == 0 {
					let tx = db.transaction()?;
					tx.execute_batch(if durable {
						"CREATE TABLE definitions (
                            id TEXT PRIMARY KEY, fingerprint TEXT NOT NULL UNIQUE,
                            definition TEXT NOT NULL, created INTEGER NOT NULL
                        ); CREATE INDEX definitions_created ON definitions(created);"
					} else {
						"CREATE TABLE entries (
                            key TEXT PRIMARY KEY, payload BLOB NOT NULL,
                            created INTEGER NOT NULL, expires INTEGER NOT NULL, bytes INTEGER NOT NULL
                        ); CREATE INDEX entries_expiry ON entries(expires);
                        CREATE INDEX entries_created ON entries(created, key);
                        CREATE TABLE cache_size (bytes INTEGER NOT NULL);
                        INSERT INTO cache_size VALUES (0);
                        CREATE TRIGGER cache_insert AFTER INSERT ON entries BEGIN
                            UPDATE cache_size SET bytes=bytes+new.bytes;
                        END;
                        CREATE TRIGGER cache_delete AFTER DELETE ON entries BEGIN
                            UPDATE cache_size SET bytes=bytes-old.bytes;
                        END;"
					})?;
					tx.execute_batch("PRAGMA user_version=1;")?;
					tx.pragma_update(None, "application_id", expected)?;
					tx.commit()?;
				}
				Ok::<_, Error>(db)
			})();
			match connection {
				Ok(mut db) => {
					if ready.send(Ok(())).is_err() {
						return;
					}
					while let Some(job) = receiver.blocking_recv() {
						job(&mut db);
					}
				}
				Err(error) => {
					let _ = ready.send(Err(error));
				}
			}
		})?;
		opened.await.map_err(|_| Error::Unavailable)??;
		Ok(Self(sender))
	}

	async fn call<T: Send + 'static>(&self, job: impl FnOnce(&mut Connection) -> Result<T, Error> + Send + 'static) -> Result<T, Error> {
		let (send, receive) = oneshot::channel();
		self
			.0
			.try_send(Box::new(move |db| {
				let _ = send.send(job(db));
			}))
			.map_err(|_| Error::Unavailable)?;
		receive.await.map_err(|_| Error::Unavailable)?
	}
}

fn private_file(path: &std::path::Path, exclusive: bool) -> std::io::Result<()> {
	let mut options = std::fs::OpenOptions::new();
	options.read(true).write(true).create(true).create_new(exclusive);
	#[cfg(unix)]
	{
		use std::os::unix::fs::OpenOptionsExt;
		options.mode(0o600);
	}
	options.open(path)?;
	Ok(())
}

pub(crate) fn now() -> i64 {
	SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64
}

pub(crate) fn fingerprint(value: &[u8]) -> String {
	format!("{:x}", Sha256::digest(value))
}

pub(crate) fn id(length: usize) -> String {
	nanoid::nanoid!(length, ALPHABET)
}

/// Fixed stripes bound lock memory even for adversarial unique requests.
#[derive(Clone)]
pub(crate) struct Flights(Arc<[Arc<Mutex<()>>; 64]>);

impl Default for Flights {
	fn default() -> Self {
		Self(Arc::new(std::array::from_fn(|_| Arc::new(Mutex::new(())))))
	}
}

impl Flights {
	pub(crate) async fn lock(&self, key: &str) -> OwnedMutexGuard<()> {
		let digest = Sha256::digest(key.as_bytes());
		self.0[usize::from(digest[0]) % 64].clone().lock_owned().await
	}
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct CacheConfig {
	pub path: PathBuf,
	pub max_bytes: u64,
}

impl Default for CacheConfig {
	fn default() -> Self {
		Self {
			path: "data/cache.sqlite".into(),
			max_bytes: 256 * 1024 * 1024,
		}
	}
}

#[derive(Clone)]
pub struct Cache {
	db: Database,
	max_bytes: u64,
	flights: Flights,
}

impl Cache {
	pub async fn open(config: &CacheConfig) -> Result<Self, Error> {
		let db = Database::open(config.path.clone(), false).await?;
		let disk_limit = config.max_bytes.saturating_mul(2).saturating_add(4 * 1024 * 1024);
		db.call(move |db| {
			let page_size: u64 = db.query_row("PRAGMA page_size", [], |r| r.get::<_, u32>(0))?.into();
			db.pragma_update(None, "max_page_count", (disk_limit / page_size).min(u64::from(u32::MAX)) as u32)?;
			db.execute_batch("PRAGMA journal_size_limit=4194304;")?;
			Ok(())
		})
		.await?;
		let cache = Self {
			db,
			max_bytes: config.max_bytes,
			flights: Default::default(),
		};
		let weak = cache.db.0.downgrade();
		tokio::spawn(async move {
			loop {
				tokio::time::sleep(Duration::from_secs(60)).await;
				let Some(sender) = weak.upgrade() else {
					break;
				};
				let result = Database(sender)
					.call(|db| {
						db.execute("DELETE FROM entries WHERE key IN (SELECT key FROM entries WHERE expires <= ?1 LIMIT 256)", [now()])?;
						db.execute_batch("PRAGMA wal_checkpoint(PASSIVE);")?;
						Ok(())
					})
					.await;
				if let Err(error) = result {
					log::warn!("cache maintenance: {error}");
				}
			}
		});
		Ok(cache)
	}

	pub(crate) async fn json<E>(&self, key: String, ttl: i64, fetch: impl std::future::Future<Output = Result<serde_json::Value, E>>) -> Result<serde_json::Value, E> {
		let _guard = self.flights.lock(&key).await;
		if let Some(bytes) = self.get(&key).await {
			if let Ok(value) = serde_json::from_slice(&bytes) {
				return Ok(value);
			}
		}
		let value = fetch.await?;
		if let Ok(bytes) = serde_json::to_vec(&value) {
			if let Err(error) = self.put(key, bytes, ttl).await {
				log::warn!("cache write: {error}");
			}
		}
		Ok(value)
	}

	pub(crate) async fn get(&self, key: &str) -> Option<Vec<u8>> {
		let key = key.to_owned();
		match self
			.db
			.call(move |db| {
				Ok(
					db.query_row("SELECT payload FROM entries WHERE key=?1 AND expires>?2", params![key, now()], |r| r.get(0))
						.optional()?,
				)
			})
			.await
		{
			Ok(value) => {
				log::debug!("cache {}", if value.is_some() { "hit" } else { "miss" });
				value
			}
			Err(error) => {
				log::warn!("cache read: {error}");
				None
			}
		}
	}

	pub(crate) async fn put(&self, key: String, payload: Vec<u8>, ttl: i64) -> Result<(), Error> {
		let max = self.max_bytes;
		let charge = (payload.len() + key.len() + 128) as u64;
		if payload.len() > MAX_ENTRY_BYTES || charge > max {
			return Err(Error::Capacity);
		}
		self
			.db
			.call(move |db| {
				let tx = db.transaction()?;
				tx.execute(
					"DELETE FROM entries WHERE key=?1 OR key IN (SELECT key FROM entries WHERE expires<=?2 LIMIT 256)",
					params![key, now()],
				)?;
				let mut size: i64 = tx.query_row("SELECT bytes FROM cache_size", [], |r| r.get(0))?;
				while size as u64 + charge > max {
					tx.execute("DELETE FROM entries WHERE key IN (SELECT key FROM entries ORDER BY created, key LIMIT 64)", [])?;
					size = tx.query_row("SELECT bytes FROM cache_size", [], |r| r.get(0))?;
				}
				tx.execute("INSERT INTO entries VALUES (?1,?2,?3,?4,?5)", params![key, payload, now(), now() + ttl, charge as i64])?;
				tx.commit()?;
				Ok(())
			})
			.await
	}
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ShortlinkConfig {
	pub enabled: bool,
	pub path: PathBuf,
	pub base_url: Option<String>,
	pub id_length: usize,
	pub max_definitions: u64,
	pub creations_per_minute: u64,
}

impl Default for ShortlinkConfig {
	fn default() -> Self {
		Self {
			enabled: true,
			path: "data/feeds.sqlite".into(),
			base_url: None,
			id_length: 10,
			max_definitions: 100_000,
			creations_per_minute: 60,
		}
	}
}

impl ShortlinkConfig {
	pub fn validate(&self) -> Result<(), &'static str> {
		if !(10..=32).contains(&self.id_length) {
			return Err("shortlinks.id_length must be between 10 and 32");
		}
		if self.max_definitions == 0 || self.creations_per_minute == 0 {
			return Err("shortlink capacity and creation rate must be positive");
		}
		if let Some(base) = &self.base_url {
			let url = url::Url::parse(base).map_err(|_| "shortlinks.base_url must be an HTTP(S) origin")?;
			if !matches!(url.scheme(), "http" | "https")
				|| url.host_str().is_none()
				|| !url.username().is_empty()
				|| url.password().is_some()
				|| url.path() != "/"
				|| url.query().is_some()
				|| url.fragment().is_some()
			{
				return Err("shortlinks.base_url must be an HTTP(S) origin without credentials, path, query, or fragment");
			}
		}
		Ok(())
	}
}

#[derive(Clone)]
pub struct Shortlinks {
	db: Database,
	config: ShortlinkConfig,
}

impl Shortlinks {
	pub async fn backup(&self, destination: PathBuf) -> Result<(), Error> {
		self
			.db
			.call(move |source| {
				// Refuse overwrite, including the live database or one of its aliases.
				private_file(&destination, true)?;
				let mut destination = Connection::open(destination)?;
				let backup = rusqlite::backup::Backup::new(source, &mut destination)?;
				backup.run_to_completion(100, Duration::from_millis(10), None)?;
				Ok(())
			})
			.await
	}

	pub async fn open(config: &ShortlinkConfig) -> Result<Self, Error> {
		config.validate().map_err(|_| Error::Unavailable)?;
		let mut config = config.clone();
		config.base_url = config.base_url.map(|base| url::Url::parse(&base).expect("validated origin").to_string());
		Ok(Self {
			db: Database::open(config.path.clone(), true).await?,
			config,
		})
	}

	pub fn url(&self, id: &str) -> String {
		format!("{}/f/{id}", self.config.base_url.as_deref().unwrap_or_default().trim_end_matches('/'))
	}

	pub(crate) async fn save(&self, definition: String) -> Result<String, Error> {
		let length = self.config.id_length;
		self.save_with(definition, move || id(length)).await
	}

	async fn save_with(&self, definition: String, mut generate: impl FnMut() -> String + Send + 'static) -> Result<String, Error> {
		let fingerprint = fingerprint(definition.as_bytes());
		let config = self.config.clone();
		self
			.db
			.call(move |db| {
				let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
				if let Some(id) = tx.query_row("SELECT id FROM definitions WHERE fingerprint=?1", [&fingerprint], |r| r.get(0)).optional()? {
					return Ok(id);
				}
				let count: i64 = tx.query_row("SELECT count(*) FROM definitions", [], |r| r.get(0))?;
				if count as u64 >= config.max_definitions {
					return Err(Error::Capacity);
				}
				let recent: i64 = tx.query_row("SELECT count(*) FROM definitions WHERE created>?1", [now() - 60], |r| r.get(0))?;
				if recent as u64 >= config.creations_per_minute {
					return Err(Error::RateLimited);
				}
				for _ in 0..8 {
					let id = generate();
					let inserted = tx.execute(
						"INSERT INTO definitions VALUES (?1,?2,?3,?4) ON CONFLICT(id) DO NOTHING",
						params![id, fingerprint, definition, now()],
					)?;
					if inserted == 1 {
						tx.commit()?;
						return Ok(id);
					}
				}
				Err(Error::Unavailable)
			})
			.await
	}

	pub(crate) async fn get(&self, id: &str) -> Result<Option<String>, Error> {
		if !(10..=32).contains(&id.len()) || !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
			return Ok(None);
		}
		let id = id.to_owned();
		self
			.db
			.call(move |db| Ok(db.query_row("SELECT definition FROM definitions WHERE id=?1", [id], |r| r.get(0)).optional()?))
			.await
	}
}
