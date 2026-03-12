use std::collections::HashMap;
use std::fmt::Write;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_graphql::Value as GqlValue;
use async_graphql::dynamic::{
    Enum, Field, FieldFuture, FieldValue, InputObject, InputValue, Object, TypeRef,
};
use deadpool_postgres::Pool;
use tokio_postgres::types::ToSql;

use crate::db::JsonListExt;
use crate::models::config::TransactionConfig;
use crate::models::table::{Column, Table};
use crate::utils::inflection::to_pascal_case;

use super::connection::{ConnectionPayload, EdgePayload, encode_cursor, make_connection_types};
use super::filter::{
    FilterOp, make_condition_filter_types, make_condition_type, make_order_by_enum, supports_range,
};
use super::sql_scalar::SqlScalar;
use super::type_mapping::to_sql_scalar;

/// Everything the schema builder needs for one table.
pub struct GeneratedQuery {
    /// The root Query field (e.g. `allUsers`).
    pub query_field: Field,
    /// The `{T}Condition` input type - must be registered with the schema.
    pub condition_type: InputObject,
    /// Per-column filter input objects referenced by `{T}Condition`.
    pub condition_filter_types: Vec<InputObject>,
    /// The `{T}OrderBy` enum - must be registered with the schema.
    pub order_by_enum: Enum,
    /// The `{T}Connection` object type - must be registered with the schema.
    pub connection_type: Object,
    /// The `{T}Edge` object type - must be registered with the schema.
    pub edge_type: Object,
}

