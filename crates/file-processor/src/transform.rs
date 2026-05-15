//! Apply per-column transformations to source rows.

use std::collections::HashMap;

use price_merger_core::models::{ColumnTransform, MappedColumn};
use price_merger_core::AppError;
use regex::Regex;

use crate::value::Value;

/// One canonical-keyed row produced from a source row + its mapping.
pub type CanonicalRow = HashMap<String, Value>;

/// Apply per-column transforms while reading a source row.
pub fn project_row(
    source_row: &[Value],
    columns: &[MappedColumn],
) -> Result<CanonicalRow, AppError> {
    let mut out = CanonicalRow::new();
    for spec in columns {
        let raw = source_row.get(spec.source_index as usize).cloned().unwrap_or(Value::Null);
        let v = apply_column_transforms(raw, &spec.transformations)?;
        if let Some(canon) = &spec.canonical {
            out.insert(canon.clone(), v);
        }
        // Columns without a canonical name are dropped from the output unless
        // an output column references them — that case is handled in `merge`.
    }
    Ok(out)
}

pub fn apply_column_transforms(
    mut v: Value,
    xs: &[ColumnTransform],
) -> Result<Value, AppError> {
    for x in xs {
        v = apply_one(v, x)?;
    }
    Ok(v)
}

fn apply_one(v: Value, x: &ColumnTransform) -> Result<Value, AppError> {
    use ColumnTransform::*;
    Ok(match x {
        Trim => Value::Text(v.as_text().trim().to_string()),
        LowerCase => Value::Text(v.as_text().to_lowercase()),
        UpperCase => Value::Text(v.as_text().to_uppercase()),
        Replace { from, to } => Value::Text(v.as_text().replace(from, to)),
        RegexReplace { pattern, replacement } => {
            let re = Regex::new(pattern)
                .map_err(|e| AppError::Validation(format!("regex: {e}")))?;
            Value::Text(re.replace_all(&v.as_text(), replacement.as_str()).into_owned())
        }
        ParseDecimal { decimal_separator, thousand_separator } => {
            let mut s = v.as_text();
            if let Some(t) = thousand_separator { s = s.replace(*t, ""); }
            if *decimal_separator != '.' { s = s.replace(*decimal_separator, "."); }
            s = s.replace(' ', "").replace('\u{00A0}', "");
            s.trim().parse::<f64>().map(Value::Float).unwrap_or(Value::Null)
        }
        MultiplyBy { factor } => match v.as_f64() {
            Some(n) => Value::Float(n * factor),
            None => v,
        },
        AddConstant { value } => match v.as_f64() {
            Some(n) => Value::Float(n + value),
            None => v,
        },
        IncreasePercent { percent } => match v.as_f64() {
            Some(n) => Value::Float(n * (1.0 + percent / 100.0)),
            None => v,
        },
        ToInt => match v.as_f64() {
            Some(n) => Value::Int(n as i64),
            None => v,
        },
        Constant { value } => Value::Text(value.clone()),
    })
}
