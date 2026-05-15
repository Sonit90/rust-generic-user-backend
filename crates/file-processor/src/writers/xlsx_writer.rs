use crate::value::Value;
use price_merger_core::AppError;
use rust_xlsxwriter::{Format, Workbook};

pub fn write(headers: &[String], rows: &[Vec<Value>]) -> Result<Vec<u8>, AppError> {
    let mut wb = Workbook::new();
    let sheet = wb.add_worksheet();

    let header_fmt = Format::new().set_bold();

    for (col, h) in headers.iter().enumerate() {
        sheet
            .write_string_with_format(0, col as u16, h, &header_fmt)
            .map_err(|e| AppError::FileProcessing(format!("xlsx header: {e}")))?;
    }

    for (r, row) in rows.iter().enumerate() {
        let row_idx = (r + 1) as u32;
        for (c, v) in row.iter().enumerate() {
            let col = c as u16;
            match v {
                Value::Null => { /* leave blank */ }
                Value::Bool(b) => {
                    sheet.write_boolean(row_idx, col, *b)
                        .map_err(|e| AppError::FileProcessing(format!("xlsx bool: {e}")))?;
                }
                Value::Int(i) => {
                    sheet.write_number(row_idx, col, *i as f64)
                        .map_err(|e| AppError::FileProcessing(format!("xlsx int: {e}")))?;
                }
                Value::Float(f) => {
                    sheet.write_number(row_idx, col, *f)
                        .map_err(|e| AppError::FileProcessing(format!("xlsx float: {e}")))?;
                }
                Value::Text(s) => {
                    sheet.write_string(row_idx, col, s)
                        .map_err(|e| AppError::FileProcessing(format!("xlsx text: {e}")))?;
                }
            }
        }
    }

    wb.save_to_buffer()
        .map_err(|e| AppError::FileProcessing(format!("xlsx save: {e}")))
}
