use price_merger_core::models::{FileKind, OutputFile, UploadedFile};
use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::types::DbFileKind;
use crate::DbResult;

#[derive(Debug, Clone)]
pub struct NewUploadedFile<'a> {
    pub owner_id: Uuid,
    pub original_name: &'a str,
    pub storage_key: &'a str,
    pub kind: FileKind,
    pub mime_type: Option<&'a str>,
    pub size_bytes: i64,
    pub sha256: Option<&'a str>,
    pub expires_at: OffsetDateTime,
    pub headers: Option<Vec<String>>,
}

pub async fn insert_uploaded(pool: &PgPool, n: NewUploadedFile<'_>) -> DbResult<UploadedFile> {
    let kind = DbFileKind::from(n.kind);
    let headers_json: Option<serde_json::Value> =
        n.headers.as_ref().map(|h| serde_json::json!(h));

    let row = sqlx::query(
        r#"
        INSERT INTO uploaded_files
            (owner_id, original_name, storage_key, kind, mime_type, size_bytes, sha256,
             expires_at, headers)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id, owner_id, original_name, storage_key,
                  kind, mime_type, size_bytes, sha256,
                  expires_at, purged_at, created_at, headers
        "#,
    )
    .bind(n.owner_id)
    .bind(n.original_name)
    .bind(n.storage_key)
    .bind(kind)
    .bind(n.mime_type)
    .bind(n.size_bytes)
    .bind(n.sha256)
    .bind(n.expires_at)
    .bind(headers_json)
    .fetch_one(pool)
    .await?;

    Ok(UploadedFile {
        id: row.try_get("id")?,
        owner_id: row.try_get("owner_id")?,
        original_name: row.try_get("original_name")?,
        storage_key: row.try_get("storage_key")?,
        kind: row.try_get::<DbFileKind, _>("kind")?.into(),
        mime_type: row.try_get("mime_type")?,
        size_bytes: row.try_get("size_bytes")?,
        sha256: row.try_get("sha256")?,
        headers: row.try_get::<Option<serde_json::Value>, _>("headers")?
            .and_then(|v| serde_json::from_value(v).ok()),
        expires_at: row.try_get("expires_at")?,
        purged_at: row.try_get("purged_at")?,
        created_at: row.try_get("created_at")?,
    })
}

