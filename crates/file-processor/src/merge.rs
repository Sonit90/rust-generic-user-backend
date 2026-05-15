//! Drive a merge run: read each input file via its mapping, normalize to
//! canonical rows, apply ordered processing steps, and write the chosen output.
//!
//! A step holds an ordered list of items; each item is either a filter or a
//! transform. Filters and transforms may freely interleave within one step.
//!
//! Pipeline per run:
//!   for each input file:
//!     1. Read raw rows
//!     2. Project to canonical row (column mapping + per-column transforms)
//!   then across the merged dataset:
//!     3. Pre-fill any output column with a `default_value` when the row is
//!        missing that key, so transforms can target it
//!     4. Walk steps in order; within each step walk items in order.
//!        Filter items drop rows where the expression is false/null;
//!        transform items mutate each row.
//!     5. Project to output column order and write

use price_merger_core::models::{ColumnMapping, FileKind, OutputFormat, StepItem};
use price_merger_core::AppError;

use crate::expr::{eval_expr, eval_filter};
use crate::readers::csv_reader::CsvReader;
use crate::readers::excel_reader::ExcelReader;
use crate::readers::RowReader;
use crate::transform::{project_row, CanonicalRow};
use crate::value::Value;
use crate::writers;

/// One input to a merge: the file bytes plus the mapping describing them.
pub struct MergeInput<'a> {
    pub bytes: &'a [u8],
    pub kind: FileKind,
    pub mapping: &'a ColumnMapping,
}

/// Run a merge. Returns the produced output bytes.
pub fn merge(inputs: &[MergeInput<'_>], format: &OutputFormat) -> Result<Vec<u8>, AppError> {
    let mut all_rows: Vec<CanonicalRow> = Vec::new();

    for input in inputs {
        let rows = match input.kind {
            FileKind::Csv => CsvReader {
                bytes: input.bytes,
                header_row: input.mapping.header_row as usize,
                data_start_row: input.mapping.data_start_row as usize,
            }.read_all()?,
            FileKind::Xls | FileKind::Xlsx => ExcelReader {
                bytes: input.bytes,
                sheet_name: input.mapping.sheet_name.clone(),
                data_start_row: input.mapping.data_start_row as usize,
            }.read_all()?,
        };

        for src in rows {
            if src.iter().all(|v| v.is_empty()) { continue; }
            all_rows.push(project_row(&src, &input.mapping.columns)?);
        }
    }

    // Pre-fill any output column that has a default_value when the row is
    // missing that key, so expression transforms can target it.
    for r in all_rows.iter_mut() {
        for col in &format.columns {
            if let Some(default) = &col.default_value {
                if !r.contains_key(&col.name) {
                    r.insert(col.name.clone(), json_to_value(default.clone()));
                }
            }
        }
    }

    // Walk steps in order; within each step walk items in order. Filter
    // items drop rows; transform items mutate each row.
    for step in &format.steps {
        for item in &step.items {
            match item {
                StepItem::Filter { expr } => {
                    let mut kept = Vec::with_capacity(all_rows.len());
                    for row in all_rows.drain(..) {
                        if eval_filter(expr, &row)? { kept.push(row); }
                    }
                    all_rows = kept;
                }
                StepItem::Transform { field, expr } => {
                    for row in all_rows.iter_mut() {
                        let v = eval_expr(expr, row)?;
                        row.insert(field.clone(), v);
                    }
                }
            }
        }
    }

    // Project rows to the output column order.
    let headers: Vec<String> = format.columns.iter().map(|c| c.name.clone()).collect();
    let table: Vec<Vec<Value>> = all_rows
        .into_iter()
        .map(|row| {
            format.columns.iter().map(|c| {
                row.get(&c.name).cloned().unwrap_or(Value::Null)
            }).collect()
        })
        .collect();

    match format.output_extension {
        FileKind::Csv | FileKind::Xls => writers::csv_writer::write(&headers, &table),
        FileKind::Xlsx => writers::xlsx_writer::write(&headers, &table),
    }
}

fn json_to_value(j: serde_json::Value) -> Value {
    match j {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() { Value::Int(i) }
            else if let Some(f) = n.as_f64() { Value::Float(f) }
            else { Value::Null }
        }
        serde_json::Value::String(s) => Value::Text(s),
        other => Value::Text(other.to_string()),
    }
}
