use crate::readers::{Row, RowReader};
use crate::value::Value;
use encoding_rs::Encoding;
use price_merger_core::AppError;

/// Sniff a delimiter from the first non-empty line. Defaults to ','.
fn sniff_delimiter(sample: &str) -> u8 {
    let line = sample.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let mut counts: [(u8, usize); 4] = [(b',', 0), (b';', 0), (b'\t', 0), (b'|', 0)];
    for c in line.chars() {
        for entry in counts.iter_mut() {
            if c as u32 == entry.0 as u32 { entry.1 += 1; }
        }
    }
    counts.iter().max_by_key(|(_, n)| *n).map(|(d, _)| *d).unwrap_or(b',')
}

pub struct CsvReader<'a> {
    pub bytes: &'a [u8],
    /// 0-based row index where headers start (informational here — the
    /// merge pipeline uses it via the mapping).
    pub header_row: usize,
    pub data_start_row: usize,
}

impl<'a> RowReader for CsvReader<'a> {
    fn read_all(self) -> Result<Vec<Row>, AppError> {
        // Decode bytes — try UTF-8 first, fall back to Windows-1251 (very
        // common in Russian price lists) if that fails.
        let (cow, _, had_errors) = encoding_rs::UTF_8.decode(self.bytes);
        let text = if had_errors {
            let (cow, _, _) = Encoding::for_label(b"windows-1251")
                .unwrap_or(encoding_rs::WINDOWS_1251)
                .decode(self.bytes);
            cow.into_owned()
        } else {
            cow.into_owned()
        };

        let delim = sniff_delimiter(&text);
        let mut rdr = csv::ReaderBuilder::new()
            .delimiter(delim)
            .has_headers(false)
            .flexible(true)
            .from_reader(text.as_bytes());

        let mut out = Vec::new();
        for (idx, rec) in rdr.records().enumerate() {
            if idx + 1 < self.data_start_row { continue; }
            let rec = rec.map_err(|e| AppError::FileProcessing(format!("csv: {e}")))?;
            out.push(rec.iter().map(|f| Value::Text(f.to_string())).collect());
        }
        Ok(out)
    }
}
