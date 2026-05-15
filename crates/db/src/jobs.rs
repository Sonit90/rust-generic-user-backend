//! Postgres-backed job queue queries.
//!
//! Workers claim a batch of due jobs with `FOR UPDATE SKIP LOCKED` to avoid
//! contention. The `requeue_stale_jobs` function (defined in the migration)
//! revives jobs whose lock leaked from a dead worker.

use price_merger_core::models::{Job, JobPayload, JobStatus};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::DbResult;

pub async fn enqueue(
    pool: &PgPool, payload: &JobPayload, run_at: Option<OffsetDateTime>,
) -> DbResult<Uuid> {
    let kind = payload.kind();
    let payload_json = serde_json::to_value(payload).expect("serializable");
    let id = sqlx::query_scalar!(
        r#"
        INSERT INTO jobs (kind, payload, run_at)
        VALUES ($1, $2, COALESCE($3, now()))
        RETURNING id
        "#,
        kind, payload_json, run_at,
    ).fetch_one(pool).await?;
    Ok(id)
}

/// Atomically claim up to `batch` due jobs and return them as `Running`.
/// `worker_id` is recorded so we know who's holding the lock.
pub async fn claim_batch(
    pool: &PgPool, worker_id: &str, batch: i64,
) -> DbResult<Vec<Job>> {
    let rows = sqlx::query!(
        r#"
        WITH due AS (
            SELECT id FROM jobs
            WHERE status = 'queued' AND run_at <= now()
            ORDER BY run_at
            FOR UPDATE SKIP LOCKED
            LIMIT $1
        )
        UPDATE jobs j
           SET status    = 'running',
               attempts  = j.attempts + 1,
               locked_at = now(),
               locked_by = $2
          FROM due
         WHERE j.id = due.id
         RETURNING j.id, j.kind, j.payload,
                   j.status::text AS "status!",
                   j.attempts, j.max_attempts, j.run_at, j.last_error,
                   j.created_at, j.finished_at
        "#,
        batch, worker_id,
    ).fetch_all(pool).await?;

    Ok(rows.into_iter().map(|r| Job {
        id: r.id,
        kind: r.kind,
        payload: r.payload,
        status: parse_status(&r.status),
        attempts: r.attempts,
        max_attempts: r.max_attempts,
        run_at: r.run_at,
        last_error: r.last_error,
        created_at: r.created_at,
        finished_at: r.finished_at,
    }).collect())
}

pub async fn mark_completed(pool: &PgPool, id: Uuid) -> DbResult<()> {
    sqlx::query!(
        "UPDATE jobs SET status='completed', finished_at = now(),
                          locked_at=NULL, locked_by=NULL WHERE id = $1", id,
    ).execute(pool).await?;
    Ok(())
}

/// Either retry (if attempts < max_attempts) with exponential backoff, or
/// move to the dead-letter state.
pub async fn mark_failed(pool: &PgPool, id: Uuid, err: &str) -> DbResult<()> {
    sqlx::query!(
        r#"
        UPDATE jobs
           SET status     = CASE WHEN attempts >= max_attempts THEN 'dead'::job_status
                                 ELSE 'queued'::job_status END,
               last_error = $2,
               run_at     = CASE WHEN attempts >= max_attempts THEN run_at
                                 ELSE now() + (interval '5 seconds' * (2 ^ attempts)) END,
               locked_at  = NULL,
               locked_by  = NULL,
               finished_at = CASE WHEN attempts >= max_attempts THEN now() ELSE NULL END
         WHERE id = $1
        "#, id, err,
    ).execute(pool).await?;
    Ok(())
}

pub async fn requeue_stale(pool: &PgPool, visibility_secs: i64) -> DbResult<i64> {
    let n = sqlx::query_scalar!(
        "SELECT requeue_stale_jobs(make_interval(secs => $1))",
        visibility_secs as f64,
    ).fetch_one(pool).await?;
    Ok(n.unwrap_or(0) as i64)
}

fn parse_status(s: &str) -> JobStatus {
    match s {
        "queued" => JobStatus::Queued,
        "running" => JobStatus::Running,
        "completed" => JobStatus::Completed,
        "failed" => JobStatus::Failed,
        "dead" => JobStatus::Dead,
        _ => JobStatus::Queued,
    }
}