/// Generates a root Query field (e.g. `allUsers`) with Turbograph-style
/// filtering arguments:
///
/// ```graphql
/// allUsers(
///   condition: UserCondition   # equality filter per column
///   orderBy:   [UserOrderBy]   # COLUMN_ASC / COLUMN_DESC
///   first:     Int             # LIMIT
///   offset:    Int             # OFFSET
/// ): UserConnection!
/// ```
pub fn generate_query(table: Arc<Table>, pool: Arc<Pool>) -> GeneratedQuery {
    let condition_filter_types = make_condition_filter_types(&table);
    let condition_type = make_condition_type(&table);
    let order_by_enum = make_order_by_enum(&table);
    let (connection_type, edge_type) = make_connection_types(&table);

    let connection_type_name = connection_type.type_name().to_string();
    let condition_type_name = condition_type.type_name().to_string();
    let order_by_type_name = order_by_enum.type_name().to_string();
    let field_name = format!("all{}", to_pascal_case(table.name()));
    let tbl_schema = table.schema_name().to_string();
    let tbl_name = table.name().to_string();

    let columns = Arc::new(table.columns().to_vec());
    let (mut name_map, mut upper_map) = (HashMap::new(), HashMap::new());
    for (i, col) in columns.iter().enumerate().filter(|(_, c)| !c.omit_read()) {
        name_map.insert(col.name().to_string(), i);
        upper_map.insert(col.name().to_uppercase(), i);
    }
    let col_by_name = Arc::new(name_map);
    let col_by_upper = Arc::new(upper_map);

    let query_field = Field::new(
        field_name,
        TypeRef::named_nn(connection_type_name),
        move |ctx| {
            let condition_pairs: Option<Vec<(String, GqlValue)>> = ctx
                .args
                .get("condition")
                .and_then(|v| v.object().ok())
                .map(|obj| {
                    obj.iter()
                        .map(|(k, v)| (k.to_string(), v.as_value().clone()))
                        .collect()
                });

            let order_by: Vec<String> = ctx
                .args
                .get("orderBy")
                .and_then(|v| v.list().ok())
                .map(|list| {
                    list.iter()
                        .filter_map(|item| item.enum_name().ok().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();

            let first = ctx.args.get("first").and_then(|v| v.i64().ok());
            let offset = ctx.args.get("offset").and_then(|v| v.i64().ok());

            let pool = pool.clone();
            let tbl_schema = tbl_schema.clone();
            let tbl_name = tbl_name.clone();
            let columns = columns.clone();
            let col_by_name = col_by_name.clone();
            let col_by_upper = col_by_upper.clone();
            let tx_config = ctx.data_opt::<TransactionConfig>().cloned();

            FieldFuture::new(async move {
                let mut where_clause = String::new();
                let mut params = Vec::<SqlScalar>::with_capacity(8);

                if let Some(pairs) = condition_pairs {
                    build_where_clause(
                        &mut where_clause,
                        &mut params,
                        pairs,
                        &columns,
                        &col_by_name,
                    )?;
                }

                let mut order_clause = String::new();
                build_order_by_clause(&mut order_clause, &order_by, &columns, &col_by_upper)?;

                let safe_limit = first.unwrap_or(100).clamp(1, 1000);
                let off = offset.unwrap_or(0).max(0);

                execute_connection_query(
                    &pool,
                    &tbl_schema,
                    &tbl_name,
                    &where_clause,
                    &order_clause,
                    params,
                    safe_limit,
                    off,
                    &order_by,
                    tx_config,
                )
                .await
            })
        },
    )
    .argument(InputValue::new(
        "condition",
        TypeRef::named(condition_type_name),
    ))
    .argument(InputValue::new(
        "orderBy",
        TypeRef::named_list(order_by_type_name),
    ))
    .argument(InputValue::new("first", TypeRef::named(TypeRef::INT)))
    .argument(InputValue::new("offset", TypeRef::named(TypeRef::INT)));

    GeneratedQuery {
        query_field,
        condition_type,
        condition_filter_types,
        order_by_enum,
        connection_type,
        edge_type,
    }
}

// -- SQL helpers --------------------------------------------------------------

fn build_where_clause(
    sql: &mut String,
    params: &mut Vec<SqlScalar>,
    pairs: Vec<(String, GqlValue)>,
    columns: &[Arc<Column>],
    col_by_name: &HashMap<String, usize>,
) -> Result<(), async_graphql::Error> {
    let mut has_where = false;

    for (key, gql_val) in pairs {
        let Some(&col_idx) = col_by_name.get(&key) else {
            continue;
        };
        let col = &columns[col_idx];

        if !matches!(gql_val, GqlValue::Object(_)) {
            if let Some(scalar) = to_sql_scalar(col, &gql_val) {
                write_where_sep(sql, &mut has_where);
                write!(sql, "\"{}\" = ${}", col.name(), params.len() + 1).unwrap();
                params.push(scalar);
            }
            continue;
        }

        if let GqlValue::Object(op_obj) = gql_val {
            for (op_key, op_val) in op_obj {
                let Some(op) = FilterOp::from_key(op_key.as_str()) else {
                    continue;
                };

                if op == FilterOp::In {
                    push_in_clause(sql, params, col, op_val, &mut has_where)?;
                    continue;
                }

                if op.is_range() && !supports_range(col._type()) {
                    continue;
                }

                if let Some(scalar) = to_sql_scalar(col, &op_val) {
                    write_where_sep(sql, &mut has_where);
                    write!(
                        sql,
                        "\"{}\" {} ${}",
                        col.name(),
                        op.sql_operator(),
                        params.len() + 1
                    )
                    .unwrap();
                    params.push(scalar);
                }
            }
        }
    }
    Ok(())
}

fn push_in_clause(
    sql: &mut String,
    params: &mut Vec<SqlScalar>,
    col: &Column,
    op_val: GqlValue,
    has_where: &mut bool,
) -> Result<(), async_graphql::Error> {
    if let GqlValue::List(values) = op_val {
        if values.len() > 10_000 {
            return Err(gql_err("IN filter exceeds maximum of 10,000 items"));
        }
        let scalars: Vec<SqlScalar> = values
            .into_iter()
            .filter_map(|val| to_sql_scalar(col, &val))
            .collect();

        if !scalars.is_empty() {
            write_where_sep(sql, has_where);
            let start = params.len() + 1;
            write!(sql, "\"{}\" IN (", col.name()).unwrap();
            for (i, scalar) in scalars.into_iter().enumerate() {
                if i > 0 {
                    sql.push_str(", ");
                }
                write!(sql, "${}", start + i).unwrap();
                params.push(scalar);
            }
            sql.push(')');
        }
    }
    Ok(())
}

fn build_order_by_clause(
    sql: &mut String,
    order_by: &[String],
    columns: &[Arc<Column>],
    col_by_upper: &HashMap<String, usize>,
) -> Result<(), async_graphql::Error> {
    if order_by.is_empty() {
        return Ok(());
    }
    sql.push_str(" ORDER BY ");
    for (i, s) in order_by.iter().enumerate() {
        let (col_upper, dir) = if let Some(c) = s.strip_suffix("_DESC") {
            (c, "DESC")
        } else if let Some(c) = s.strip_suffix("_ASC") {
            (c, "ASC")
        } else {
            continue;
        };
        let Some(&col_idx) = col_by_upper.get(col_upper) else {
            return Err(gql_err(format!("unknown column for ordering: {col_upper}")));
        };
        if i > 0 {
            sql.push_str(", ");
        }
        write!(sql, "\"{}\" {}", columns[col_idx].name(), dir).unwrap();
    }
    Ok(())
}

async fn with_transaction<T>(
    pool: &Pool,
    tx_config: Option<TransactionConfig>,
    callback: impl for<'c> FnOnce(
        &'c tokio_postgres::Client,
    ) -> Pin<
        Box<dyn Future<Output = Result<T, async_graphql::Error>> + Send + 'c>,
    >,
) -> Result<T, async_graphql::Error> {
    let client = pool
        .get()
        .await
        .map_err(|e| gql_err(format!("DB pool error: {e}")))?;

    // Build BEGIN with optional transaction characteristics.
    let mut begin = String::from("BEGIN");
    if let Some(ref config) = tx_config {
        if let Some(level) = config.isolation_level {
            let lvl_str = match level {
                tokio_postgres::IsolationLevel::ReadUncommitted => "READ UNCOMMITTED",
                tokio_postgres::IsolationLevel::ReadCommitted => "READ COMMITTED",
                tokio_postgres::IsolationLevel::RepeatableRead => "REPEATABLE READ",
                tokio_postgres::IsolationLevel::Serializable => "SERIALIZABLE",
                _ => "READ COMMITTED",
            };
            write!(begin, " ISOLATION LEVEL {lvl_str}").unwrap();
        }
        if config.read_only {
            begin.push_str(" READ ONLY");
        }
        if config.deferrable {
            begin.push_str(" DEFERRABLE");
        }
    }
    client
        .batch_execute(&begin)
        .await
        .map_err(|e| gql_err(format!("BEGIN error: {e}")))?;

    if let Some(ref config) = tx_config {
        config.apply(&client).await?;
    }

    let result = callback(&*client).await;

    match &result {
        Ok(_) => {
            client
                .batch_execute("COMMIT")
                .await
                .map_err(|e| gql_err(format!("COMMIT error: {e}")))?;
        }
        Err(_) => {
            let _ = client.batch_execute("ROLLBACK").await;
        }
    }

    result
}

async fn execute_connection_query(
    pool: &Pool,
    tbl_schema: &str,
    tbl_name: &str,
    where_clause: &str,
    order_clause: &str,
    params: Vec<SqlScalar>,
    limit: i64,
    offset: i64,
    order_by: &[String],
    tx_config: Option<TransactionConfig>,
) -> Result<Option<FieldValue<'static>>, async_graphql::Error> {
    let limit_param = params.len() + 1;
    let offset_param = params.len() + 2;

    let count_sql = format!("SELECT COUNT(*) FROM \"{tbl_schema}\".\"{tbl_name}\"{where_clause}");
    let data_sql = format!(
        "SELECT * FROM \"{tbl_schema}\".\"{tbl_name}\"{where_clause}{order_clause} LIMIT ${limit_param} OFFSET ${offset_param}"
    );
    let order_by = order_by.to_vec();

    with_transaction(pool, tx_config, |client| {
        Box::pin(async move {
            let base_refs: Vec<&(dyn ToSql + Sync)> =
                params.iter().map(|p| p as &(dyn ToSql + Sync)).collect();

            let data_refs: Vec<&(dyn ToSql + Sync)> = base_refs
                .iter()
                .copied()
                .chain([&limit as &(dyn ToSql + Sync), &offset as _])
                .collect();

            let (count_row, data_rows) = tokio::try_join!(
                client.query_one(&count_sql, &base_refs),
                client.query(&data_sql, &data_refs),
            )
            .map_err(|e| gql_err(format!("DB query error: {e}")))?;

            let total_count: i64 = count_row.get(0);
            let json_rows = data_rows.to_json_list();
            let edge_count = json_rows.len() as i64;

            let edges = json_rows
                .into_iter()
                .enumerate()
                .map(|(i, node)| EdgePayload {
                    cursor: encode_cursor(&order_by, (offset as usize) + i),
                    node,
                })
                .collect();

            Ok(Some(FieldValue::owned_any(ConnectionPayload {
                total_count,
                has_next_page: (offset + edge_count) < total_count,
                has_previous_page: offset > 0,
                edges,
            })))
        })
    })
    .await
}

#[inline]
fn gql_err(msg: impl std::fmt::Display) -> async_graphql::Error {
    async_graphql::Error::new(msg.to_string())
}

#[inline]
fn write_where_sep(sql: &mut String, has_where: &mut bool) {
    if *has_where {
        sql.push_str(" AND ");
    } else {
        sql.push_str(" WHERE ");
        *has_where = true;
    }
}