pub async fn get_uploaded(pool: &PgPool, id: Uuid) -> DbResult<Option<UploadedFile>> {
    let row = sqlx::query(
        r#"
        SELECT id, owner_id, original_name, storage_key,
               kind, mime_type, size_bytes, sha256,
               expires_at, purged_at, created_at, headers
        FROM uploaded_files WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    row.map(|r: sqlx::postgres::PgRow| -> DbResult<UploadedFile> {
        Ok(UploadedFile {
            id: r.try_get("id")?,
            owner_id: r.try_get("owner_id")?,
            original_name: r.try_get("original_name")?,
            storage_key: r.try_get("storage_key")?,
            kind: r.try_get::<DbFileKind, _>("kind")?.into(),
            mime_type: r.try_get("mime_type")?,
            size_bytes: r.try_get("size_bytes")?,
            sha256: r.try_get("sha256")?,
            headers: r.try_get::<Option<serde_json::Value>, _>("headers")?
                .and_then(|v| serde_json::from_value(v).ok()),
            expires_at: r.try_get("expires_at")?,
            purged_at: r.try_get("purged_at")?,
            created_at: r.try_get("created_at")?,
        })
    })
    .transpose()
}

pub async fn list_uploaded_by_owner(
    pool: &PgPool, owner_id: Uuid, limit: i64, offset: i64,
) -> DbResult<Vec<UploadedFile>> {
    let rows = sqlx::query(
        r#"
        SELECT id, owner_id, original_name, storage_key,
               kind, mime_type, size_bytes, sha256,
               expires_at, purged_at, created_at, headers
        FROM uploaded_files
        WHERE owner_id = $1 AND purged_at IS NULL
        ORDER BY created_at DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(owner_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r: sqlx::postgres::PgRow| -> DbResult<UploadedFile> {
            Ok(UploadedFile {
                id: r.try_get("id")?,
                owner_id: r.try_get("owner_id")?,
                original_name: r.try_get("original_name")?,
                storage_key: r.try_get("storage_key")?,
                kind: r.try_get::<DbFileKind, _>("kind")?.into(),
                mime_type: r.try_get("mime_type")?,
                size_bytes: r.try_get("size_bytes")?,
                sha256: r.try_get("sha256")?,
                headers: r.try_get::<Option<serde_json::Value>, _>("headers")?
                    .and_then(|v| serde_json::from_value(v).ok()),
                expires_at: r.try_get("expires_at")?,
                purged_at: r.try_get("purged_at")?,
                created_at: r.try_get("created_at")?,
            })
        })
        .collect()
}

pub async fn mark_purged(pool: &PgPool, id: Uuid, is_output: bool) -> DbResult<()> {
    let table = if is_output { "output_files" } else { "uploaded_files" };
    let sql = format!("UPDATE {table} SET purged_at = now() WHERE id = $1");
    sqlx::query(&sql).bind(id).execute(pool).await?;
    Ok(())
}

/// Files that are past their TTL and not yet purged.
pub async fn list_expired(
    pool: &PgPool, now: OffsetDateTime, limit: i64,
) -> DbResult<Vec<(Uuid, bool)>> {
    let uploaded = sqlx::query!(
        "SELECT id FROM uploaded_files
         WHERE purged_at IS NULL AND expires_at <= $1
         ORDER BY expires_at LIMIT $2",
        now, limit,
    ).fetch_all(pool).await?;
    let outputs = sqlx::query!(
        "SELECT id FROM output_files
         WHERE purged_at IS NULL AND expires_at <= $1
         ORDER BY expires_at LIMIT $2",
        now, limit,
    ).fetch_all(pool).await?;
    let mut v: Vec<(Uuid, bool)> = uploaded.into_iter().map(|r| (r.id, false)).collect();
    v.extend(outputs.into_iter().map(|r| (r.id, true)));
    Ok(v)
}

#[derive(Debug, Clone)]
pub struct NewOutputFile<'a> {
    pub owner_id: Uuid,
    pub storage_key: &'a str,
    pub kind: FileKind,
    pub size_bytes: i64,
    pub expires_at: OffsetDateTime,
}

pub async fn get_output(pool: &PgPool, id: Uuid) -> DbResult<Option<OutputFile>> {
    let row = sqlx::query(
        r#"
        SELECT id, owner_id, storage_key,
               kind, size_bytes, expires_at, purged_at, created_at
        FROM output_files WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    row.map(|r: sqlx::postgres::PgRow| -> DbResult<OutputFile> {
        Ok(OutputFile {
            id: r.try_get("id")?,
            owner_id: r.try_get("owner_id")?,
            storage_key: r.try_get("storage_key")?,
            kind: r.try_get::<DbFileKind, _>("kind")?.into(),
            size_bytes: r.try_get("size_bytes")?,
            expires_at: r.try_get("expires_at")?,
            purged_at: r.try_get("purged_at")?,
            created_at: r.try_get("created_at")?,
        })
    })
    .transpose()
}

pub async fn insert_output(pool: &PgPool, n: NewOutputFile<'_>) -> DbResult<OutputFile> {
    let kind = DbFileKind::from(n.kind);
    let row = sqlx::query!(
        r#"
        INSERT INTO output_files (owner_id, storage_key, kind, size_bytes, expires_at)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, owner_id, storage_key, kind AS "kind!: DbFileKind",
                  size_bytes, expires_at, purged_at, created_at
        "#,
        n.owner_id, n.storage_key, kind as DbFileKind, n.size_bytes, n.expires_at,
    ).fetch_one(pool).await?;
    Ok(OutputFile {
        id: row.id, owner_id: row.owner_id, storage_key: row.storage_key,
        kind: row.kind.into(), size_bytes: row.size_bytes,
        expires_at: row.expires_at, purged_at: row.purged_at, created_at: row.created_at,
    })
}
