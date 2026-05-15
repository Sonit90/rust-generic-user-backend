use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use price_merger_auth::permissions::{load_permissions, require, Permission};
use price_merger_core::{models::{ColumnMapping, MappedColumn}, AppError};
use price_merger_db::mappings as map_db;
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::middleware::auth::VerifiedUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(create).get(list))
        .route("/{id}", get(get_one).delete(delete))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateMappingReq {
    pub file_id: Uuid,
    pub sheet_name: Option<String>,
    pub header_row: Option<i32>,
    pub data_start_row: Option<i32>,
    pub columns: Vec<MappedColumn>,
}

#[utoipa::path(
    post,
    operation_id = "create_mapping",
    path = "/api/v1/mappings",
    request_body = CreateMappingReq,
    responses(
        (status = 201, body = ColumnMapping),
        (status = 401),
        (status = 403),
    ),
    security(("bearer_auth" = [])),
    tag = "mappings",
)]
pub(crate) async fn create(
    State(state): State<AppState>,
    user: VerifiedUser,
    Json(body): Json<CreateMappingReq>,
) -> ApiResult<(StatusCode, Json<ColumnMapping>)> {
    let perms = load_permissions(&state.db, user.user_id).await.map_err(ApiError)?;
    require(&perms, Permission::MappingsManageOwn).map_err(ApiError)?;

    let row = map_db::insert(&state.db, map_db::NewColumnMapping {
        owner_id: user.user_id,
        file_id: body.file_id,
        sheet_name: body.sheet_name.as_deref(),
        header_row: body.header_row.unwrap_or(1),
        data_start_row: body.data_start_row.unwrap_or(2),
        columns: &body.columns,
    }).await.map_err(|e| ApiError(e.into()))?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[utoipa::path(
    get,
    operation_id = "list_mappings",
    path = "/api/v1/mappings",
    responses(
        (status = 200, body = Vec<ColumnMapping>),
        (status = 401),
    ),
    security(("bearer_auth" = [])),
    tag = "mappings",
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    user: VerifiedUser,
) -> ApiResult<Json<Vec<ColumnMapping>>> {
    let v = map_db::list_by_owner(&state.db, user.user_id)
        .await.map_err(|e| ApiError(e.into()))?;
    Ok(Json(v))
}

#[utoipa::path(
    get,
    operation_id = "get_mapping",
    path = "/api/v1/mappings/{id}",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, body = ColumnMapping),
        (status = 401),
        (status = 403),
        (status = 404),
    ),
    security(("bearer_auth" = [])),
    tag = "mappings",
)]
pub(crate) async fn get_one(
    State(state): State<AppState>,
    user: VerifiedUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ColumnMapping>> {
    let m = map_db::get(&state.db, id).await.map_err(|e| ApiError(e.into()))?
        .ok_or_else(|| ApiError(AppError::NotFound))?;
    if m.owner_id != user.user_id {
        let perms = load_permissions(&state.db, user.user_id).await.map_err(ApiError)?;
        require(&perms, Permission::MappingsManageAny).map_err(ApiError)?;
    }
    Ok(Json(m))
}

#[utoipa::path(
    delete,
    operation_id = "delete_mapping",
    path = "/api/v1/mappings/{id}",
    params(("id" = Uuid, Path,)),
    responses(
        (status = 204),
        (status = 401),
        (status = 403),
        (status = 404),
    ),
    security(("bearer_auth" = [])),
    tag = "mappings",
)]
pub(crate) async fn delete(
    State(state): State<AppState>,
    user: VerifiedUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let m = map_db::get(&state.db, id).await.map_err(|e| ApiError(e.into()))?
        .ok_or_else(|| ApiError(AppError::NotFound))?;
    if m.owner_id != user.user_id {
        let perms = load_permissions(&state.db, user.user_id).await.map_err(ApiError)?;
        require(&perms, Permission::MappingsManageAny).map_err(ApiError)?;
    }
    map_db::delete(&state.db, id).await.map_err(|e| ApiError(e.into()))?;
    Ok(StatusCode::NO_CONTENT)
}