#![forbid(unsafe_code)]

use clap::{Arg, ArgAction, Command};

use log::{info, warn};
use neddit::api;
use neddit::client::RedditClient;
use neddit::media::MediaSigner;
use neddit::server;
use neddit::service::RedditService;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	// Initialize logger
	pretty_env_logger::init();

	let matches = Command::new("Neddit")
		.version(env!("CARGO_PKG_VERSION"))
		.about("Reddit data service written in Rust")
		.arg(Arg::new("ipv4-only").short('4').long("ipv4-only").help("Listen on IPv4 only").num_args(0))
		.arg(Arg::new("ipv6-only").short('6').long("ipv6-only").help("Listen on IPv6 only").num_args(0))
		.arg(
			Arg::new("address")
				.short('a')
				.long("address")
				.value_name("ADDRESS")
				.help("Sets address to listen on")
				.default_value("[::]")
				.num_args(1),
		)
		.arg(
			Arg::new("port")
				.short('p')
				.long("port")
				.value_name("PORT")
				.env("PORT")
				.help("Port to listen on")
				.default_value("8080")
				.action(ArgAction::Set)
				.num_args(1),
		)
		.arg(
			Arg::new("media-key-file")
				.long("media-key-file")
				.value_name("FILE")
				.env("NEDDIT_MEDIA_KEY_FILE")
				.help("Read the media URL signing secret from this file")
				.num_args(1),
		)
		.get_matches();

	let address = matches.get_one::<String>("address").unwrap();
	let port = matches.get_one::<String>("port").unwrap();
	let ipv4_only = std::env::var("IPV4_ONLY").is_ok() || matches.get_flag("ipv4-only");
	let ipv6_only = std::env::var("IPV6_ONLY").is_ok() || matches.get_flag("ipv6-only");

	let listener = if ipv4_only {
		format!("0.0.0.0:{port}")
	} else if ipv6_only {
		format!("[::]:{port}")
	} else {
		[address, ":", port].concat()
	};

	println!("Starting Neddit...");
	let signer = match matches.get_one::<String>("media-key-file") {
		Some(path) => MediaSigner::from_secret(&read_secret(path)?)?,
		None => {
			warn!("No media key file configured; signed media URLs will expire when this process stops");
			MediaSigner::random()?
		}
	};
	info!("Creating Reddit client");
	let reddit = RedditClient::new().await?;
	let service = RedditService::new(reddit.clone());
	let media = server::MediaProxy::new(reddit.clone(), signer.clone());

	let api = api::with_json_aliases(api::with_reddit_urls(api::router(service), signer));
	let app = server::with_middleware(server::router(media).fallback_service(api));

	println!("Running Neddit v{} on {listener}!", env!("CARGO_PKG_VERSION"));

	let result = server::listen(app, &listener).await;
	reddit.shutdown().await;
	result?;
	Ok(())
}

fn read_secret(path: impl AsRef<Path>) -> std::io::Result<Vec<u8>> {
	let secret = std::fs::read(path)?;
	let start = secret.iter().position(|byte| !byte.is_ascii_whitespace()).unwrap_or(secret.len());
	let end = secret.iter().rposition(|byte| !byte.is_ascii_whitespace()).map_or(start, |index| index + 1);
	Ok(secret[start..end].to_vec())
}
