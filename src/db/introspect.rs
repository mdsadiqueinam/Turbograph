use crate::db::error::DbError;
use crate::models::table::{Column, Table};
use std::collections::HashMap;

fn map_columns_to_table(tables: Vec<Table>, columns: Vec<Column>) -> Vec<Table> {
    let mut table_map: HashMap<u32, Table> = tables
        .into_iter()
        .map(|table| (table.oid().clone(), table))
        .collect();

    for col in columns.into_iter() {
        if let Some(table) = table_map.get_mut(col.table_oid()) {
            table.push_column(col);
        }
    }

    table_map.into_values().collect()
}

/// Introspects the PostgreSQL catalog and returns all tables and materialized
/// views in the given `schemas`, with their columns populated.
///
/// Uses `pg_catalog.pg_class` and `pg_catalog.pg_attribute` to discover
/// tables, their columns, data types, nullability, defaults, and
/// object comments.  Object comments are read from `pg_catalog.obj_description`
/// and parsed into [`Omit`](crate::models::table::Omit) values by
/// `Table::from_row` / `Column::form_row`, allowing schema generation to
/// suppress fields and mutations according to the `@omit` annotation.
pub async fn get_tables(
    pool: &deadpool_postgres::Pool,
    schemas: &[String],
) -> Result<Vec<Table>, DbError> {
    let client = pool.get().await.map_err(|e| DbError::Pool(e.to_string()))?;

    let table_rows = client
        .query(
            "SELECT
                c.oid,
                n.nspname AS schema_name,
                c.relname AS table_name,
                c.relkind::text,
                pg_catalog.obj_description(c.oid, 'pg_class') AS comment
            FROM pg_catalog.pg_class c
            JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace     -- To filter schema
            WHERE n.nspname = ANY($1)
            AND c.relkind IN ('r', 'm')
            ORDER BY n.nspname, c.relname;",
            &[&schemas],
        )
        .await
        .map_err(|e| DbError::Query(e.to_string()))?;

    let tables: Vec<Table> = table_rows
        .iter()
        .map(|r| Table::from_row(r))
        .collect::<Result<Vec<_>, _>>()?;

    let table_oids = tables.iter().map(|t| t.oid()).collect::<Vec<&u32>>();

    let column_rows = client
        .query(
            "SELECT
                a.attrelid AS table_oid,
                a.attnum::int4 AS column_id,
                a.attname AS column_name,
                a.atttypid AS type_oid,
                NOT a.attnotnull AS nullable,
                a.atthasdef AS has_default,
                pg_catalog.col_description(a.attrelid, a.attnum) AS comment
            FROM
                pg_catalog.pg_attribute a
            WHERE
                a.attrelid = ANY($1)              -- Your Table OID
                AND a.attnum > 0
                AND NOT a.attisdropped
            ORDER BY
                a.attnum;",
            &[&table_oids],
        )
        .await
        .map_err(|e| DbError::Query(e.to_string()))?;

    let columns: Vec<Column> = column_rows
        .iter()
        .map(|r| Column::form_row(r))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(map_columns_to_table(tables, columns))
}
