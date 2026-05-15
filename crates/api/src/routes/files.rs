//! File upload, listing, and download.
//!
//! The upload handler streams the multipart body into object storage, then
//! writes a row to `uploaded_files`. Files are accessible until `expires_at`,
//! after which the periodic purge job removes them.

use axum::{
    extract::{Multipart, Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use bytes::BytesMut;
use price_merger_auth::permissions::{load_permissions, require, Permission};
use price_merger_core::{models::FileKind, AppError};
use price_merger_db::files as files_db;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::{Duration as TDuration, OffsetDateTime};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::middleware::auth::VerifiedUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(upload).get(list))
        .route("/{id}", get(download).delete(delete_file))
        .route("/{id}/preview", get(preview))
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct FilePreviewResponse {
    /// Column headers from the first row, in source order.
    pub headers: Vec<String>,
    /// Data rows (each row is an array of cell values).
    #[schema(value_type = Vec<Vec<serde_json::Value>>)]
    pub rows: Vec<Vec<serde_json::Value>>,
    /// Sheet names — populated for XLS/XLSX files, empty for CSV.
    pub sheet_names: Vec<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/files",
    request_body(content_type = "multipart/form-data", content = inline(String)),
    responses(
        (status = 201, description = "File uploaded", body = price_merger_core::models::UploadedFile),
        (status = 400),
        (status = 401),
        (status = 403),
    ),
    security(("bearer_auth" = [])),
    tag = "files",
)]
pub(crate) async fn upload(
    State(state): State<AppState>,
    user: VerifiedUser,
    mut mp: Multipart,
) -> ApiResult<(StatusCode, Json<price_merger_core::models::UploadedFile>)> {
    let perms = load_permissions(&state.db, user.user_id).await.map_err(ApiError)?;
    require(&perms, Permission::FilesUpload).map_err(ApiError)?;

    let max_bytes = (state.settings.files.max_upload_mb as usize) * 1024 * 1024;

    while let Some(field) = mp.next_field().await
        .map_err(|e| ApiError(AppError::BadRequest(format!("multipart: {e}"))))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name != "file" { continue; }

        let original_name = field.file_name().unwrap_or("upload").to_string();
        let mime = field.content_type().map(|s| s.to_string());

        // Determine kind from extension; reject unsupported extensions.
        let ext = std::path::Path::new(&original_name)
            .extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
        if !state.settings.files.allowed_extensions.iter().any(|a| a.eq_ignore_ascii_case(&ext)) {
            return Err(ApiError(AppError::BadRequest(format!("ext not allowed: {ext}"))));
        }
        let kind = FileKind::from_extension(&ext)
            .ok_or_else(|| ApiError(AppError::BadRequest("unknown file kind".into())))?;

        // Stream to memory (with a hard cap) and hash on the fly.
        // For very large files, switch this to a temp-file or chunked upload.
        let mut buf = BytesMut::new();
        let mut hasher = Sha256::new();
        let mut data = field;
        while let Some(chunk) = data.chunk().await
            .map_err(|e| ApiError(AppError::BadRequest(format!("multipart chunk: {e}"))))?
        {
            if buf.len() + chunk.len() > max_bytes {
                return Err(ApiError(AppError::BadRequest(
                    format!("file exceeds {} MB", state.settings.files.max_upload_mb))));
            }
            hasher.update(&chunk);
            buf.extend_from_slice(&chunk);
        }
        let sha = hex::encode(hasher.finalize());
        let bytes = buf.freeze();
        let size = bytes.len() as i64;

        let storage_key = format!(
            "uploads/{}/{}.{}",
            user.user_id, Uuid::new_v4(), kind.extension(),
        );
        state.storage.put(&storage_key, bytes.clone(), mime.as_deref()).await.map_err(ApiError)?;

        let headers = price_merger_file_processor::peek(&bytes, kind, None, 0)
            .ok()
            .map(|p| p.headers)
            .filter(|h| !h.is_empty());

        let expires_at = OffsetDateTime::now_utc()
            + TDuration::hours(state.settings.files.ttl_hours);

        let row = files_db::insert_uploaded(&state.db, files_db::NewUploadedFile {
            owner_id: user.user_id,
            original_name: &original_name,
            storage_key: &storage_key,
            kind,
            mime_type: mime.as_deref(),
            size_bytes: size,
            sha256: Some(&sha),
            expires_at,
            headers,
        }).await.map_err(|e| ApiError(e.into()))?;

        return Ok((StatusCode::CREATED, Json(row)));
    }
    Err(ApiError(AppError::BadRequest("missing 'file' field".into())))
}

#[utoipa::path(
    get,
    operation_id = "list_files",
    path = "/api/v1/files",
    responses(
        (status = 200, body = Vec<price_merger_core::models::UploadedFile>),
        (status = 401),
    ),
    security(("bearer_auth" = [])),
    tag = "files",
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    user: VerifiedUser,
) -> ApiResult<Json<Vec<price_merger_core::models::UploadedFile>>> {
    let files = files_db::list_uploaded_by_owner(&state.db, user.user_id, 100, 0)
        .await.map_err(|e| ApiError(e.into()))?;
    Ok(Json(files))
}

