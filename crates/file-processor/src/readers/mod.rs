pub mod csv_reader;
pub mod excel_reader;

use crate::value::Value;
use price_merger_core::AppError;

/// A single row of the source file as parsed cells, in source-column order.
pub type Row = Vec<Value>;

/// All readers expose the same interface: an iterator of rows.
/// Implementations buffer the entire sheet into memory for now (simplifies
/// xls/xlsx; revisit if you need to handle multi-GB files).
pub trait RowReader {
    fn read_all(self) -> Result<Vec<Row>, AppError>;
}
