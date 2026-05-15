use price_merger_core::models::{FileKind, OutputColumn, OutputFormat, OutputStep};
use sqlx::PgPool;
use uuid::Uuid;

use crate::types::DbFileKind;
use crate::DbResult;

macro_rules! into_format {
    ($r:expr) => {
        OutputFormat {
            id:               $r.id,
            owner_id:         $r.owner_id,
            description:      $r.description,
            filename:         $r.filename,
            columns:          serde_json::from_value($r.columns).unwrap_or_default(),
            steps:            serde_json::from_value($r.steps).unwrap_or_default(),
            output_extension: $r.output_extension.into(),
            created_at:       $r.created_at,
            updated_at:       $r.updated_at,
        }
    };
}

#[derive(Debug, Clone)]
pub struct NewOutputFormat<'a> {
    pub owner_id: Uuid,
    pub description: Option<&'a str>,
    pub filename: Option<&'a str>,
    pub columns: &'a [OutputColumn],
    pub steps: &'a [OutputStep],
    pub output_extension: FileKind,
}

pub async fn insert(pool: &PgPool, n: NewOutputFormat<'_>) -> DbResult<OutputFormat> {
    let cols  = serde_json::to_value(n.columns).expect("serializable");
    let steps = serde_json::to_value(n.steps).expect("serializable");
    let kind  = DbFileKind::from(n.output_extension);

    let row = sqlx::query!(
        r#"
        INSERT INTO output_formats
            (owner_id, description, filename, columns, steps, output_extension)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, owner_id, description, filename, columns, steps,
                  output_extension AS "output_extension!: DbFileKind",
                  created_at, updated_at
        "#,
        n.owner_id, n.description, n.filename,
        cols, steps, kind as DbFileKind,
    ).fetch_one(pool).await?;

    Ok(into_format!(row))
}

pub async fn get(pool: &PgPool, id: Uuid) -> DbResult<Option<OutputFormat>> {
    let row = sqlx::query!(
        r#"
        SELECT id, owner_id, description, filename, columns, steps,
               output_extension AS "output_extension!: DbFileKind",
               created_at, updated_at
        FROM output_formats WHERE id = $1
        "#,
        id,
    ).fetch_optional(pool).await?;

    Ok(row.map(|r| into_format!(r)))
}

pub async fn list_by_owner(pool: &PgPool, owner_id: Uuid) -> DbResult<Vec<OutputFormat>> {
    let rows = sqlx::query!(
        r#"
        SELECT id, owner_id, description, filename, columns, steps,
               output_extension AS "output_extension!: DbFileKind",
               created_at, updated_at
        FROM output_formats WHERE owner_id = $1 ORDER BY created_at
        "#,
        owner_id,
    ).fetch_all(pool).await?;

    Ok(rows.into_iter().map(|r| into_format!(r)).collect())
}

pub async fn delete(pool: &PgPool, id: Uuid) -> DbResult<()> {
    sqlx::query!("DELETE FROM output_formats WHERE id = $1", id)
        .execute(pool).await?;
    Ok(())
}

#[derive(Debug, Clone, Default)]
pub struct UpdateOutputFormat<'a> {
    pub set_description: bool,
    pub description: Option<&'a str>,
    pub set_filename: bool,
    pub filename: Option<&'a str>,
    pub columns: Option<&'a [OutputColumn]>,
    pub steps: Option<&'a [OutputStep]>,
    pub output_extension: Option<FileKind>,
}

pub async fn update(
    pool: &PgPool,
    id: Uuid,
    u: UpdateOutputFormat<'_>,
) -> DbResult<Option<OutputFormat>> {
    let cols  = u.columns.map(|c| serde_json::to_value(c).expect("serializable"));
    let steps = u.steps.map(|s| serde_json::to_value(s).expect("serializable"));
    let kind  = u.output_extension.map(DbFileKind::from);

    let row = sqlx::query!(
        r#"
        UPDATE output_formats SET
            description      = CASE WHEN $2 THEN $3 ELSE description END,
            filename         = CASE WHEN $4 THEN $5 ELSE filename END,
            columns          = COALESCE($6, columns),
            steps            = COALESCE($7, steps),
            output_extension = COALESCE($8::file_kind, output_extension),
            updated_at       = now()
        WHERE id = $1
        RETURNING id, owner_id, description, filename, columns, steps,
                  output_extension AS "output_extension!: DbFileKind",
                  created_at, updated_at
        "#,
        id,
        u.set_description, u.description,
        u.set_filename, u.filename,
        cols, steps,
        kind as Option<DbFileKind>,
    ).fetch_optional(pool).await?;

    Ok(row.map(|r| into_format!(r)))
}
