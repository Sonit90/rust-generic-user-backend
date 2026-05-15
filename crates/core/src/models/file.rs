use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    Csv,
    Xls,
    Xlsx,
}

impl FileKind {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "csv" => Some(FileKind::Csv),
            "xls" => Some(FileKind::Xls),
            "xlsx" => Some(FileKind::Xlsx),
            _ => None,
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            FileKind::Csv => "csv",
            FileKind::Xls => "xls",
            FileKind::Xlsx => "xlsx",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UploadedFile {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub original_name: String,
    pub storage_key: String,
    pub kind: FileKind,
    pub mime_type: Option<String>,
    pub size_bytes: i64,
    pub sha256: Option<String>,
    /// Column headers parsed from the first row of the file, in source order.
    pub headers: Option<Vec<String>>,
    #[schema(value_type = String, format = DateTime)]
    pub expires_at: OffsetDateTime,
    #[schema(value_type = String, format = DateTime, nullable = true)]
    pub purged_at: Option<OffsetDateTime>,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OutputFile {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub storage_key: String,
    pub kind: FileKind,
    pub size_bytes: i64,
    #[schema(value_type = String, format = DateTime)]
    pub expires_at: OffsetDateTime,
    #[schema(value_type = String, format = DateTime, nullable = true)]
    pub purged_at: Option<OffsetDateTime>,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
}