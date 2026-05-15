use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use super::file::FileKind;
use super::transformation::Expr;
use crate::AppError;

/// How a datetime column value should be interpreted.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DatetimeFormat {
    #[default]
    Iso8601,
    Unix,
    Custom { pattern: String },
}

/// Expected data type for an output column.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ColumnType {
    #[default]
    String,
    Integer,
    Float,
    Boolean,
    Datetime { format: DatetimeFormat },
}

/// One column in a user-defined output format.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OutputColumn {
    /// Header text written in the output file. Also the lookup key into the
    /// canonical row produced by the input mappings.
    pub name: String,
    /// Expected data type for this column.
    #[serde(default)]
    pub col_type: ColumnType,
    /// Value used when the source field is absent.
    #[serde(default)]
    #[schema(value_type = Object, nullable = true)]
    pub default_value: Option<serde_json::Value>,
}

/// One item inside a step. Steps may freely mix filters and transforms;
/// items execute in declared order.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StepItem {
    /// Drop rows where `expr` evaluates to false/null.
    Filter {
        #[schema(value_type = Object)]
        expr: Expr,
    },
    /// Set `field` = eval(expr, row) for each row.
    Transform {
        field: String,
        #[schema(value_type = Object)]
        expr: Expr,
    },
}

/// One processing step. Steps are applied in order; items within a step
/// also apply in order. Filters and transforms may interleave.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OutputStep {
    pub items: Vec<StepItem>,
}

impl OutputStep {
    pub fn validate(&self, columns: &[OutputColumn]) -> Result<(), AppError> {
        for item in &self.items {
            match item {
                StepItem::Filter { expr } => expr.validate()?,
                StepItem::Transform { field, expr } => {
                    expr.validate()?;
                    if let Some(col) = columns.iter().find(|c| c.name == *field) {
                        validate_expr_for_column_type(expr, &col.col_type)?;
                    }
                }
            }
        }
        Ok(())
    }
}

fn validate_expr_for_column_type(expr: &Expr, col_type: &ColumnType) -> Result<(), AppError> {
    match col_type {
        ColumnType::String => forbid_arithmetic(expr),
        ColumnType::Boolean => forbid_arithmetic_and_concat(expr),
        ColumnType::Datetime { .. } => forbid_arithmetic_and_concat(expr),
        ColumnType::Integer | ColumnType::Float => forbid_concat(expr),
    }
}

fn forbid_arithmetic(expr: &Expr) -> Result<(), AppError> {
    use Expr::*;
    match expr {
        Add { .. } | Sub { .. } | Mul { .. } | Div { .. } | Round { .. } => {
            Err(AppError::Validation("arithmetic not allowed for string column".into()))
        }
        _ => walk_children(expr, forbid_arithmetic),
    }
}

fn forbid_concat(expr: &Expr) -> Result<(), AppError> {
    use Expr::*;
    match expr {
        Concat { .. } => Err(AppError::Validation("concat not allowed for numeric column".into())),
        _ => walk_children(expr, forbid_concat),
    }
}

fn forbid_arithmetic_and_concat(expr: &Expr) -> Result<(), AppError> {
    use Expr::*;
    match expr {
        Add { .. } | Sub { .. } | Mul { .. } | Div { .. } | Round { .. } => Err(
            AppError::Validation("arithmetic not allowed for boolean/datetime column".into()),
        ),
        Concat { .. } => Err(AppError::Validation(
            "concat not allowed for boolean/datetime column".into(),
        )),
        _ => walk_children(expr, forbid_arithmetic_and_concat),
    }
}

fn walk_children<F>(expr: &Expr, f: F) -> Result<(), AppError>
where
    F: Fn(&Expr) -> Result<(), AppError> + Copy,
{
    use Expr::*;
    match expr {
        Num { .. } | Str { .. } | Bool { .. } | Null {} | Var { .. } => Ok(()),
        Add { args }
        | Sub { args }
        | Mul { args }
        | Div { args }
        | Gt { args }
        | Gte { args }
        | Lt { args }
        | Lte { args }
        | Eq { args }
        | Ne { args }
        | And { args }
        | Or { args }
        | Concat { args } => args.iter().try_for_each(f),
        Not { arg } => f(arg),
        If { cond, then, else_ } => {
            f(cond)?;
            f(then)?;
            f(else_)
        }
        In { value, items } => {
            f(value)?;
            items.iter().try_for_each(f)
        }
        Round { value, decimals } => {
            f(value)?;
            f(decimals)
        }
    }
}

/// A reusable output-format definition.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OutputFormat {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub description: Option<String>,
    pub filename: Option<String>,
    pub columns: Vec<OutputColumn>,
    /// Processing steps applied in order (filters and transforms).
    pub steps: Vec<OutputStep>,
    pub output_extension: FileKind,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
}
