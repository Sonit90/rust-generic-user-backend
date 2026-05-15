use axum::{routing::get, Router};

use crate::state::AppState;

pub mod admin;
pub mod auth;
pub mod files;
pub mod formats;
pub mod mappings;
pub mod merge;
pub mod users;

pub fn router(state: AppState) -> Router {
    let api_v1 = Router::new()
        .nest("/auth",     auth::router())
        .nest("/users",    users::router())
        .nest("/files",    files::router())
        .nest("/mappings", mappings::router())
        .nest("/output-formats", formats::router())
        .nest("/merge",    merge::router())
        .nest("/admin",    admin::router())
        .with_state(state);

    Router::new()
        .route("/health", get(health))
        .nest("/api/v1", api_v1)
}

async fn health() -> &'static str { "ok" }
