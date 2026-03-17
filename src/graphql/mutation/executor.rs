use std::collections::HashMap;
use std::sync::Arc;

use async_graphql::Value as GqlValue;
use async_graphql::dynamic::FieldValue;
use deadpool_postgres::Pool;

use crate::db::error::DbError;
use crate::db::pool::PoolExt;
use crate::db::query::delete::Delete;
use crate::db::query::insert::Insert;
use crate::db::query::update::Update;
use crate::db::{JsonExt, JsonListExt};
use crate::models::table::Column;
use crate::models::transaction::TransactionConfig;

use super::super::query::sql::{apply_gql_conditions, quote_ident, quote_table};
use super::super::type_mapping::to_sql_scalar;

fn db_err_to_gql(err: DbError) -> async_graphql::Error {
    async_graphql::Error::new(err.to_string())
}

/// INSERT … RETURNING *  →  single entity (or null if no columns provided).
pub(super) async fn execute_create(
    pool: &Pool,
    tbl_schema: &str,
    tbl_name: &str,
    input: Vec<(String, GqlValue)>,
    columns: &[Arc<Column>],
    col_map: &HashMap<String, usize>,
    tx_config: Option<TransactionConfig>,
) -> Result<Option<FieldValue<'static>>, async_graphql::Error> {
    let table_ref = quote_table(tbl_schema, tbl_name);
    let mut insert = pool.insert(&table_ref);
    insert.returning_all();

    let mut row = HashMap::new();
    for (key, val) in &input {
        let Some(&idx) = col_map.get(key) else {
            continue;
        };
        let col = &columns[idx];
        if let Some(scalar) = to_sql_scalar(col, val) {
            row.insert(quote_ident(col.name()), Some(scalar));
        }
    }

    if row.is_empty() {
        return Err(async_graphql::Error::new("No valid columns provided for insert"));
    }

    insert.values(row);

    let rows = insert
        .execute_with_returning(tx_config)
        .await
        .map_err(db_err_to_gql)?;

    let row = rows
        .into_iter()
        .next()
        .map(|r| FieldValue::owned_any(r.to_json()));

    Ok(row)
}

/// UPDATE … SET … WHERE … RETURNING *  →  list of updated entities.
pub(super) async fn execute_update(
    pool: &Pool,
    tbl_schema: &str,
    tbl_name: &str,
    patch: Vec<(String, GqlValue)>,
    condition: Option<Vec<(String, GqlValue)>>,
    columns: &[Arc<Column>],
    update_col_map: &HashMap<String, usize>,
    tx_config: Option<TransactionConfig>,
) -> Result<Option<FieldValue<'static>>, async_graphql::Error> {
    let table_ref = quote_table(tbl_schema, tbl_name);
    let mut update = pool.update(&table_ref);
    update.returning_all();

    let mut has_set = false;
    for (key, val) in &patch {
        let Some(&idx) = update_col_map.get(key) else {
            continue;
        };
        let col = &columns[idx];
        let quoted = quote_ident(col.name());
        if matches!(val, GqlValue::Null) {
            update.set(&quoted, None);
            has_set = true;
        } else if let Some(scalar) = to_sql_scalar(col, val) {
            update.set(&quoted, Some(scalar));
            has_set = true;
        }
    }

    if !has_set {
        return Err(async_graphql::Error::new("No valid columns provided for update"));
    }

    if let Some(pairs) = condition {
        apply_gql_conditions(&mut update, pairs, columns)?;
    }

    let rows = update
        .execute_with_returning(tx_config)
        .await
        .map_err(db_err_to_gql)?;

    let list: Vec<FieldValue> = rows
        .to_json_list()
        .into_iter()
        .map(FieldValue::owned_any)
        .collect();

    Ok(Some(FieldValue::list(list)))
}

/// DELETE … WHERE … RETURNING *  →  list of deleted entities.
pub(super) async fn execute_delete(
    pool: &Pool,
    tbl_schema: &str,
    tbl_name: &str,
    condition: Option<Vec<(String, GqlValue)>>,
    columns: &[Arc<Column>],
    tx_config: Option<TransactionConfig>,
) -> Result<Option<FieldValue<'static>>, async_graphql::Error> {
    let table_ref = quote_table(tbl_schema, tbl_name);
    let mut delete = pool.delete(&table_ref);
    delete.returning_all();

    if let Some(pairs) = condition {
        apply_gql_conditions(&mut delete, pairs, columns)?;
    }

    let rows = delete
        .execute_with_returning(tx_config)
        .await
        .map_err(db_err_to_gql)?;

    let list: Vec<FieldValue> = rows
        .to_json_list()
        .into_iter()
        .map(FieldValue::owned_any)
        .collect();

    Ok(Some(FieldValue::list(list)))
}
