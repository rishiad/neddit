#![forbid(unsafe_code)]

use std::path::PathBuf;

use clap::Parser;
use neddit::config::Config;
use neddit_api::{
	client::RedditClient,
	media::MediaSigner,
	server,
	server::MediaProxy,
	service::{ContentPolicy, RedditService},
	video::VideoResolver,
};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "Neddit", version, about = "Private Reddit-compatible read service")]
struct Args {
	#[arg(short, long, value_name = "FILE", help = "Read configuration from this TOML file")]
	config: Option<PathBuf>,

	#[arg(long, help = "Validate the configuration and exit")]
	check_config: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let args = Args::parse();
	let config = Config::load(args.config.as_deref())?;
	if args.check_config {
		println!("configuration is valid");
		return Ok(());
	}

	tracing_subscriber::fmt().with_env_filter(EnvFilter::try_new(&config.logging.filter)?).init();

	let domains = config.domain_policy()?;
	let signer = if let Some(path) = &config.media.signing_key_file {
		MediaSigner::from_file(path)?
	} else {
		warn!("no media key file configured; signed media URLs will expire when this process stops");
		MediaSigner::random()?
	}
	.with_domains(domains.clone())
	.with_content_policy(config.content.allow_nsfw);
	info!("creating Reddit client");
	let reddit = RedditClient::with_domains(domains).await?;
	let service = RedditService::with_content_policy(reddit.clone(), ContentPolicy::new(config.content.allow_nsfw));
	let video = config.video.enabled.then(|| VideoResolver::new(&config.video.executable));
	let media = MediaProxy::new(reddit.clone(), signer, video);
	let app = neddit::router(service, &media, config.web.enabled, config.api.enabled);

	info!(address = %config.server.listen, version = env!("CARGO_PKG_VERSION"), "neddit listening");
	let result = server::listen(app, &config.server.listen).await;
	reddit.shutdown().await;
	result?;
	Ok(())
}
