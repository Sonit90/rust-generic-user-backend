//! Rust mirrors of Postgres custom enums, used by sqlx::query! macros.

use price_merger_core::models::{FileKind, JobStatus, MergeStatus};

#[derive(sqlx::Type, Debug, Clone, Copy, PartialEq, Eq)]
#[sqlx(type_name = "file_kind", rename_all = "lowercase")]
pub enum DbFileKind { Csv, Xls, Xlsx }

impl From<FileKind> for DbFileKind {
    fn from(k: FileKind) -> Self {
        match k { FileKind::Csv => Self::Csv, FileKind::Xls => Self::Xls, FileKind::Xlsx => Self::Xlsx }
    }
}
impl From<DbFileKind> for FileKind {
    fn from(k: DbFileKind) -> Self {
        match k { DbFileKind::Csv => Self::Csv, DbFileKind::Xls => Self::Xls, DbFileKind::Xlsx => Self::Xlsx }
    }
}

#[derive(sqlx::Type, Debug, Clone, Copy, PartialEq, Eq)]
#[sqlx(type_name = "job_status", rename_all = "lowercase")]
pub enum DbJobStatus { Queued, Running, Completed, Failed, Dead }

impl From<DbJobStatus> for JobStatus {
    fn from(k: DbJobStatus) -> Self {
        match k {
            DbJobStatus::Queued => Self::Queued,
            DbJobStatus::Running => Self::Running,
            DbJobStatus::Completed => Self::Completed,
            DbJobStatus::Failed => Self::Failed,
            DbJobStatus::Dead => Self::Dead,
        }
    }
}

#[derive(sqlx::Type, Debug, Clone, Copy, PartialEq, Eq)]
#[sqlx(type_name = "merge_status", rename_all = "lowercase")]
pub enum DbMergeStatus { Queued, Running, Completed, Failed, Cancelled }

impl From<DbMergeStatus> for MergeStatus {
    fn from(k: DbMergeStatus) -> Self {
        match k {
            DbMergeStatus::Queued => Self::Queued,
            DbMergeStatus::Running => Self::Running,
            DbMergeStatus::Completed => Self::Completed,
            DbMergeStatus::Failed => Self::Failed,
            DbMergeStatus::Cancelled => Self::Cancelled,
        }
    }
}
