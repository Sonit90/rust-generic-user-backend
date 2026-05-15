//! Trigger and monitor merge runs.
//!
//! A merge run reads N input files, applies per-column and global transforms,
//! and writes a single output file in the requested format. The heavy work
//! happens in the jobs crate; this module only enqueues the job and serves
//! status / download.

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use price_merger_auth::permissions::{load_permissions, require, Permission};
use price_merger_core::{
    models::{JobPayload, MappedColumn, MergeStatus},
    AppError,
};
use price_merger_db::{files as files_db, mappings as map_db, merge_runs};
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::middleware::auth::VerifiedUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/runs", post(start_run).get(list_runs))
        .route("/runs/{id}", get(get_run))
        .route("/runs/{id}/download", get(download_run))
}

/// Per-file input specification supplied inline when starting a merge run.
#[derive(Debug, Deserialize, ToSchema)]
pub struct InputSpec {
    /// ID of an uploaded file to merge.
    pub file_id: Uuid,
    /// Sheet name for XLS/XLSX files. Defaults to the first sheet.
    pub sheet_name: Option<String>,
    /// 1-based row that contains column headers. Default: 1.
    #[serde(default = "default_one")]
    pub header_row: i32,
    /// 1-based first data row. Default: 2.
    #[serde(default = "default_two")]
    pub data_start_row: i32,
    /// Column mappings: source index → canonical name + per-column transforms.
    pub columns: Vec<MappedColumn>,
}

fn default_one() -> i32 { 1 }
fn default_two() -> i32 { 2 }

#[derive(Debug, Deserialize, ToSchema)]
pub struct StartRunReq {
    /// Output format that defines the column schema and global transforms.
    pub output_format_id: Uuid,
    /// One entry per input file. At least one required.
    pub inputs: Vec<InputSpec>,
}

// ---- POST /runs ------------------------------------------------------------

#[utoipa::path(
    post,
    operation_id = "start_merge_run",
    path = "/api/v1/merge/runs",
    request_body = StartRunReq,
    responses(
        (status = 202, description = "Run enqueued", body = price_merger_core::models::MergeRun),
        (status = 400),
        (status = 401),
        (status = 403),
    ),
    security(("bearer_auth" = [])),
    tag = "merge",
)]
pub(crate) async fn start_run(
    State(state): State<AppState>,
    user: VerifiedUser,
    Json(body): Json<StartRunReq>,
) -> ApiResult<(StatusCode, Json<price_merger_core::models::MergeRun>)> {
    let perms = load_permissions(&state.db, user.user_id).await.map_err(ApiError)?;
    require(&perms, Permission::JobsRun).map_err(ApiError)?;

    if body.inputs.is_empty() {
        return Err(ApiError(AppError::BadRequest("at least one input required".into())));
    }

    // Validate all files are accessible before creating anything.
    for spec in &body.inputs {
        let file = files_db::get_uploaded(&state.db, spec.file_id)
            .await.map_err(|e| ApiError(e.into()))?
            .ok_or_else(|| ApiError(AppError::NotFound))?;
        if file.purged_at.is_some() {
            return Err(ApiError(AppError::NotFound));
        }
        if file.owner_id != user.user_id {
            require(&perms, Permission::FilesReadAll).map_err(ApiError)?;
        }
    }

    // Create one column_mapping per input file.
    let mut mapping_ids = Vec::with_capacity(body.inputs.len());
    for spec in &body.inputs {
        let m = map_db::insert(&state.db, map_db::NewColumnMapping {
            owner_id: user.user_id,
            file_id: spec.file_id,
            sheet_name: spec.sheet_name.as_deref(),
            header_row: spec.header_row,
            data_start_row: spec.data_start_row,
            columns: &spec.columns,
        }).await.map_err(|e| ApiError(e.into()))?;
        mapping_ids.push(m.id);
    }

    let mr = merge_runs::create(
        &state.db, user.user_id, body.output_format_id, &mapping_ids,
    ).await.map_err(|e| ApiError(e.into()))?;

    price_merger_db::jobs::enqueue(
        &state.db,
        &JobPayload::MergeRun { merge_run_id: mr.id },
        None,
    ).await.map_err(|e| ApiError(e.into()))?;

    Ok((StatusCode::ACCEPTED, Json(mr)))
}

