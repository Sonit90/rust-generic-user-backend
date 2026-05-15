//! Periodic sweep — find files past TTL and enqueue purge jobs for each.

use async_trait::async_trait;
use price_merger_core::models::JobPayload;
use price_merger_core::AppResult;
use time::OffsetDateTime;

use super::{Context, Handler};

pub struct Run;

#[async_trait]
impl Handler for Run {
    async fn handle(&self, ctx: &Context) -> AppResult<()> {
        let expired = price_merger_db::files::list_expired(
            &ctx.pool, OffsetDateTime::now_utc(), 1000,
        ).await?;

        for (file_id, is_output) in expired {
            let _ = price_merger_db::jobs::enqueue(
                &ctx.pool,
                &JobPayload::FilePurge { file_id, is_output },
                None,
            ).await;
        }
        Ok(())
    }
}
