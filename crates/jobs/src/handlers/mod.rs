use std::sync::Arc;

use async_trait::async_trait;
use price_merger_core::models::JobPayload;
use price_merger_core::AppResult;
use sqlx::PgPool;

use crate::ObjectStore;

pub mod file_purge;
pub mod merge_run;
pub mod purge_sweep;

#[derive(Clone)]
pub struct Context {
    pub pool: PgPool,
    pub storage: Arc<ObjectStore>,
}

#[async_trait]
pub trait Handler {
    async fn handle(&self, ctx: &Context) -> AppResult<()>;
}

pub async fn dispatch(ctx: &Context, payload: JobPayload) -> AppResult<()> {
    match payload {
        JobPayload::FilePurge { file_id, is_output } =>
            file_purge::Run { file_id, is_output }.handle(ctx).await,
        JobPayload::MergeRun { merge_run_id } =>
            merge_run::Run { merge_run_id }.handle(ctx).await,
        JobPayload::PurgeSweep => purge_sweep::Run.handle(ctx).await,
    }
}