// ---- GET /runs -------------------------------------------------------------

#[utoipa::path(
    get,
    operation_id = "list_merge_runs",
    path = "/api/v1/merge/runs",
    responses(
        (status = 200, body = Vec<price_merger_core::models::MergeRun>),
        (status = 401),
    ),
    security(("bearer_auth" = [])),
    tag = "merge",
)]
pub(crate) async fn list_runs(
    State(state): State<AppState>,
    user: VerifiedUser,
) -> ApiResult<Json<Vec<price_merger_core::models::MergeRun>>> {
    let runs = merge_runs::list_by_owner(&state.db, user.user_id)
        .await.map_err(|e| ApiError(e.into()))?;
    Ok(Json(runs))
}

// ---- GET /runs/{id} --------------------------------------------------------

#[utoipa::path(
    get,
    operation_id = "get_merge_run",
    path = "/api/v1/merge/runs/{id}",
    params(("id" = Uuid, Path,)),
    responses(
        (status = 200, body = price_merger_core::models::MergeRun),
        (status = 401),
        (status = 403),
        (status = 404),
    ),
    security(("bearer_auth" = [])),
    tag = "merge",
)]
pub(crate) async fn get_run(
    State(state): State<AppState>,
    user: VerifiedUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<price_merger_core::models::MergeRun>> {
    let mr = merge_runs::get(&state.db, id).await.map_err(|e| ApiError(e.into()))?
        .ok_or_else(|| ApiError(AppError::NotFound))?;
    if mr.owner_id != user.user_id {
        return Err(ApiError(AppError::Forbidden));
    }
    Ok(Json(mr))
}

// ---- GET /runs/{id}/download -----------------------------------------------

#[utoipa::path(
    get,
    operation_id = "download_merge_run",
    path = "/api/v1/merge/runs/{id}/download",
    params(("id" = Uuid, Path,)),
    responses(
        (status = 200, description = "Output file bytes", content_type = "application/octet-stream"),
        (status = 400, description = "Run not yet completed"),
        (status = 401),
        (status = 403),
        (status = 404),
    ),
    security(("bearer_auth" = [])),
    tag = "merge",
)]
pub(crate) async fn download_run(
    State(state): State<AppState>,
    user: VerifiedUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Response> {
    let mr = merge_runs::get(&state.db, id).await.map_err(|e| ApiError(e.into()))?
        .ok_or_else(|| ApiError(AppError::NotFound))?;
    if mr.owner_id != user.user_id {
        return Err(ApiError(AppError::Forbidden));
    }
    if mr.status != MergeStatus::Completed {
        return Err(ApiError(AppError::BadRequest("run not completed yet".into())));
    }
    let output_id = mr.output_file_id
        .ok_or_else(|| ApiError(AppError::Internal("completed run has no output file".into())))?;

    let out = files_db::get_output(&state.db, output_id)
        .await.map_err(|e| ApiError(e.into()))?
        .ok_or_else(|| ApiError(AppError::NotFound))?;
    if out.purged_at.is_some() {
        return Err(ApiError(AppError::NotFound));
    }

    let bytes = state.storage.get(&out.storage_key).await.map_err(ApiError)?;
    let (content_type, ext) = match out.kind {
        price_merger_core::models::FileKind::Csv => ("text/csv", "csv"),
        price_merger_core::models::FileKind::Xls => (
            "application/vnd.ms-excel", "xls",
        ),
        price_merger_core::models::FileKind::Xlsx => (
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", "xlsx",
        ),
    };
    let cd = format!("attachment; filename=\"output.{ext}\"");

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type.to_string()),
            (header::CONTENT_DISPOSITION, cd),
        ],
        bytes,
    ).into_response())
}
