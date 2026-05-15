//! Transformation primitives.
//!
//! Two scopes:
//!  * **Per-column** — applied while reading a source file (e.g. parse a
//!    European-decimal price, trim whitespace, uppercase SKUs). Unchanged.
//!  * **Expression** — JSON AST evaluated against canonical rows for
//!    file-level filtering, global filtering, and column value transforms.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::AppError;

/// Transforms applied per-column while reading a source file.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ColumnTransform {
    /// Trim whitespace.
    Trim,
    /// Lower / upper case.
    LowerCase,
    UpperCase,
    /// Replace `from` with `to`. Plain string match.
    Replace { from: String, to: String },
    /// Regex replace.
    RegexReplace { pattern: String, replacement: String },
    /// Parse "1 234,56" / "1,234.56" / "1234.56" -> f64.
    ParseDecimal {
        #[schema(value_type = String)]
        decimal_separator: char,
        #[schema(value_type = String, nullable = true)]
        thousand_separator: Option<char>,
    },
    /// Multiply numeric value by a constant.
    MultiplyBy { factor: f64 },
    /// Add a constant.
    AddConstant { value: f64 },
    /// Increase by percent: x * (1 + p/100).
    IncreasePercent { percent: f64 },
    /// Coerce to int (truncate).
    ToInt,
    /// Coerce to a fixed string.
    Constant { value: String },
}

/// A single node in the expression AST.
///
/// Serialized as a tagged JSON object with `"op"` as the discriminator:
/// ```json
/// {"op": "+", "args": [{"op": "var", "field": "price"}, {"op": "num", "value": 10}]}
/// ```
///
/// Operators and their shape:
///
/// | `op`        | Shape |
/// |-------------|-------|
/// | `num`       | `{"op":"num","value":f64}` |
/// | `str`       | `{"op":"str","value":string}` |
/// | `bool`      | `{"op":"bool","value":bool}` |
/// | `null`      | `{"op":"null"}` |
/// | `var`       | `{"op":"var","field":string}` |
/// | `+` `-` `*` `/` | `{"op":"…","args":[expr,expr]}` |
/// | `>` `>=` `<` `<=` `==` `!=` | `{"op":"…","args":[expr,expr]}` |
/// | `and` `or`  | `{"op":"…","args":[expr,expr,…]}` (≥2) |
/// | `!`         | `{"op":"!","arg":expr}` |
/// | `if`        | `{"op":"if","cond":expr,"then":expr,"else":expr}` |
/// | `in`        | `{"op":"in","value":expr,"items":[expr,…]}` |
/// | `concat`    | `{"op":"concat","args":[expr,…]}` |
/// | `round`     | `{"op":"round","value":expr,"decimals":expr}` |
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum Expr {
    // ── Literals ────────────────────────────────────────────────────────────
    #[serde(rename = "num")]  Num  { value: f64 },
    #[serde(rename = "str")]  Str  { value: String },
    #[serde(rename = "bool")] Bool { value: bool },
    #[serde(rename = "null")] Null {},

    // ── Variable access ─────────────────────────────────────────────────────
    /// Reads a named field from the current canonical row.
    /// Missing fields evaluate to null.
    #[serde(rename = "var")] Var { field: String },

    // ── Arithmetic (exactly 2 args; non-numeric inputs → null) ───────────
    #[serde(rename = "+")] Add { args: Vec<Expr> },
    #[serde(rename = "-")] Sub { args: Vec<Expr> },
    #[serde(rename = "*")] Mul { args: Vec<Expr> },
    #[serde(rename = "/")] Div { args: Vec<Expr> },

    // ── Comparison (exactly 2 args) ──────────────────────────────────────
    #[serde(rename = ">")]  Gt  { args: Vec<Expr> },
    #[serde(rename = ">=")] Gte { args: Vec<Expr> },
    #[serde(rename = "<")]  Lt  { args: Vec<Expr> },
    #[serde(rename = "<=")] Lte { args: Vec<Expr> },
    /// Text comparison (coerces both sides to string).
    #[serde(rename = "==")] Eq  { args: Vec<Expr> },
    #[serde(rename = "!=")] Ne  { args: Vec<Expr> },

    // ── Logical (short-circuits) ─────────────────────────────────────────
    /// `and` requires ≥ 2 args; returns true only if all args are true.
    #[serde(rename = "and")] And { args: Vec<Expr> },
    /// `or` requires ≥ 2 args; returns true if any arg is true.
    #[serde(rename = "or")]  Or  { args: Vec<Expr> },
    #[serde(rename = "!")]   Not { arg: Box<Expr> },

    // ── Control flow ─────────────────────────────────────────────────────
    #[serde(rename = "if")]
    If {
        cond:  Box<Expr>,
        then:  Box<Expr>,
        #[serde(rename = "else")]
        else_: Box<Expr>,
    },

    // ── Collection / string ──────────────────────────────────────────────
    /// True when `value` (as text) matches any item (as text) in `items`.
    #[serde(rename = "in")]     In     { value: Box<Expr>, items: Vec<Expr> },
    #[serde(rename = "concat")] Concat { args: Vec<Expr> },
    /// Round `value` to `decimals` decimal places.
    #[serde(rename = "round")]  Round  { value: Box<Expr>, decimals: Box<Expr> },
}

