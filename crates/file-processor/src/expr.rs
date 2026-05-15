//! Expression evaluation engine.
//!
//! The `Expr` AST is parsed from JSON once (at format-load / deserialize time)
//! and reused across all rows — there is no per-row JSON parsing.
//!
//! Safety guarantees:
//!  * Max recursion depth 20 (also enforced structurally by `Expr::validate`).
//!  * All expressions are pure (read-only access to row, no side effects).
//!  * Division by zero → null (no panics).
//!  * Type mismatches (arithmetic on text, logical on numbers) → null/false.

use price_merger_core::models::Expr;
use price_merger_core::AppError;

use crate::transform::CanonicalRow;
use crate::value::Value;

const MAX_DEPTH: usize = 20;

/// Evaluate `expr` against `row`. Returns a typed [`Value`].
pub fn eval_expr(expr: &Expr, row: &CanonicalRow) -> Result<Value, AppError> {
    eval(expr, row, 0)
}

/// Evaluate `expr` and coerce to bool.
/// Non-bool results (including null) are treated as false.
pub fn eval_filter(expr: &Expr, row: &CanonicalRow) -> Result<bool, AppError> {
    match eval_expr(expr, row)? {
        Value::Bool(b) => Ok(b),
        _ => Ok(false),
    }
}

fn eval(expr: &Expr, row: &CanonicalRow, depth: usize) -> Result<Value, AppError> {
    if depth > MAX_DEPTH {
        return Err(AppError::FileProcessing("expression max depth exceeded".into()));
    }
    use Expr::*;
    Ok(match expr {
        // ── Literals ────────────────────────────────────────────────────
        Num { value }  => Value::Float(*value),
        Str { value }  => Value::Text(value.clone()),
        Bool { value } => Value::Bool(*value),
        Null {}        => Value::Null,

        // ── Variable access ─────────────────────────────────────────────
        Var { field }  => row.get(field).cloned().unwrap_or(Value::Null),

        // ── Arithmetic ──────────────────────────────────────────────────
        Add { args } => bin_num(args, row, depth, |a, b| a + b)?,
        Sub { args } => bin_num(args, row, depth, |a, b| a - b)?,
        Mul { args } => bin_num(args, row, depth, |a, b| a * b)?,
        Div { args } => {
            let (a, b) = eval_bin(args, row, depth)?;
            match (a.as_f64(), b.as_f64()) {
                (Some(a), Some(b)) if b != 0.0 => Value::Float(a / b),
                _ => Value::Null,
            }
        }

        // ── Comparison ──────────────────────────────────────────────────
        Gt  { args } => bin_cmp(args, row, depth, |a, b| a >  b)?,
        Gte { args } => bin_cmp(args, row, depth, |a, b| a >= b)?,
        Lt  { args } => bin_cmp(args, row, depth, |a, b| a <  b)?,
        Lte { args } => bin_cmp(args, row, depth, |a, b| a <= b)?,
        Eq  { args } => {
            let (a, b) = eval_bin(args, row, depth)?;
            Value::Bool(a.as_text() == b.as_text())
        }
        Ne  { args } => {
            let (a, b) = eval_bin(args, row, depth)?;
            Value::Bool(a.as_text() != b.as_text())
        }

        // ── Logical (short-circuits) ─────────────────────────────────────
        And { args } => {
            for a in args {
                if !matches!(eval(a, row, depth + 1)?, Value::Bool(true)) {
                    return Ok(Value::Bool(false));
                }
            }
            Value::Bool(true)
        }
        Or { args } => {
            for a in args {
                if matches!(eval(a, row, depth + 1)?, Value::Bool(true)) {
                    return Ok(Value::Bool(true));
                }
            }
            Value::Bool(false)
        }
        Not { arg } => match eval(arg, row, depth + 1)? {
            Value::Bool(b) => Value::Bool(!b),
            _ => Value::Null,
        },

        // ── Control flow ─────────────────────────────────────────────────
        If { cond, then, else_ } => match eval(cond, row, depth + 1)? {
            Value::Bool(true) => eval(then, row, depth + 1)?,
            _                 => eval(else_, row, depth + 1)?,
        },

        // ── Collection / string ──────────────────────────────────────────
        In { value, items } => {
            let v = eval(value, row, depth + 1)?;
            let target_text = v.as_text();
            let mut found = false;
            for item in items {
                if eval(item, row, depth + 1)?.as_text() == target_text {
                    found = true;
                    break;
                }
            }
            Value::Bool(found)
        }

        Concat { args } => {
            let mut s = String::new();
            for a in args {
                s.push_str(&eval(a, row, depth + 1)?.as_text());
            }
            Value::Text(s)
        }

        Round { value, decimals } => {
            let v = eval(value, row, depth + 1)?;
            let d = eval(decimals, row, depth + 1)?;
            match (v.as_f64(), d.as_f64()) {
                (Some(v), Some(d)) => {
                    let mult = 10f64.powi(d as i32);
                    Value::Float((v * mult).round() / mult)
                }
                _ => Value::Null,
            }
        }
    })
}

fn eval_bin(
    args: &[Expr],
    row: &CanonicalRow,
    depth: usize,
) -> Result<(Value, Value), AppError> {
    if args.len() != 2 {
        return Err(AppError::FileProcessing(format!(
            "binary operator requires 2 args, got {}",
            args.len()
        )));
    }
    let a = eval(&args[0], row, depth + 1)?;
    let b = eval(&args[1], row, depth + 1)?;
    Ok((a, b))
}

fn bin_num(
    args: &[Expr],
    row: &CanonicalRow,
    depth: usize,
    op: impl Fn(f64, f64) -> f64,
) -> Result<Value, AppError> {
    let (a, b) = eval_bin(args, row, depth)?;
    Ok(match (a.as_f64(), b.as_f64()) {
        (Some(a), Some(b)) => Value::Float(op(a, b)),
        _ => Value::Null,
    })
}

fn bin_cmp(
    args: &[Expr],
    row: &CanonicalRow,
    depth: usize,
    op: impl Fn(f64, f64) -> bool,
) -> Result<Value, AppError> {
    let (a, b) = eval_bin(args, row, depth)?;
    Ok(match (a.as_f64(), b.as_f64()) {
        (Some(a), Some(b)) => Value::Bool(op(a, b)),
        _ => Value::Bool(false),
    })
}
