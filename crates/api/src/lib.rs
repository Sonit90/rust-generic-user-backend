use std::sync::Arc;

use anyhow::Context as _;
use axum::Router;
use hyper::http::Method;
use tokio::net::TcpListener;
use tokio::signal;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;
use utoipa::OpenApi as _;
use utoipa_swagger_ui::SwaggerUi;

pub mod commands;
pub mod config;
pub mod email;
pub mod error;
pub mod middleware;
pub mod openapi;
pub mod routes;
pub mod state;

pub use state::AppState;

pub async fn run(settings: Arc<config::Settings>) -> anyhow::Result<()> {
    let state = AppState::build(settings.clone()).await
        .context("building app state")?;

    let cors = build_cors_layer(&settings);

    let app: Router = routes::router(state.clone())
        .merge(SwaggerUi::new("/api/docs")
            .url("/api/docs/openapi.json", openapi::ApiDoc::openapi()))
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    let listener = TcpListener::bind(&settings.http.bind).await
        .with_context(|| format!("bind {}", settings.http.bind))?;
    tracing::info!(addr = %settings.http.bind, "listening");

    // Spawn the worker pool.
    let (tx, rx) = tokio::sync::watch::channel(false);
    let worker = price_merger_jobs::Worker::new(
        state.db.clone(),
        state.storage.clone(),
        price_merger_jobs::WorkerConfig {
            workers: settings.jobs.workers,
            poll_interval_secs: settings.jobs.poll_interval_secs,
            visibility_timeout_secs: settings.jobs.visibility_timeout_secs as i64,
            batch_size: 4,
            purge_sweep_interval_secs: 600,
        },
    );
    let worker_handle = tokio::spawn(worker.run(rx));

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("axum::serve")?;

    let _ = tx.send(true); // signal workers to drain
    let _ = worker_handle.await;
    Ok(())
}

fn build_cors_layer(settings: &config::Settings) -> CorsLayer {
    if settings.app_env == "development" {
        return CorsLayer::permissive();
    }

    let origins: Vec<_> = settings.cors.allowed_origins.iter()
        .filter_map(|o| o.parse().ok())
        .collect();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            Method::GET, Method::POST, Method::PUT,
            Method::PATCH, Method::DELETE, Method::OPTIONS,
        ])
        .allow_headers(tower_http::cors::Any)
        .allow_credentials(false)
}

async fn shutdown_signal() {
    let ctrl_c = async { let _ = signal::ctrl_c().await; };
    #[cfg(unix)]
    let term = async {
        let mut t = signal::unix::signal(signal::unix::SignalKind::terminate()).unwrap();
        t.recv().await;
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();

    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
    tracing::info!("shutting down");
}
