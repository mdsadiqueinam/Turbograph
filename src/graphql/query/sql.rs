use std::collections::HashMap;
use std::sync::Arc;

use async_graphql::Value as GqlValue;
use tokio_postgres::types::Type;

use crate::db::operator::Op;
use crate::db::scalar::SqlScalar;
use crate::db::where_clause::WhereBuilder;
use crate::models::table::Column;
use crate::utils::inflection::to_screaming_snake_case;

use super::super::filter::supports_range;
use super::super::type_mapping::{scalars_to_sql_array, to_sql_scalar};

use crate::error::gql_err;

/// Wraps a column name in double quotes for PostgreSQL identifier quoting.
#[inline]
pub(crate) fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name)
}

/// Builds a schema-qualified table reference: `"schema"."table"`.
#[inline]
pub(crate) fn quote_table(schema: &str, table: &str) -> String {
    format!("\"{}\".\"{}\"", schema, table)
}

/// Applies GraphQL condition pairs to any query builder that implements
/// [`WhereBuilder`].  Handles simple equality, operator objects
/// (`equal`, `greaterThan`, …) and `in` lists.
pub(crate) fn apply_gql_conditions<T: WhereBuilder>(
    builder: &mut T,
    pairs: Vec<(String, GqlValue)>,
    columns: &[Arc<Column>],
) -> Result<(), async_graphql::Error> {
    let col_by_name: HashMap<String, usize> = columns
        .iter()
        .enumerate()
        .map(|(i, c)| (c.name().to_string(), i))
        .collect();

    for (key, gql_val) in pairs {
        let Some(&col_idx) = col_by_name.get(&key) else {
            continue;
        };
        let col = &columns[col_idx];
        let quoted = quote_ident(col.name());

        if !matches!(gql_val, GqlValue::Object(_)) {
            if let Some(scalar) = to_sql_scalar(col, &gql_val) {
                builder.where_clause(&quoted, Op::Eq, Some(scalar));
            }
            continue;
        }

        if let GqlValue::Object(op_obj) = gql_val {
            for (op_key, op_val) in op_obj {
                let Some(filter_op) = Op::from_key(op_key.as_str()) else {
                    continue;
                };

                if filter_op == Op::In {
                    if let GqlValue::List(values) = op_val {
                        if values.len() > 10_000 {
                            return Err(gql_err("IN filter exceeds maximum of 10,000 items"));
                        }
                        let scalars: Vec<SqlScalar> = values
                            .into_iter()
                            .filter_map(|val| to_sql_scalar(col, &val))
                            .collect();
                        if let Some(sql_array) = scalars_to_sql_array(col._type(), scalars) {
                            builder.where_in(&quoted, sql_array);
                        }
                    }
                    continue;
                }

                if filter_op.is_range() && !supports_range(col._type()) {
                    continue;
                }

                let op = filter_op;

                if let Some(scalar) = to_sql_scalar(col, &op_val) {
                    builder.where_clause(&quoted, op, Some(scalar));
                }
            }
        }
    }
    Ok(())
}

/// Parses a list of GraphQL `orderBy` enum values (e.g. `["NAME_ASC", "ID_DESC"]`)
/// and returns a list of `(quoted_column, direction)` pairs.
pub(super) fn parse_order_by(
    order_by: &[String],
    columns: &[Arc<Column>],
) -> Result<Vec<(String, &'static str)>, async_graphql::Error> {
    if order_by.is_empty() {
        return Ok(Vec::new());
    }

    let col_by_upper: HashMap<String, usize> = columns
        .iter()
        .enumerate()
        .map(|(i, c)| (to_screaming_snake_case(c.name()), i))
        .collect();

    let mut result = Vec::with_capacity(order_by.len());
    for s in order_by {
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
        result.push((quote_ident(columns[col_idx].name()), dir));
    }
    Ok(result)
}
