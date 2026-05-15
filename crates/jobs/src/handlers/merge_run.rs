//! Execute a merge: read inputs from object storage, run the file-processor
//! merge pipeline, write the output back to storage, register the output file
//! row, and update the merge_runs row.

use async_trait::async_trait;
use price_merger_core::models::FileKind;
use price_merger_core::AppError;
use price_merger_core::AppResult;
use price_merger_db::{files as files_db, formats, mappings, merge_runs};
use price_merger_file_processor::{merge as run_merge, MergeInput};
use time::{Duration as TDuration, OffsetDateTime};
use uuid::Uuid;

use super::{Context, Handler};

pub struct Run {
    pub merge_run_id: Uuid,
}

#[async_trait]
impl Handler for Run {
    async fn handle(&self, ctx: &Context) -> AppResult<()> {
        let mr = merge_runs::get(&ctx.pool, self.merge_run_id).await?
            .ok_or(AppError::NotFound)?;
        merge_runs::mark_running(&ctx.pool, mr.id).await?;

        let result = process(ctx, &mr).await;
        match result {
            Ok(output_file_id) => {
                merge_runs::mark_completed(&ctx.pool, mr.id, output_file_id).await?;
                Ok(())
            }
            Err(e) => {
                let msg = e.to_string();
                merge_runs::mark_failed(&ctx.pool, mr.id, &msg).await?;
                Err(e)
            }
        }
    }
}

async fn process(
    ctx: &Context,
    mr: &price_merger_core::models::MergeRun,
) -> AppResult<Uuid> {
    let format = formats::get(&ctx.pool, mr.output_format_id).await?
        .ok_or(AppError::NotFound)?;

    // Resolve mappings + load input files into memory.
    let mut inputs_meta = Vec::with_capacity(mr.input_mapping_ids.len());
    for mapping_id in &mr.input_mapping_ids {
        let mapping = mappings::get(&ctx.pool, *mapping_id).await?
            .ok_or(AppError::NotFound)?;
        let file = files_db::get_uploaded(&ctx.pool, mapping.file_id).await?
            .ok_or(AppError::NotFound)?;
        let bytes = ctx.storage.get(&file.storage_key).await?;
        inputs_meta.push((mapping, file.kind, bytes));
    }

    let inputs: Vec<MergeInput> = inputs_meta
        .iter()
        .map(|(m, kind, bytes)| MergeInput {
            bytes: bytes.as_ref(),
            kind: *kind,
            mapping: m,
        })
        .collect();

    let out_bytes = run_merge(&inputs, &format)?;

    // Write the output to storage and register the row.
    let output_ext = format.output_extension;
    let uuid_str;
    let base = match &format.filename {
        Some(name) => name.as_str(),
        None => { uuid_str = Uuid::new_v4().to_string(); &uuid_str }
    };
    let key = format!("outputs/{}/{}.{}", mr.owner_id, base, output_ext.extension());
    let content_type = match output_ext {
        FileKind::Csv => "text/csv",
        FileKind::Xls | FileKind::Xlsx =>
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    };
    ctx.storage.put(&key, bytes::Bytes::from(out_bytes.clone()), Some(content_type)).await?;

    let expires_at = OffsetDateTime::now_utc() + TDuration::hours(72);
    let out_row = files_db::insert_output(&ctx.pool, files_db::NewOutputFile {
        owner_id: mr.owner_id,
        storage_key: &key,
        kind: output_ext,
        size_bytes: out_bytes.len() as i64,
        expires_at,
    }).await?;

    Ok(out_row.id)
}
