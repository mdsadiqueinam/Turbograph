use std::sync::Arc;

use async_graphql::Value as GqlValue;
use async_graphql::dynamic::{Field, FieldFuture, InputValue, TypeRef};
use deadpool_postgres::Pool;

use crate::db::operator::Op;
use crate::db::pool::PoolExt;
use crate::db::query::select::OrderDirection;
use crate::db::where_clause::WhereBuilder;
use crate::error::db_err_to_gql;
use crate::graphql::type_mapping::{condition_type_ref, to_sql_scalar};
use crate::models::table::Table;
use crate::models::transaction::TransactionConfig;
use crate::utils::inflection::{to_camel_case, to_pascal_case};

mod executor;
pub(crate) mod sql;

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
pub fn generate_query(table: Arc<Table>, pool: Arc<Pool>) -> Field {
    let field_name = format!("all{}", to_pascal_case(table.name()));
    let tbl_schema = table.schema_name().to_string();
    let tbl_name = table.name().to_string();
    let columns = Arc::new(table.columns().to_vec());

    Field::new(
        field_name,
        TypeRef::named_nn(table.connection_type_name()),
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
            let tx_config = ctx.data_opt::<TransactionConfig>().cloned();

            FieldFuture::new(async move {
                let table_ref = sql::quote_table(&tbl_schema, &tbl_name);
                let mut select = pool.select(&table_ref);

                if let Some(pairs) = condition_pairs {
                    sql::apply_gql_conditions(&mut select, pairs, &columns)?;
                }

                let order_pairs = sql::parse_order_by(&order_by, &columns)?;
                let safe_limit = first.unwrap_or(100).clamp(1, 1000);
                let off = offset.unwrap_or(0).max(0);

                let (total_count, json_rows) = if !order_pairs.is_empty() {
                    let first_pair = &order_pairs[0];
                    let dir = if first_pair.1 == "DESC" {
                        OrderDirection::Desc
                    } else {
                        OrderDirection::Asc
                    };
                    let mut ordered = select.order_by(&first_pair.0, dir);

                    for (col, d) in &order_pairs[1..] {
                        let direction = if *d == "DESC" {
                            OrderDirection::Desc
                        } else {
                            OrderDirection::Asc
                        };
                        ordered = ordered.order_by(col, direction);
                    }
                    ordered
                        .limit(safe_limit)
                        .offset(off)
                        .execute(tx_config)
                        .await
                        .map_err(db_err_to_gql)?
                } else {
                    select
                        .limit(safe_limit)
                        .offset(off)
                        .execute(tx_config)
                        .await
                        .map_err(db_err_to_gql)?
                };

                Ok(executor::build_connection_payload(
                    total_count,
                    json_rows,
                    &order_by,
                    off,
                ))
            })
        },
    )
    .argument(InputValue::new(
        "condition",
        TypeRef::named(table.condition_type_name()),
    ))
    .argument(InputValue::new(
        "orderBy",
        TypeRef::named_list(table.order_by_enum_name()),
    ))
    .argument(InputValue::new("first", TypeRef::named(TypeRef::INT)))
    .argument(InputValue::new("offset", TypeRef::named(TypeRef::INT)))
}

/// Generates a root Query field that fetches a single row by its primary key.
///
/// For a single-column PK (e.g. `id`), produces:
///
/// ```graphql
/// userById(id: ID!): User
/// ```
///
/// For a compound PK (e.g. `tenant_id` + `email`), produces:
///
/// ```graphql
/// userByTenantIdAndEmail(tenantId: ID!, email: String!): User
/// ```
///
/// Returns `null` if no matching row exists.
pub fn generate_query_by_id(table: Arc<Table>, pool: Arc<Pool>) -> Option<Field> {
    // Get PK columns from the column metadata (is_pk flag)
    let pk_columns: Vec<Arc<crate::models::table::Column>> = table
        .columns()
        .iter()
        .filter(|c| c.is_pk())
        .cloned()
        .collect();

    if pk_columns.is_empty() {
        return None;
    }

    // Build the field name: singularTableNameByColumn1AndColumn2...
    let type_name = to_camel_case(&table.type_name());
    let pk_part: String = pk_columns
        .iter()
        .map(|col| to_pascal_case(col.name()))
        .collect::<Vec<_>>()
        .join("And");
    let field_name = format!("{}By{}", type_name, pk_part);

    // Build the columns lookup for type mapping
    let columns: Vec<Arc<crate::models::table::Column>> = table.columns().to_vec();
    let col_by_name: std::collections::HashMap<String, usize> = columns
        .iter()
        .enumerate()
        .map(|(i, c)| (c.name().to_string(), i))
        .collect();

    let tbl_schema = table.schema_name().to_string();
    let tbl_name = table.name().to_string();

    // Build arguments for each PK column first (before capturing in closure)
    let pk_args: Vec<(String, TypeRef)> = pk_columns
        .iter()
        .filter_map(|col| {
            let type_ref = condition_type_ref(col)
                .map(|tr| {
                    // PK columns are always required (non-nullable)
                    let non_null_ref = tr.to_string().replace(">", "!>");
                    TypeRef::named_nn(non_null_ref)
                })
                .unwrap_or_else(|| TypeRef::named_nn(TypeRef::ID));
            Some((col.field_name(), type_ref))
        })
        .collect();

    // Build the field with resolver closure
    let mut field = Field::new(field_name, TypeRef::named(table.type_name()), move |ctx| {
        let pool = pool.clone();
        let tbl_schema = tbl_schema.clone();
        let tbl_name = tbl_name.clone();
        let col_by_name = col_by_name.clone();
        let columns = columns.clone();
        let pk_columns = pk_columns.clone();
        let tx_config = ctx.data_opt::<TransactionConfig>().cloned();

        FieldFuture::new(async move {
            let table_ref = sql::quote_table(&tbl_schema, &tbl_name);
            let mut select = pool.select(&table_ref);

            // Apply equality filters for each PK column
            for col in &pk_columns {
                let col_idx = *col_by_name.get(col.name()).ok_or_else(|| {
                    db_err_to_gql(crate::db::error::DbError::Query(format!(
                        "column {} not found",
                        col.name()
                    )))
                })?;
                let col = &columns[col_idx];
                let quoted = sql::quote_ident(col.name());
                let arg_val = ctx.args.get(&col.field_name()).ok_or_else(|| {
                    db_err_to_gql(crate::db::error::DbError::Query(format!(
                        "argument {} not found",
                        col.field_name()
                    )))
                })?;
                let gql_val: &async_graphql::Value = arg_val.as_value();
                if let Some(sql_scalar) = to_sql_scalar(col, gql_val) {
                    select.where_clause(&quoted, Op::Eq, Some(sql_scalar));
                }
            }

            // Execute with limit 1
            let (_, json_rows) = select
                .limit(1)
                .execute(tx_config)
                .await
                .map_err(db_err_to_gql)?;

            // Return the first row as an Option<FieldValue>
            if json_rows.is_empty() {
                Ok(None)
            } else {
                let row = json_rows.into_iter().next().expect("row exists");
                Ok(Some(async_graphql::dynamic::FieldValue::owned_any(row)))
            }
        })
    });

    // Add arguments
    for (field_name, type_ref) in pk_args {
        field = field.argument(InputValue::new(&field_name, type_ref));
    }

    Some(field)
}
