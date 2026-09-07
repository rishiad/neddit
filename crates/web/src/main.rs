#![forbid(unsafe_code)]

use std::env;

#[cfg(not(unix))]
use std::future;

use neddit_api::{client::RedditClient, service::RedditService};
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	tracing_subscriber::fmt()
		.with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
		.init();

	let host = env::var("HOST").unwrap_or_else(|_| "127.0.0.1".into());
	let port = env::var("PORT").unwrap_or_else(|_| "8081".into());
	let address = format!("{host}:{port}");
	let listener = TcpListener::bind(&address).await?;
	let reddit = RedditClient::new().await?;
	let service = RedditService::new(reddit.clone());

	info!(%address, "neddit-web listening");
	let result = axum::serve(listener, neddit_web::router(service)).with_graceful_shutdown(shutdown_signal()).await;
	reddit.shutdown().await;
	result?;
	Ok(())
}

async fn shutdown_signal() {
	let interrupt = async {
		if let Err(error) = tokio::signal::ctrl_c().await {
			tracing::error!(%error, "failed to install interrupt handler");
		}
	};

	#[cfg(unix)]
	let terminate = async {
		match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
			Ok(mut signal) => {
				signal.recv().await;
			}
			Err(error) => tracing::error!(%error, "failed to install termination handler"),
		}
	};

	#[cfg(not(unix))]
	let terminate = future::pending::<()>();

	tokio::select! {
			() = interrupt => {}
			() = terminate => {}
	}
}
