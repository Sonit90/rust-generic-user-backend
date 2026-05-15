use async_trait::async_trait;
use price_merger_core::AppResult;
use uuid::Uuid;

use super::{Context, Handler};

pub struct Run {
    pub file_id: Uuid,
    pub is_output: bool,
}

#[async_trait]
impl Handler for Run {
    async fn handle(&self, ctx: &Context) -> AppResult<()> {
        // Look up the storage key from the appropriate table, delete the
        // object, then mark the row as purged.
        let key: Option<String> = if self.is_output {
            sqlx::query_scalar!(
                "SELECT storage_key FROM output_files WHERE id = $1",
                self.file_id,
            ).fetch_optional(&ctx.pool).await
                .map_err(|e| price_merger_core::AppError::Database(e.to_string()))?
        } else {
            sqlx::query_scalar!(
                "SELECT storage_key FROM uploaded_files WHERE id = $1",
                self.file_id,
            ).fetch_optional(&ctx.pool).await
                .map_err(|e| price_merger_core::AppError::Database(e.to_string()))?
        };

        let Some(storage_key) = key else { return Ok(()) };

        // Best-effort delete; "not found" is fine.
        if let Err(e) = ctx.storage.delete(&storage_key).await {
            tracing::warn!(file_id = %self.file_id, "storage delete failed: {e}");
        }

        price_merger_db::files::mark_purged(&ctx.pool, self.file_id, self.is_output).await?;
        Ok(())
    }
}
