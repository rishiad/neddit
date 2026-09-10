#![forbid(unsafe_code)]

use std::{env, io, path::PathBuf};

use clap::Parser;
use neddit_api::{client::RedditClient, media::MediaSigner, server, server::MediaProxy, service::RedditService, video::VideoResolver};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "Neddit", version, about = "Private Reddit-compatible read service")]
struct Args {
	#[arg(short = '4', long, conflicts_with = "ipv6_only", help = "Listen on IPv4 only")]
	ipv4_only: bool,

	#[arg(short = '6', long, conflicts_with = "ipv4_only", help = "Listen on IPv6 only")]
	ipv6_only: bool,

	#[arg(short, long, default_value = "[::]", value_name = "ADDRESS", help = "Set the listen address")]
	address: String,

	#[arg(short, long, env = "PORT", default_value_t = 8080, value_name = "PORT", help = "Set the listen port")]
	port: u16,

	#[arg(long, env = "NEDDIT_MEDIA_KEY_FILE", value_name = "FILE", help = "Read the media URL signing secret from this file")]
	media_key_file: Option<PathBuf>,

	#[arg(
		long,
		env = "NEDDIT_YT_DLP_PATH",
		default_value = "yt-dlp",
		value_name = "FILE",
		help = "Set the yt-dlp executable used for hosted video resolution"
	)]
	yt_dlp_path: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	tracing_subscriber::fmt()
		.with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
		.init();

	let args = Args::parse();
	let ipv4_only = env::var_os("IPV4_ONLY").is_some() || args.ipv4_only;
	let ipv6_only = env::var_os("IPV6_ONLY").is_some() || args.ipv6_only;
	if ipv4_only && ipv6_only {
		return Err(io::Error::other("IPv4-only and IPv6-only modes cannot both be enabled").into());
	}
	let listener = if ipv4_only {
		format!("0.0.0.0:{}", args.port)
	} else if ipv6_only {
		format!("[::]:{}", args.port)
	} else {
		format!("{}:{}", args.address, args.port)
	};

	let signer = if let Some(path) = args.media_key_file {
		MediaSigner::from_file(path)?
	} else {
		warn!("no media key file configured; signed media URLs will expire when this process stops");
		MediaSigner::random()?
	};
	info!("creating Reddit client");
	let reddit = RedditClient::new().await?;
	let service = RedditService::new(reddit.clone());
	let video = VideoResolver::new(args.yt_dlp_path);
	let media = MediaProxy::new(reddit.clone(), signer, video);
	let app = neddit::router(service, media);

	info!(address = %listener, version = env!("CARGO_PKG_VERSION"), "neddit listening");
	let result = server::listen(app, &listener).await;
	reddit.shutdown().await;
	result?;
	Ok(())
}
