//! `calib-targets-studio` — local web GUI server for exploring calibration
//! datasets and detector configs.
//!
//! Wraps the `calib-targets-bench` library (dataset manifest, baselines,
//! runner, diagnose) behind a JSON API and serves the React SPA from
//! `studio/dist`. Launch with `cargo studio` and open the printed URL.

mod error;
mod routes;
mod snaps;
mod state;

use std::net::SocketAddr;
use std::sync::Arc;

use clap::Parser;

use crate::state::StudioState;

/// Command-line options for the studio server.
#[derive(Parser)]
#[command(
    name = "calib-targets-studio",
    about = "local web GUI for exploring calibration datasets and detector configs"
)]
struct Cli {
    /// TCP port to listen on.
    #[arg(long, default_value_t = 8930)]
    port: u16,
    /// Bind address. Keep the localhost default unless you trust your LAN.
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    /// API-only mode for frontend development: skip serving `studio/dist`
    /// and print the expected Vite dev-server URL instead.
    #[arg(long)]
    dev: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=debug".into()),
        )
        .init();

    let cli = Cli::parse();
    let state = Arc::new(StudioState::load()?);
    let app = routes::router(state, cli.dev);

    let addr: SocketAddr = format!("{}:{}", cli.host, cli.port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let url = format!("http://{}:{}", cli.host, cli.port);
    if cli.dev {
        tracing::info!(
            "API listening on {url} (dev mode — run `bun run dev` in studio/ for the UI)"
        );
    } else {
        tracing::info!("studio listening on {url}");
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutting down");
}
