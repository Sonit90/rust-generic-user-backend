use crate::readers::{Row, RowReader};
use crate::value::Value;
use calamine::{open_workbook_auto_from_rs, Data, Reader};
use price_merger_core::AppError;
use std::io::Cursor;

pub fn sheet_names_from_bytes(bytes: &[u8]) -> Result<Vec<String>, AppError> {
    let cursor = Cursor::new(bytes);
    let wb = open_workbook_auto_from_rs(cursor)
        .map_err(|e| AppError::FileProcessing(format!("xlsx open: {e}")))?;
    Ok(wb.sheet_names().to_vec())
}

pub struct ExcelReader<'a> {
    pub bytes: &'a [u8],
    pub sheet_name: Option<String>,
    pub data_start_row: usize, // 1-based
}

impl<'a> RowReader for ExcelReader<'a> {
    fn read_all(self) -> Result<Vec<Row>, AppError> {
        let cursor = Cursor::new(self.bytes);
        let mut wb = open_workbook_auto_from_rs(cursor)
            .map_err(|e| AppError::FileProcessing(format!("xlsx open: {e}")))?;

        let sheet_name = match self.sheet_name {
            Some(n) => n,
            None => wb.sheet_names().first()
                .ok_or_else(|| AppError::FileProcessing("workbook has no sheets".into()))?
                .clone(),
        };

        let range = wb.worksheet_range(&sheet_name)
            .map_err(|e| AppError::FileProcessing(format!("read sheet: {e}")))?;

        let mut out = Vec::with_capacity(range.height());
        for (idx, row) in range.rows().enumerate() {
            if idx + 1 < self.data_start_row { continue; }
            out.push(row.iter().map(cell_to_value).collect());
        }
        Ok(out)
    }
}

fn cell_to_value(c: &Data) -> Value {
    match c {
        Data::Empty => Value::Null,
        Data::Bool(b) => Value::Bool(*b),
        Data::Int(i) => Value::Int(*i),
        Data::Float(f) => Value::Float(*f),
        Data::String(s) => Value::Text(s.clone()),
        Data::DateTime(dt) => Value::Text(dt.to_string()),
        Data::DateTimeIso(s) | Data::DurationIso(s) => Value::Text(s.clone()),
        Data::Error(e) => Value::Text(format!("#ERR:{e:?}")),
    }
}
