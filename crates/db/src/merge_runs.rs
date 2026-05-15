use price_merger_core::models::{MergeRun, MergeStatus};
use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::DbResult;

pub async fn create(
    pool: &PgPool,
    owner_id: Uuid,
    output_format_id: Uuid,
    input_mapping_ids: &[Uuid],
) -> DbResult<MergeRun> {
    let row = sqlx::query!(
        r#"
        INSERT INTO merge_runs (owner_id, output_format_id, input_mapping_ids)
        VALUES ($1, $2, $3)
        RETURNING id, owner_id, output_format_id, input_mapping_ids,
                  status::text AS "status!", output_file_id, error_message,
                  created_at, started_at, finished_at
        "#,
        owner_id, output_format_id, input_mapping_ids,
    ).fetch_one(pool).await?;
    Ok(map(row.id, row.owner_id, row.output_format_id, row.input_mapping_ids,
           &row.status, row.output_file_id, row.error_message,
           row.created_at, row.started_at, row.finished_at))
}

pub async fn get(pool: &PgPool, id: Uuid) -> DbResult<Option<MergeRun>> {
    let row = sqlx::query!(
        r#"
        SELECT id, owner_id, output_format_id, input_mapping_ids,
               status::text AS "status!", output_file_id, error_message,
               created_at, started_at, finished_at
        FROM merge_runs WHERE id = $1
        "#, id,
    ).fetch_optional(pool).await?;
    Ok(row.map(|r| map(r.id, r.owner_id, r.output_format_id, r.input_mapping_ids,
                       &r.status, r.output_file_id, r.error_message,
                       r.created_at, r.started_at, r.finished_at)))
}

pub async fn mark_running(pool: &PgPool, id: Uuid) -> DbResult<()> {
    sqlx::query!(
        "UPDATE merge_runs SET status = 'running', started_at = now() WHERE id = $1", id,
    ).execute(pool).await?;
    Ok(())
}

pub async fn mark_completed(pool: &PgPool, id: Uuid, output_file_id: Uuid) -> DbResult<()> {
    sqlx::query!(
        "UPDATE merge_runs SET status = 'completed', finished_at = now(), output_file_id = $2
         WHERE id = $1", id, output_file_id,
    ).execute(pool).await?;
    Ok(())
}

pub async fn mark_failed(pool: &PgPool, id: Uuid, err: &str) -> DbResult<()> {
    sqlx::query!(
        "UPDATE merge_runs SET status = 'failed', finished_at = now(), error_message = $2
         WHERE id = $1", id, err,
    ).execute(pool).await?;
    Ok(())
}

pub async fn list_by_owner(pool: &PgPool, owner_id: Uuid) -> DbResult<Vec<MergeRun>> {
    let rows = sqlx::query(
        r#"
        SELECT id, owner_id, output_format_id, input_mapping_ids,
               status::text AS status, output_file_id, error_message,
               created_at, started_at, finished_at
        FROM merge_runs WHERE owner_id = $1 ORDER BY created_at DESC
        "#,
    )
    .bind(owner_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r: sqlx::postgres::PgRow| -> DbResult<MergeRun> {
            let status: String = r.try_get("status")?;
            Ok(map(
                r.try_get("id")?,
                r.try_get("owner_id")?,
                r.try_get("output_format_id")?,
                r.try_get("input_mapping_ids")?,
                &status,
                r.try_get("output_file_id")?,
                r.try_get("error_message")?,
                r.try_get("created_at")?,
                r.try_get("started_at")?,
                r.try_get("finished_at")?,
            ))
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn map(
    id: Uuid, owner_id: Uuid, output_format_id: Uuid, inputs: Vec<Uuid>,
    status: &str, output_file_id: Option<Uuid>, error_message: Option<String>,
    created_at: OffsetDateTime, started_at: Option<OffsetDateTime>,
    finished_at: Option<OffsetDateTime>,
) -> MergeRun {
    let status = match status {
        "queued" => MergeStatus::Queued,
        "running" => MergeStatus::Running,
        "completed" => MergeStatus::Completed,
        "failed" => MergeStatus::Failed,
        "cancelled" => MergeStatus::Cancelled,
        _ => MergeStatus::Queued,
    };
    MergeRun {
        id, owner_id, output_format_id,
        input_mapping_ids: inputs,
        status, output_file_id, error_message,
        created_at, started_at, finished_at,
    }
}