#[utoipa::path(
    get,
    path = "/api/v1/files/{id}",
    params(("id" = Uuid, Path,)),
    responses(
        (status = 200, description = "File bytes", content_type = "application/octet-stream"),
        (status = 401),
        (status = 403),
        (status = 404),
    ),
    security(("bearer_auth" = [])),
    tag = "files",
)]
pub(crate) async fn download(
    State(state): State<AppState>,
    user: VerifiedUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Response> {
    let file = files_db::get_uploaded(&state.db, id)
        .await.map_err(|e| ApiError(e.into()))?
        .ok_or_else(|| ApiError(AppError::NotFound))?;

    // Owner or files.read_all
    if file.owner_id != user.user_id {
        let perms = load_permissions(&state.db, user.user_id).await.map_err(ApiError)?;
        require(&perms, Permission::FilesReadAll).map_err(ApiError)?;
    }
    if file.purged_at.is_some() {
        return Err(ApiError(AppError::NotFound));
    }

    let bytes = state.storage.get(&file.storage_key).await.map_err(ApiError)?;
    let mime = file.mime_type.as_deref().unwrap_or("application/octet-stream");
    let cd = format!("attachment; filename=\"{}\"", file.original_name.replace('\"', ""));
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, mime.to_string()),
            (header::CONTENT_DISPOSITION, cd),
        ],
        bytes,
    ).into_response())
}

#[utoipa::path(
    delete,
    path = "/api/v1/files/{id}",
    params(("id" = Uuid, Path)),
    responses(
        (status = 202, description = "Deletion enqueued"),
        (status = 401),
        (status = 403),
        (status = 404),
    ),
    security(("bearer_auth" = [])),
    tag = "files",
)]
pub(crate) async fn delete_file(
    State(state): State<AppState>,
    user: VerifiedUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let file = files_db::get_uploaded(&state.db, id)
        .await.map_err(|e| ApiError(e.into()))?
        .ok_or_else(|| ApiError(AppError::NotFound))?;

    let perms = load_permissions(&state.db, user.user_id).await.map_err(ApiError)?;
    if file.owner_id == user.user_id {
        require(&perms, Permission::FilesDeleteOwn).map_err(ApiError)?;
    } else {
        require(&perms, Permission::FilesDeleteAny).map_err(ApiError)?;
    }

    // Don't block on storage; let the purge job clean up.
    price_merger_db::jobs::enqueue(
        &state.db,
        &price_merger_core::models::JobPayload::FilePurge { file_id: id, is_output: false },
        None,
    ).await.map_err(|e| ApiError(e.into()))?;

    Ok(StatusCode::ACCEPTED)
}

// ---- preview ---------------------------------------------------------------

#[derive(Debug, Deserialize, IntoParams)]
pub(crate) struct PreviewQuery {
    /// Number of data rows to return. Clamped to 10–50. Default: 20.
    pub rows: Option<u32>,
    /// Sheet name for XLS/XLSX files. Defaults to the first sheet.
    pub sheet: Option<String>,
}

#[utoipa::path(
    get,
    operation_id = "preview_file",
    path = "/api/v1/files/{id}/preview",
    params(("id" = Uuid, Path), PreviewQuery),
    responses(
        (status = 200, body = FilePreviewResponse),
        (status = 401),
        (status = 403),
        (status = 404),
    ),
    security(("bearer_auth" = [])),
    tag = "files",
)]
pub(crate) async fn preview(
    State(state): State<AppState>,
    user: VerifiedUser,
    Path(id): Path<Uuid>,
    Query(q): Query<PreviewQuery>,
) -> ApiResult<Json<FilePreviewResponse>> {
    let file = files_db::get_uploaded(&state.db, id)
        .await.map_err(|e| ApiError(e.into()))?
        .ok_or_else(|| ApiError(AppError::NotFound))?;

    if file.owner_id != user.user_id {
        let perms = load_permissions(&state.db, user.user_id).await.map_err(ApiError)?;
        require(&perms, Permission::FilesReadAll).map_err(ApiError)?;
    }
    if file.purged_at.is_some() {
        return Err(ApiError(AppError::NotFound));
    }

    let limit = q.rows.unwrap_or(20).clamp(10, 50) as usize;
    let bytes = state.storage.get(&file.storage_key).await.map_err(ApiError)?;

    let peek = price_merger_file_processor::peek(
        &bytes,
        file.kind,
        q.sheet.as_deref(),
        limit,
    ).map_err(ApiError)?;

    let rows = peek.rows.into_iter()
        .map(|row| {
            row.into_iter()
                .map(|v| serde_json::to_value(&v).unwrap_or(serde_json::Value::Null))
                .collect()
        })
        .collect();

    Ok(Json(FilePreviewResponse {
        headers: peek.headers,
        rows,
        sheet_names: peek.sheet_names,
    }))
}