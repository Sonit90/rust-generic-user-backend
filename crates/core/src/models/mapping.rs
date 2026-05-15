use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use super::transformation::ColumnTransform;

/// A canonical column the system understands. Extend as needed.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
#[schema(value_type = String)]
pub struct CanonicalColumn(pub String);

impl CanonicalColumn {
    pub const SKU: &'static str = "sku";
    pub const NAME: &'static str = "name";
    pub const BRAND: &'static str = "brand";
    pub const PRICE: &'static str = "price";
    pub const QUANTITY: &'static str = "quantity";
    pub const CURRENCY: &'static str = "currency";
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DataType {
    String,
    Integer,
    Decimal,
    Boolean,
}

/// One column inside a `ColumnMapping.columns` JSON array.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MappedColumn {
    /// 0-based index in the source file.
    pub source_index: u32,
    /// Header text as seen in the file (informational).
    pub source_header: Option<String>,
    /// Canonical name; if None, the column is read but not used in the output
    /// unless an `extra` output column references it by header.
    pub canonical: Option<String>,
    pub data_type: DataType,
    #[serde(default)]
    pub transformations: Vec<ColumnTransform>,
}

/// A user's markup of an uploaded file.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ColumnMapping {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub file_id: Uuid,
    pub sheet_name: Option<String>,
    pub header_row: u32,
    pub data_start_row: u32,
    pub columns: Vec<MappedColumn>,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
}