impl Expr {
    /// Structural validation: depth ≤ 20, total node count ≤ 100,
    /// correct argument counts per operator.
    ///
    /// Called at format-create time so invalid expressions are rejected before
    /// they reach the evaluation engine.
    pub fn validate(&self) -> Result<(), AppError> {
        Self::check(self, 0, &mut 0)
    }

    fn check(expr: &Expr, depth: usize, count: &mut usize) -> Result<(), AppError> {
        const MAX_DEPTH: usize = 20;
        const MAX_NODES: usize = 100;

        if depth > MAX_DEPTH {
            return Err(AppError::Validation("expression exceeds max depth (20)".into()));
        }
        *count += 1;
        if *count > MAX_NODES {
            return Err(AppError::Validation("expression exceeds max size (100 nodes)".into()));
        }

        use Expr::*;
        match expr {
            Num { .. } | Str { .. } | Bool { .. } | Null {} | Var { .. } => {}

            Add { args } | Sub { args } | Mul { args } | Div { args }
            | Gt { args } | Gte { args } | Lt { args } | Lte { args }
            | Eq { args } | Ne { args } => {
                if args.len() != 2 {
                    return Err(AppError::Validation(format!(
                        "binary operator requires exactly 2 args, got {}",
                        args.len()
                    )));
                }
                for a in args { Self::check(a, depth + 1, count)?; }
            }

            And { args } | Or { args } => {
                if args.len() < 2 {
                    return Err(AppError::Validation(
                        "logical 'and'/'or' requires at least 2 args".into(),
                    ));
                }
                for a in args { Self::check(a, depth + 1, count)?; }
            }

            Not { arg } => Self::check(arg, depth + 1, count)?,

            If { cond, then, else_ } => {
                Self::check(cond, depth + 1, count)?;
                Self::check(then, depth + 1, count)?;
                Self::check(else_, depth + 1, count)?;
            }

            In { value, items } => {
                if items.is_empty() {
                    return Err(AppError::Validation(
                        "'in' requires at least 1 item".into(),
                    ));
                }
                Self::check(value, depth + 1, count)?;
                for item in items { Self::check(item, depth + 1, count)?; }
            }

            Concat { args } => {
                if args.is_empty() {
                    return Err(AppError::Validation("'concat' requires at least 1 arg".into()));
                }
                for a in args { Self::check(a, depth + 1, count)?; }
            }

            Round { value, decimals } => {
                Self::check(value, depth + 1, count)?;
                Self::check(decimals, depth + 1, count)?;
            }
        }
        Ok(())
    }
}

/// A column transform using the expression engine.
/// Sets `field` = `eval(expr, current_row)` for each row.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExprTransform {
    /// Target field name in the canonical row.
    pub field: String,
    /// Expression whose result is assigned to `field`.
    #[schema(value_type = Object)]
    pub expr: Expr,
}
