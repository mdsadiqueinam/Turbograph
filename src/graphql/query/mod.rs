use std::sync::Arc;

use async_graphql::Value as GqlValue;
use async_graphql::dynamic::{Field, FieldFuture, InputValue, TypeRef};
use deadpool_postgres::Pool;

use crate::db::pool::PoolExt;
use crate::db::query::select::OrderDirection;
use crate::error::db_err_to_gql;
use crate::models::table::Table;
use crate::models::transaction::TransactionConfig;
use crate::utils::inflection::to_pascal_case;

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
