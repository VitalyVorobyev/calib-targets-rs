//! Router assembly: `/api/*` JSON endpoints + static serving of the built
//! SPA (`studio/dist`).

pub mod configs;
pub mod dataset;
pub mod detect;
pub mod diagnose;
pub mod runs;

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

use crate::state::StudioState;

/// Shared state handle passed to every handler.
pub type AppState = Arc<StudioState>;

/// Build the full application router. `dev` skips static-file serving (the
/// Vite dev server owns the UI and proxies `/api` here).
pub fn router(state: AppState, dev: bool) -> Router {
    let api = Router::new()
        .route("/health", get(dataset::health))
        .route("/dataset", get(dataset::dataset))
        .route("/image/{*label}", get(dataset::image))
        .route("/baseline/{*label}", get(dataset::baseline))
        .route("/detect", post(detect::detect))
        .route("/diagnose", post(diagnose::diagnose))
        .route("/configs", get(configs::list))
        .route("/configs/_defaults", get(configs::defaults))
        .route(
            "/configs/{name}",
            get(configs::get).put(configs::put).delete(configs::delete),
        )
        .route("/runs", get(runs::list).post(runs::create))
        .route("/runs/{id}", get(runs::get))
        .with_state(state)
        .layer(CorsLayer::permissive());

    let app = Router::new().nest("/api", api);
    if dev {
        return app;
    }

    let dist = calib_targets_bench::workspace_root().join("studio/dist");
    if dist.join("index.html").exists() {
        let spa = ServeDir::new(&dist).not_found_service(ServeFile::new(dist.join("index.html")));
        app.fallback_service(spa)
    } else {
        app.fallback(missing_dist)
    }
}

async fn missing_dist() -> (axum::http::StatusCode, &'static str) {
    (
        axum::http::StatusCode::NOT_FOUND,
        "studio/dist not built — run `bun install && bun run build` in studio/, \
         or start the server with --dev and use the Vite dev server",
    )
}
