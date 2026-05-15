use serde::{Deserialize, Serialize};

/// A typed cell value. Cheap to clone via `String` ownership only when needed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

impl Value {
    pub fn is_empty(&self) -> bool {
        matches!(self, Value::Null) || matches!(self, Value::Text(s) if s.trim().is_empty())
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            Value::Text(s) => s.trim().parse::<f64>().ok(),
            _ => None,
        }
    }

    pub fn as_text(&self) -> String {
        match self {
            Value::Null => String::new(),
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => format!("{f}"),
            Value::Text(s) => s.clone(),
        }
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self { Value::Text(s.to_string()) }
}
impl From<String> for Value {
    fn from(s: String) -> Self { Value::Text(s) }
}
impl From<f64> for Value {
    fn from(f: f64) -> Self { Value::Float(f) }
}
impl From<i64> for Value {
    fn from(i: i64) -> Self { Value::Int(i) }
}
