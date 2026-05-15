use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Dead,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MergeStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Job {
    pub id: Uuid,
    pub kind: String,
    #[schema(value_type = Object)]
    pub payload: serde_json::Value,
    pub status: JobStatus,
    pub attempts: i32,
    pub max_attempts: i32,
    #[schema(value_type = String, format = DateTime)]
    pub run_at: OffsetDateTime,
    pub last_error: Option<String>,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    #[schema(value_type = String, format = DateTime, nullable = true)]
    pub finished_at: Option<OffsetDateTime>,
}

/// Strongly-typed payloads. Each variant maps to a `kind` string.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JobPayload {
    /// Purge a single uploaded or output file from storage and mark it purged.
    FilePurge { file_id: Uuid, is_output: bool },
    /// Run a merge: combine N input files into one output via a format.
    MergeRun { merge_run_id: Uuid },
    /// Sweep for files past their TTL and enqueue purge jobs.
    PurgeSweep,
}

impl JobPayload {
    pub fn kind(&self) -> &'static str {
        match self {
            JobPayload::FilePurge { .. } => "file_purge",
            JobPayload::MergeRun { .. } => "merge_run",
            JobPayload::PurgeSweep => "purge_sweep",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MergeRun {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub output_format_id: Uuid,
    pub input_mapping_ids: Vec<Uuid>,
    pub status: MergeStatus,
    pub output_file_id: Option<Uuid>,
    pub error_message: Option<String>,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    #[schema(value_type = String, format = DateTime, nullable = true)]
    pub started_at: Option<OffsetDateTime>,
    #[schema(value_type = String, format = DateTime, nullable = true)]
    pub finished_at: Option<OffsetDateTime>,
}