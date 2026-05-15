use price_merger_core::models::{ColumnMapping, MappedColumn};
use sqlx::PgPool;
use uuid::Uuid;

use crate::DbResult;

#[derive(Debug, Clone)]
pub struct NewColumnMapping<'a> {
    pub owner_id: Uuid,
    pub file_id: Uuid,
    pub sheet_name: Option<&'a str>,
    pub header_row: i32,
    pub data_start_row: i32,
    pub columns: &'a [MappedColumn],
}

pub async fn insert(pool: &PgPool, n: NewColumnMapping<'_>) -> DbResult<ColumnMapping> {
    let columns_json = serde_json::to_value(n.columns)
        .expect("MappedColumn is serializable");
    let row = sqlx::query!(
        r#"
        INSERT INTO column_mappings (owner_id, file_id, sheet_name,
                                     header_row, data_start_row, columns)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, owner_id, file_id, sheet_name, header_row, data_start_row,
                  columns, created_at, updated_at
        "#,
        n.owner_id, n.file_id, n.sheet_name,
        n.header_row, n.data_start_row, columns_json,
    ).fetch_one(pool).await?;

    Ok(ColumnMapping {
        id: row.id,
        owner_id: row.owner_id,
        file_id: row.file_id,
        sheet_name: row.sheet_name,
        header_row: row.header_row as u32,
        data_start_row: row.data_start_row as u32,
        columns: serde_json::from_value(row.columns).unwrap_or_default(),
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

pub async fn get(pool: &PgPool, id: Uuid) -> DbResult<Option<ColumnMapping>> {
    let row = sqlx::query!(
        "SELECT id, owner_id, file_id, sheet_name, header_row, data_start_row,
                columns, created_at, updated_at FROM column_mappings WHERE id = $1",
        id,
    ).fetch_optional(pool).await?;
    Ok(row.map(|r| ColumnMapping {
        id: r.id, owner_id: r.owner_id, file_id: r.file_id,
        sheet_name: r.sheet_name,
        header_row: r.header_row as u32,
        data_start_row: r.data_start_row as u32,
        columns: serde_json::from_value(r.columns).unwrap_or_default(),
        created_at: r.created_at, updated_at: r.updated_at,
    }))
}

pub async fn list_by_owner(pool: &PgPool, owner_id: Uuid) -> DbResult<Vec<ColumnMapping>> {
    let rows = sqlx::query!(
        "SELECT id, owner_id, file_id, sheet_name, header_row, data_start_row,
                columns, created_at, updated_at
         FROM column_mappings WHERE owner_id = $1 ORDER BY created_at DESC",
        owner_id,
    ).fetch_all(pool).await?;
    Ok(rows.into_iter().map(|r| ColumnMapping {
        id: r.id, owner_id: r.owner_id, file_id: r.file_id,
        sheet_name: r.sheet_name,
        header_row: r.header_row as u32,
        data_start_row: r.data_start_row as u32,
        columns: serde_json::from_value(r.columns).unwrap_or_default(),
        created_at: r.created_at, updated_at: r.updated_at,
    }).collect())
}

pub async fn delete(pool: &PgPool, id: Uuid) -> DbResult<()> {
    sqlx::query!("DELETE FROM column_mappings WHERE id = $1", id)
        .execute(pool).await?;
    Ok(())
}
