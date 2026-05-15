use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use price_merger_auth::permissions::{load_permissions, require, Permission};
use price_merger_core::{
    models::{FileKind, OutputColumn, OutputFormat, OutputStep},
    AppError,
};
use price_merger_db::formats as fmt_db;
use serde::{Deserialize, Deserializer};
use utoipa::ToSchema;
use uuid::Uuid;

/// Deserialize tri-state field: missing key → `None`, `null` → `Some(None)`,
/// value → `Some(Some(value))`. Use with `#[serde(default, deserialize_with)]`.
fn double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

use crate::error::{ApiError, ApiResult};
use crate::middleware::auth::VerifiedUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(create).get(list))
        .route("/{id}", get(get_one).delete(delete).patch(update))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateFormatReq {
    pub description: Option<String>,
    /// Base filename (without extension) for the output file.
    pub filename: Option<String>,
    pub columns: Vec<OutputColumn>,
    /// Processing steps applied in order (filters and transforms).
    #[serde(default)]
    pub steps: Vec<OutputStep>,
    #[serde(default = "default_extension")]
    pub output_extension: FileKind,
}
fn default_extension() -> FileKind { FileKind::Xlsx }

#[utoipa::path(
    post,
    operation_id = "create_format",
    path = "/api/v1/output-formats",
    request_body = CreateFormatReq,
    responses(
        (status = 201, body = OutputFormat),
        (status = 400),
        (status = 401),
        (status = 403),
    ),
    security(("bearer_auth" = [])),
    tag = "formats",
)]
pub(crate) async fn create(
    State(state): State<AppState>,
    user: VerifiedUser,
    Json(body): Json<CreateFormatReq>,
) -> ApiResult<(StatusCode, Json<OutputFormat>)> {
    let perms = load_permissions(&state.db, user.user_id).await.map_err(ApiError)?;
    require(&perms, Permission::FormatsManageOwn).map_err(ApiError)?;

    for step in &body.steps {
        step.validate(&body.columns).map_err(|e| ApiError(e.into()))?;
    }

    let row = fmt_db::insert(&state.db, fmt_db::NewOutputFormat {
        owner_id:         user.user_id,
        description:      body.description.as_deref(),
        filename:         body.filename.as_deref(),
        columns:          &body.columns,
        steps:            &body.steps,
        output_extension: body.output_extension,
    }).await.map_err(|e| ApiError(e.into()))?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[utoipa::path(
    get,
    operation_id = "list_formats",
    path = "/api/v1/output-formats",
    responses(
        (status = 200, body = Vec<OutputFormat>),
        (status = 401),
    ),
    security(("bearer_auth" = [])),
    tag = "formats",
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    user: VerifiedUser,
) -> ApiResult<Json<Vec<OutputFormat>>> {
    let v = fmt_db::list_by_owner(&state.db, user.user_id)
        .await.map_err(|e| ApiError(e.into()))?;
    Ok(Json(v))
}

#[utoipa::path(
    get,
    operation_id = "get_format",
    path = "/api/v1/output-formats/{id}",
    params(("id" = Uuid, Path,)),
    responses(
        (status = 200, body = OutputFormat),
        (status = 401),
        (status = 403),
        (status = 404),
    ),
    security(("bearer_auth" = [])),
    tag = "formats",
)]
pub(crate) async fn get_one(
    State(state): State<AppState>,
    user: VerifiedUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<OutputFormat>> {
    let f = fmt_db::get(&state.db, id).await.map_err(|e| ApiError(e.into()))?
        .ok_or_else(|| ApiError(AppError::NotFound))?;
    if f.owner_id != user.user_id {
        let perms = load_permissions(&state.db, user.user_id).await.map_err(ApiError)?;
        require(&perms, Permission::FormatsManageAny).map_err(ApiError)?;
    }
    Ok(Json(f))
}

#[utoipa::path(
    delete,
    operation_id = "delete_format",
    path = "/api/v1/output-formats/{id}",
    params(("id" = Uuid, Path,)),
    responses(
        (status = 204),
        (status = 401),
        (status = 403),
        (status = 404),
    ),
    security(("bearer_auth" = [])),
    tag = "formats",
)]
pub(crate) async fn delete(
    State(state): State<AppState>,
    user: VerifiedUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let f = fmt_db::get(&state.db, id).await.map_err(|e| ApiError(e.into()))?
        .ok_or_else(|| ApiError(AppError::NotFound))?;
    if f.owner_id != user.user_id {
        let perms = load_permissions(&state.db, user.user_id).await.map_err(ApiError)?;
        require(&perms, Permission::FormatsManageAny).map_err(ApiError)?;
    }
    fmt_db::delete(&state.db, id).await.map_err(|e| ApiError(e.into()))?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateFormatReq {
    /// Omit to skip; `null` clears; value sets.
    #[serde(default, deserialize_with = "double_option")]
    pub description: Option<Option<String>>,
    /// Omit to skip; `null` clears; value sets.
    #[serde(default, deserialize_with = "double_option")]
    pub filename: Option<Option<String>>,
    #[serde(default)]
    pub columns: Option<Vec<OutputColumn>>,
    #[serde(default)]
    pub steps: Option<Vec<OutputStep>>,
    #[serde(default)]
    pub output_extension: Option<FileKind>,
}

impl UpdateFormatReq {
    fn is_empty(&self) -> bool {
        self.description.is_none()
            && self.filename.is_none()
            && self.columns.is_none()
            && self.steps.is_none()
            && self.output_extension.is_none()
    }
}

#[utoipa::path(
    patch,
    operation_id = "update_format",
    path = "/api/v1/output-formats/{id}",
    params(("id" = Uuid, Path,)),
    request_body = UpdateFormatReq,
    responses(
        (status = 200, body = OutputFormat),
        (status = 400),
        (status = 401),
        (status = 403),
        (status = 404),
    ),
    security(("bearer_auth" = [])),
    tag = "formats",
)]
pub(crate) async fn update(
    State(state): State<AppState>,
    user: VerifiedUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateFormatReq>,
) -> ApiResult<Json<OutputFormat>> {
    if body.is_empty() {
        return Err(ApiError(AppError::Validation("no fields to update".into())));
    }

    let existing = fmt_db::get(&state.db, id).await.map_err(|e| ApiError(e.into()))?
        .ok_or_else(|| ApiError(AppError::NotFound))?;

    if existing.owner_id != user.user_id {
        let perms = load_permissions(&state.db, user.user_id).await.map_err(ApiError)?;
        require(&perms, Permission::FormatsManageAny).map_err(ApiError)?;
    } else {
        let perms = load_permissions(&state.db, user.user_id).await.map_err(ApiError)?;
        require(&perms, Permission::FormatsManageOwn).map_err(ApiError)?;
    }

    if let Some(steps) = body.steps.as_deref() {
        let cols = body.columns.as_deref().unwrap_or(&existing.columns);
        for step in steps {
            step.validate(cols).map_err(|e| ApiError(e.into()))?;
        }
    } else if let Some(cols) = body.columns.as_deref() {
        for step in &existing.steps {
            step.validate(cols).map_err(|e| ApiError(e.into()))?;
        }
    }

    let row = fmt_db::update(&state.db, id, fmt_db::UpdateOutputFormat {
        set_description:  body.description.is_some(),
        description:      body.description.as_ref().and_then(|o| o.as_deref()),
        set_filename:     body.filename.is_some(),
        filename:         body.filename.as_ref().and_then(|o| o.as_deref()),
        columns:          body.columns.as_deref(),
        steps:            body.steps.as_deref(),
        output_extension: body.output_extension,
    }).await.map_err(|e| ApiError(e.into()))?
        .ok_or_else(|| ApiError(AppError::NotFound))?;

    Ok(Json(row))
}
