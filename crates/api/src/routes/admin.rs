//! Admin endpoints. All routes require the `users.manage` (or
//! `users.assign_roles`) permission, enforced inline.

use axum::{
    extract::{Path, Query, State},
    routing::{get, post, patch},
    Json, Router,
};
use generic_auth_auth::permissions::{load_permissions, require, Permission};
use generic_auth_core::models::{Role, User};
use generic_auth_db::users as user_db;
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::middleware::auth::VerifiedUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/users",                 get(list_users))
        .route("/users/{id}/role",        patch(set_role))
        .route("/users/{id}/active",      patch(set_active))
        .route("/users/{id}/permissions", post(grant_permission))
}

#[derive(Debug, Deserialize, ToSchema, IntoParams)]
pub struct Page {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}
fn default_limit() -> i64 { 50 }

#[utoipa::path(
    get,
    path = "/api/v1/admin/users",
    params(Page),
    responses(
        (status = 200, body = Vec<User>),
        (status = 401),
        (status = 403),
    ),
    security(("bearer_auth" = [])),
    tag = "admin",
)]
pub(crate) async fn list_users(
    State(state): State<AppState>,
    user: VerifiedUser,
    Query(page): Query<Page>,
) -> ApiResult<Json<Vec<User>>> {
    let perms = load_permissions(&state.db, user.user_id).await.map_err(ApiError)?;
    require(&perms, Permission::UsersRead).map_err(ApiError)?;

    let users = user_db::list_users(&state.db, page.limit.min(200), page.offset)
        .await.map_err(|e| ApiError(e.into()))?;
    Ok(Json(users))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetRoleReq { pub role: Role }

#[utoipa::path(
    patch,
    path = "/api/v1/admin/users/{id}/role",
    params(("id" = Uuid, Path,)),
    request_body = SetRoleReq,
    responses(
        (status = 204, description = "Role updated"),
        (status = 401),
        (status = 403),
    ),
    security(("bearer_auth" = [])),
    tag = "admin",
)]
pub(crate) async fn set_role(
    State(state): State<AppState>,
    user: VerifiedUser,
    Path(id): Path<Uuid>,
    Json(body): Json<SetRoleReq>,
) -> ApiResult<axum::http::StatusCode> {
    let perms = load_permissions(&state.db, user.user_id).await.map_err(ApiError)?;
    require(&perms, Permission::UsersAssignRoles).map_err(ApiError)?;

    user_db::set_role(&state.db, id, body.role)
        .await.map_err(|e| ApiError(e.into()))?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetActiveReq { pub active: bool }

#[utoipa::path(
    patch,
    path = "/api/v1/admin/users/{id}/active",
    params(("id" = Uuid, Path,)),
    request_body = SetActiveReq,
    responses(
        (status = 204, description = "Active flag updated"),
        (status = 401),
        (status = 403),
    ),
    security(("bearer_auth" = [])),
    tag = "admin",
)]
pub(crate) async fn set_active(
    State(state): State<AppState>,
    user: VerifiedUser,
    Path(id): Path<Uuid>,
    Json(body): Json<SetActiveReq>,
) -> ApiResult<axum::http::StatusCode> {
    let perms = load_permissions(&state.db, user.user_id).await.map_err(ApiError)?;
    require(&perms, Permission::UsersManage).map_err(ApiError)?;

    user_db::set_active(&state.db, id, body.active)
        .await.map_err(|e| ApiError(e.into()))?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct GrantPermReq { pub permission: String, pub granted: bool }

#[utoipa::path(
    post,
    path = "/api/v1/admin/users/{id}/permissions",
    params(("id" = Uuid, Path,)),
    request_body = GrantPermReq,
    responses(
        (status = 204, description = "Permission updated"),
        (status = 401),
        (status = 403),
    ),
    security(("bearer_auth" = [])),
    tag = "admin",
)]
pub(crate) async fn grant_permission(
    State(state): State<AppState>,
    user: VerifiedUser,
    Path(id): Path<Uuid>,
    Json(body): Json<GrantPermReq>,
) -> ApiResult<axum::http::StatusCode> {
    let perms = load_permissions(&state.db, user.user_id).await.map_err(ApiError)?;
    require(&perms, Permission::UsersAssignRoles).map_err(ApiError)?;

    sqlx::query!(
        r#"
        INSERT INTO user_permissions (user_id, permission_id, granted)
        SELECT $1, p.id, $3 FROM permissions p WHERE p.name = $2
        ON CONFLICT (user_id, permission_id)
        DO UPDATE SET granted = EXCLUDED.granted
        "#,
        id, body.permission, body.granted,
    ).execute(&state.db).await
        .map_err(|e| ApiError(generic_auth_core::AppError::Database(e.to_string())))?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}