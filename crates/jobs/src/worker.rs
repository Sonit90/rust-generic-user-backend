use std::sync::Arc;
use std::time::Duration;

use price_merger_core::models::JobPayload;
use sqlx::PgPool;
use tokio::time::sleep;
use tracing::{error, info, warn};

use crate::handlers::Context;
use crate::ObjectStore;

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub workers: usize,
    pub poll_interval_secs: u64,
    pub visibility_timeout_secs: i64,
    pub batch_size: i64,
    pub purge_sweep_interval_secs: u64,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            workers: 4,
            poll_interval_secs: 5,
            visibility_timeout_secs: 300,
            batch_size: 4,
            purge_sweep_interval_secs: 600,
        }
    }
}

/// Worker pool. `run` blocks until cancelled.
pub struct Worker {
    pool: PgPool,
    storage: Arc<ObjectStore>,
    cfg: WorkerConfig,
    worker_id: String,
}

impl Worker {
    pub fn new(pool: PgPool, storage: Arc<ObjectStore>, cfg: WorkerConfig) -> Self {
        let worker_id = format!("worker-{}", uuid::Uuid::new_v4());
        Self {
            pool,
            storage,
            cfg,
            worker_id,
        }
    }

    pub async fn run(self, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        info!(worker_id = %self.worker_id, "starting worker pool");

        // Spawn the periodic stale-job janitor and TTL sweep.
        let janitor = tokio::spawn(janitor_loop(
            self.pool.clone(),
            self.cfg.visibility_timeout_secs,
            self.cfg.purge_sweep_interval_secs,
            shutdown.clone(),
        ));

        // Spawn N concurrent worker tasks all polling the same pool.
        let mut handles = Vec::with_capacity(self.cfg.workers);
        for i in 0..self.cfg.workers {
            let pool = self.pool.clone();
            let storage = self.storage.clone();
            let cfg = self.cfg.clone();
            let mut shutdown = shutdown.clone();
            let worker_id = format!("{}-{}", self.worker_id, i);
            handles.push(tokio::spawn(async move {
                let ctx = Context {
                    pool: pool.clone(),
                    storage: storage.clone(),
                };
                loop {
                    tokio::select! {
                        _ = shutdown.changed() => { break; }
                        _ = poll_once(&pool, &ctx, &worker_id, cfg.batch_size) => {
                            sleep(Duration::from_secs(cfg.poll_interval_secs)).await;
                        }
                    }
                }
            }));
        }

        // Wait for shutdown.
        let _ = shutdown.changed().await;
        for h in handles {
            let _ = h.await;
        }
        let _ = janitor.await;
    }
}

async fn poll_once(pool: &PgPool, ctx: &Context, worker_id: &str, batch: i64) {
    match price_merger_db::jobs::claim_batch(pool, worker_id, batch).await {
        Ok(jobs) if !jobs.is_empty() => {
            for job in jobs {
                let payload: Result<JobPayload, _> = serde_json::from_value(job.payload.clone());
                let result = match payload {
                    Ok(p) => crate::handlers::dispatch(ctx, p).await,
                    Err(e) => Err(price_merger_core::AppError::internal(format!(
                        "decode job payload: {e}"
                    ))),
                };
                match result {
                    Ok(_) => {
                        if let Err(e) = price_merger_db::jobs::mark_completed(pool, job.id).await {
                            error!(job_id = %job.id, "mark completed failed: {e:?}");
                        }
                    }
                    Err(e) => {
                        warn!(job_id = %job.id, kind = %job.kind, "job failed: {e}");
                        if let Err(e2) =
                            price_merger_db::jobs::mark_failed(pool, job.id, &e.to_string()).await
                        {
                            error!(job_id = %job.id, "mark failed errored: {e2:?}");
                        }
                    }
                }
            }
        }
        Ok(_) => { /* idle */ }
        Err(e) => warn!("claim_batch failed: {e}"),
    }
}

async fn janitor_loop(
    pool: PgPool,
    visibility_secs: i64,
    sweep_secs: u64,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let mut last_sweep = std::time::Instant::now();
    loop {
        tokio::select! {
            _ = shutdown.changed() => break,
            _ = sleep(Duration::from_secs(30)) => {}
        }
        if let Err(e) = price_merger_db::jobs::requeue_stale(&pool, visibility_secs).await {
            warn!("requeue_stale: {e}");
        }
        if last_sweep.elapsed() >= Duration::from_secs(sweep_secs) {
            // Enqueue a periodic sweep; the handler does the actual work.
            let _ = price_merger_db::jobs::enqueue(&pool, &JobPayload::PurgeSweep, None).await;
            last_sweep = std::time::Instant::now();
        }
    }
}
