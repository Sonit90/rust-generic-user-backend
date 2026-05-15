//! Parsing, transforming, and writing price-list files.
//!
//! Architecture:
//!  * `readers` — stream rows from a source file (CSV / XLS / XLSX).
//!  * `value`   — a small `Value` enum representing a typed cell.
//!  * `transform` — apply per-column and global transforms to rows.
//!  * `merge`   — drive a merge run: read each input via its mapping, build
//!                a unified row stream, apply globals, write the output.
//!  * `writers` — produce CSV or XLSX output.

pub mod expr;
pub mod merge;
pub mod readers;
pub mod transform;
pub mod value;
pub mod writers;

pub use merge::{merge, MergeInput};
pub use value::Value;

pub struct FilePeek {
    /// Column headers from the first row, in source order.
    pub headers: Vec<String>,
    /// Up to `limit` data rows (rows after the header row).
    pub rows: Vec<Vec<Value>>,
    /// Sheet names — populated for XLS/XLSX, empty for CSV.
    pub sheet_names: Vec<String>,
}

/// Read headers and up to `limit` data rows from a file without a mapping.
/// Assumes row 1 is the header row and data starts at row 2.
/// Pass `limit = 0` to return only headers.
pub fn peek(
    bytes: &[u8],
    kind: price_merger_core::models::FileKind,
    sheet_name: Option<&str>,
    limit: usize,
) -> Result<FilePeek, price_merger_core::AppError> {
    use price_merger_core::models::FileKind;
    use readers::csv_reader::CsvReader;
    use readers::excel_reader::{ExcelReader, sheet_names_from_bytes};
    use readers::RowReader;

    match kind {
        FileKind::Csv => {
            let all = CsvReader { bytes, header_row: 1, data_start_row: 1 }.read_all()?;
            let headers = all.first()
                .map(|r| r.iter().map(|v| v.as_text()).collect())
                .unwrap_or_default();
            let rows = all.into_iter().skip(1).take(limit).collect();
            Ok(FilePeek { headers, rows, sheet_names: vec![] })
        }
        FileKind::Xls | FileKind::Xlsx => {
            let sheet_names = sheet_names_from_bytes(bytes)?;
            let target = sheet_name
                .map(|s| s.to_string())
                .or_else(|| sheet_names.first().cloned())
                .ok_or_else(|| price_merger_core::AppError::FileProcessing(
                    "workbook has no sheets".into(),
                ))?;
            let all = ExcelReader {
                bytes,
                sheet_name: Some(target),
                data_start_row: 1,
            }.read_all()?;
            let headers = all.first()
                .map(|r| r.iter().map(|v| v.as_text()).collect())
                .unwrap_or_default();
            let rows = all.into_iter().skip(1).take(limit).collect();
            Ok(FilePeek { headers, rows, sheet_names })
        }
    }
}
