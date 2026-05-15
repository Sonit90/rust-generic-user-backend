use crate::value::Value;
use price_merger_core::AppError;

pub fn write(headers: &[String], rows: &[Vec<Value>]) -> Result<Vec<u8>, AppError> {
    let mut wtr = csv::WriterBuilder::new().from_writer(Vec::<u8>::new());
    wtr.write_record(headers)
        .map_err(|e| AppError::FileProcessing(format!("csv header: {e}")))?;
    for row in rows {
        let rec: Vec<String> = row.iter().map(|v| v.as_text()).collect();
        wtr.write_record(&rec)
            .map_err(|e| AppError::FileProcessing(format!("csv row: {e}")))?;
    }
    wtr.into_inner()
        .map_err(|e| AppError::FileProcessing(format!("csv flush: {e}")))
}
