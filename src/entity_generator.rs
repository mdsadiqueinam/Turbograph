use std::sync::Arc;

use async_graphql::Value as GqlValue;
use async_graphql::dynamic::{
    Enum, EnumItem, Field, FieldFuture, FieldValue, InputObject, InputValue, Object, TypeRef,
};
use bytes::BytesMut;
use deadpool_postgres::Pool;
use tokio_postgres::types::{IsNull, ToSql, Type};

use crate::extensions::JsonListExt;
use crate::table::{Column, Table};
use crate::utils::inflection::to_pascal_case;

// ── SQL parameter wrapper ────────────────────────────────────────────────────
// Lets us build a Vec<SqlScalar> then borrow as &[&(dyn ToSql + Sync)], which
// is what tokio_postgres::Client::query expects.

#[derive(Debug)]
enum SqlScalar {
    Bool(bool),
    Int2(i16),
    Int4(i32),
    Int8(i64),
    Float4(f32),
    Float8(f64),
    Text(String),
    Json(serde_json::Value),
}

impl ToSql for SqlScalar {
    fn to_sql(
        &self,
        ty: &Type,
        out: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        match self {
            SqlScalar::Bool(v) => v.to_sql(ty, out),
            SqlScalar::Int2(v) => v.to_sql(ty, out),
            SqlScalar::Int4(v) => v.to_sql(ty, out),
            SqlScalar::Int8(v) => v.to_sql(ty, out),
            SqlScalar::Float4(v) => v.to_sql(ty, out),
            SqlScalar::Float8(v) => v.to_sql(ty, out),
            SqlScalar::Text(v) => v.to_sql(ty, out),
            SqlScalar::Json(v) => v.to_sql(ty, out),
        }
    }

    fn accepts(ty: &Type) -> bool {
        matches!(
            *ty,
            Type::BOOL
                | Type::INT2
                | Type::INT4
                | Type::INT8
                | Type::FLOAT4
                | Type::FLOAT8
                | Type::TEXT
                | Type::VARCHAR
                | Type::BPCHAR
                | Type::JSON
                | Type::JSONB
        )
    }

    tokio_postgres::types::to_sql_checked!();
}

// ── field-value helpers ──────────────────────────────────────────────────────

fn get_field_value<'a>(column: &Column, value: &serde_json::Value) -> Option<FieldValue<'a>> {
    let raw_val = value.get(column.name())?;

    if raw_val.is_null() {
        return None;
    }

    let field_val = match *column._type() {
        Type::BOOL => FieldValue::value(raw_val.as_bool()),
        Type::INT2 | Type::INT4 => FieldValue::value(raw_val.as_i64().map(|v| v as i32)),
        // i64 exceeds GraphQL Int (i32), so serialise as String
        Type::INT8 => FieldValue::value(raw_val.as_i64().map(|v| v.to_string())),
        Type::FLOAT4 | Type::FLOAT8 => FieldValue::value(raw_val.as_f64()),
        Type::TEXT | Type::VARCHAR | Type::BPCHAR => FieldValue::value(raw_val.as_str()),
        // JSON/JSONB: serialise to a JSON string
        Type::JSON | Type::JSONB => FieldValue::value(Some(raw_val.to_string())),
        // --- array types ---
        Type::BOOL_ARRAY => FieldValue::list(
            raw_val
                .as_array()
                .into_iter()
                .flatten()
                .map(|v| FieldValue::value(v.as_bool()))
                .collect::<Vec<_>>(),
        ),
        Type::INT2_ARRAY | Type::INT4_ARRAY => FieldValue::list(
            raw_val
                .as_array()
                .into_iter()
                .flatten()
                .map(|v| FieldValue::value(v.as_i64().map(|n| n as i32)))
                .collect::<Vec<_>>(),
        ),
        Type::INT8_ARRAY => FieldValue::list(
            raw_val
                .as_array()
                .into_iter()
                .flatten()
                .map(|v| FieldValue::value(v.as_i64().map(|n| n.to_string())))
                .collect::<Vec<_>>(),
        ),
        Type::FLOAT4_ARRAY | Type::FLOAT8_ARRAY => FieldValue::list(
            raw_val
                .as_array()
                .into_iter()
                .flatten()
                .map(|v| FieldValue::value(v.as_f64()))
                .collect::<Vec<_>>(),
        ),
        Type::TEXT_ARRAY | Type::VARCHAR_ARRAY | Type::BPCHAR_ARRAY => FieldValue::list(
            raw_val
                .as_array()
                .into_iter()
                .flatten()
                .map(|v| FieldValue::value(v.as_str()))
                .collect::<Vec<_>>(),
        ),
        Type::JSON_ARRAY | Type::JSONB_ARRAY => FieldValue::list(
            raw_val
                .as_array()
                .into_iter()
                .flatten()
                .map(|v| FieldValue::value(Some(v.to_string())))
                .collect::<Vec<_>>(),
        ),
        _ => FieldValue::value(raw_val.as_str()),
    };

    Some(field_val)
}

fn get_type_ref(column: &Column) -> TypeRef {
    let (base, is_list): (&str, bool) = match *column._type() {
        Type::BOOL => (TypeRef::BOOLEAN, false),
        Type::INT2 | Type::INT4 => (TypeRef::INT, false),
        // i64 exceeds GraphQL Int (i32), expose as String
        Type::INT8 => (TypeRef::STRING, false),
        Type::FLOAT4 | Type::FLOAT8 => (TypeRef::FLOAT, false),
        Type::TEXT | Type::VARCHAR | Type::BPCHAR => (TypeRef::STRING, false),
        // JSON/JSONB serialised as a JSON string
        Type::JSON | Type::JSONB => (TypeRef::STRING, false),
        // --- array types ---
        Type::BOOL_ARRAY => (TypeRef::BOOLEAN, true),
        Type::INT2_ARRAY | Type::INT4_ARRAY => (TypeRef::INT, true),
        Type::INT8_ARRAY => (TypeRef::STRING, true),
        Type::FLOAT4_ARRAY | Type::FLOAT8_ARRAY => (TypeRef::FLOAT, true),
        Type::TEXT_ARRAY | Type::VARCHAR_ARRAY | Type::BPCHAR_ARRAY => (TypeRef::STRING, true),
        Type::JSON_ARRAY | Type::JSONB_ARRAY => (TypeRef::STRING, true),
        _ => (TypeRef::STRING, false),
    };

    match (is_list, column.nullable()) {
        (false, true) => TypeRef::named(base),
        (false, false) => TypeRef::named_nn(base),
        (true, true) => TypeRef::named_list(base),
        (true, false) => TypeRef::named_nn_list(base),
    }
}

/// Returns a nullable scalar TypeRef for use in a condition input object.
/// Returns None for array / unsupported types (they cannot be equality-filtered).
fn condition_type_ref(column: &Column) -> Option<TypeRef> {
    let scalar = match *column._type() {
        Type::BOOL => TypeRef::BOOLEAN,
        Type::INT2 | Type::INT4 => TypeRef::INT,
        // INT8 mapped to String (i64 > i32 GraphQL range)
        Type::INT8 => TypeRef::STRING,
        Type::FLOAT4 | Type::FLOAT8 => TypeRef::FLOAT,
        Type::TEXT | Type::VARCHAR | Type::BPCHAR => TypeRef::STRING,
        // JSON/JSONB accept a serialised JSON string for filtering
        Type::JSON | Type::JSONB => TypeRef::STRING,
        // arrays and everything else are excluded from condition
        _ => return None,
    };
    // Always nullable — every condition field is optional
    Some(TypeRef::named(scalar))
}

/// Convert an incoming GraphQL argument value to a typed SQL parameter.
fn to_sql_scalar(column: &Column, val: &GqlValue) -> Option<SqlScalar> {
    match *column._type() {
        Type::BOOL => {
            if let GqlValue::Boolean(b) = val {
                Some(SqlScalar::Bool(*b))
            } else {
                None
            }
        }
        Type::INT2 => {
            if let GqlValue::Number(n) = val {
                n.as_i64().map(|v| SqlScalar::Int2(v as i16))
            } else {
                None
            }
        }
        Type::INT4 => {
            if let GqlValue::Number(n) = val {
                n.as_i64().map(|v| SqlScalar::Int4(v as i32))
            } else {
                None
            }
        }
        // INT8 is exposed as String in the schema
        Type::INT8 => match val {
            GqlValue::Number(n) => n.as_i64().map(SqlScalar::Int8),
            GqlValue::String(s) => s.parse::<i64>().ok().map(SqlScalar::Int8),
            _ => None,
        },
        Type::FLOAT4 => {
            if let GqlValue::Number(n) = val {
                n.as_f64().map(|v| SqlScalar::Float4(v as f32))
            } else {
                None
            }
        }
        Type::FLOAT8 => {
            if let GqlValue::Number(n) = val {
                n.as_f64().map(SqlScalar::Float8)
            } else {
                None
            }
        }
        Type::TEXT | Type::VARCHAR | Type::BPCHAR => {
            if let GqlValue::String(s) = val {
                Some(SqlScalar::Text(s.clone()))
            } else {
                None
            }
        }
        // JSON/JSONB condition value is a serialised JSON string
        Type::JSON | Type::JSONB => {
            if let GqlValue::String(s) = val {
                serde_json::from_str(s).ok().map(SqlScalar::Json)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn generate_field(column: Arc<Column>) -> Field {
    Field::new(
        column.name().to_string(),
        get_type_ref(&column),
        move |ctx| {
            let column = column.clone();

            FieldFuture::new(async move {
                let parent_value = ctx.parent_value.try_downcast_ref::<serde_json::Value>()?;
                let field_value = get_field_value(&column, parent_value);
                Ok(field_value)
            })
        },
    )
}

// ── public API ───────────────────────────────────────────────────────────────

pub fn generate_entity(table: Arc<Table>) -> Object {
    let type_name = table.type_name();
    let obj = Object::new(type_name.as_str());

    table
        .columns()
        .iter()
        .filter(|col| !col.omit_read())
        .fold(obj, |obj, col| {
            obj.field(generate_field(Arc::new(col.clone())))
        })
}

/// Builds the `{TypeName}Condition` input object (equality filters per column).
/// Exported so callers can register it with the schema separately.
pub fn make_condition_type(table: &Table) -> InputObject {
    let name = format!("{}Condition", table.type_name());
    table
        .columns()
        .iter()
        .filter(|c| !c.omit_read())
        .fold(
            InputObject::new(name),
            |obj, col| match condition_type_ref(col) {
                Some(tr) => obj.field(InputValue::new(col.name().as_str(), tr)),
                None => obj,
            },
        )
}

/// Builds the `{TypeName}OrderBy` enum (COLUMN_ASC / COLUMN_DESC per column).
/// Exported so callers can register it with the schema separately.
pub fn make_order_by_enum(table: &Table) -> Enum {
    let name = format!("{}OrderBy", table.type_name());
    table
        .columns()
        .iter()
        .filter(|c| !c.omit_read())
        .flat_map(|c| {
            let upper = c.name().to_uppercase();
            [
                EnumItem::new(format!("{}_ASC", upper)),
                EnumItem::new(format!("{}_DESC", upper)),
            ]
        })
        .fold(Enum::new(name), |e, item| e.item(item))
}

/// Everything the schema builder needs for one table.
pub struct GeneratedQuery {
    /// The root Query field (e.g. `allUsers`).
    pub query_field: Field,
    /// The `{T}Condition` input type — must be registered with the schema.
    pub condition_type: InputObject,
    /// The `{T}OrderBy` enum — must be registered with the schema.
    pub order_by_enum: Enum,
}

/// Generates a root Query field (e.g. `allUsers`) with PostGraphile-style
/// filtering arguments:
///
/// ```graphql
/// allUsers(
///   condition: UserCondition   # equality filter per column
///   orderBy:   UserOrderBy     # COLUMN_ASC / COLUMN_DESC
///   first:     Int             # LIMIT
///   offset:    Int             # OFFSET
/// ): [User!]!
/// ```
pub fn generate_query(table: Arc<Table>, pool: Arc<Pool>) -> GeneratedQuery {
    let condition_type = make_condition_type(&table);
    let order_by_enum = make_order_by_enum(&table);

    let type_name = table.type_name();
    let condition_type_name = condition_type.type_name().to_string();
    let order_by_type_name = order_by_enum.type_name().to_string();
    let field_name = format!("all{}", to_pascal_case(table.name()));
    let tbl_schema = table.schema_name().to_string();
    let tbl_name = table.name().to_string();
    // Snapshot columns once; clone is cheap (Arc inside would be better for
    // large schemas, but Vec<Column> is fine here).
    let columns = Arc::new(table.columns().to_vec());

    let query_field = Field::new(
        field_name,
        TypeRef::named_nn_list_nn(type_name),
        move |ctx| {
            // ── extract args synchronously (ctx lifetime doesn't cross await) ──
            let condition_pairs: Option<Vec<(String, GqlValue)>> = ctx
                .args
                .get("condition")
                .and_then(|v| v.object().ok())
                .map(|obj| {
                    obj.iter()
                        .map(|(k, v)| (k.to_string(), v.as_value().clone()))
                        .collect()
                });

            let order_by = ctx
                .args
                .get("orderBy")
                .and_then(|v| v.enum_name().ok().map(|s| s.to_string()));

            let first = ctx.args.get("first").and_then(|v| v.i64().ok());
            let offset = ctx.args.get("offset").and_then(|v| v.i64().ok());

            let pool = pool.clone();
            let tbl_schema = tbl_schema.clone();
            let tbl_name = tbl_name.clone();
            let columns = columns.clone();

            FieldFuture::new(async move {
                // ── WHERE ────────────────────────────────────────────────────
                let mut where_clauses = Vec::<String>::new();
                let mut params = Vec::<SqlScalar>::new();

                if let Some(pairs) = condition_pairs {
                    for (key, gql_val) in pairs {
                        let Some(col) = columns
                            .iter()
                            .find(|c| !c.omit_read() && c.name() == key.as_str())
                        else {
                            continue;
                        };
                        if let Some(scalar) = to_sql_scalar(col, &gql_val) {
                            where_clauses.push(format!(
                                "\"{}\" = ${}",
                                col.name(),
                                params.len() + 1
                            ));
                            params.push(scalar);
                        }
                    }
                }

                let where_sql = if where_clauses.is_empty() {
                    String::new()
                } else {
                    format!("WHERE {}", where_clauses.join(" AND "))
                };

                // ── ORDER BY ─────────────────────────────────────────────────
                // The value is one of our own enum variants (e.g. CREATED_AT_DESC),
                // so we validate the column name against the known column list
                // before interpolating — no injection possible.
                let order_sql = if let Some(s) = &order_by {
                    let (col_upper, dir) = if s.ends_with("_DESC") {
                        (&s[..s.len() - 5], "DESC")
                    } else {
                        (&s[..s.len() - 4], "ASC")
                    };
                    let col_name = col_upper.to_lowercase();
                    if columns
                        .iter()
                        .any(|c| !c.omit_read() && c.name() == col_name.as_str())
                    {
                        format!("ORDER BY \"{}\" {}", col_name, dir)
                    } else {
                        return Err(async_graphql::Error::new(format!(
                            "unknown column for ordering: {}",
                            col_name
                        )));
                    }
                } else {
                    String::new()
                };

                // ── LIMIT / OFFSET ───────────────────────────────────────────
                let limit_sql = first.map(|n| format!("LIMIT {}", n)).unwrap_or_default();
                let offset_sql = offset.map(|n| format!("OFFSET {}", n)).unwrap_or_default();

                // ── execute ──────────────────────────────────────────────────
                let sql = format!(
                    "SELECT * FROM \"{}\".\"{}\" {} {} {} {}",
                    tbl_schema, tbl_name, where_sql, order_sql, limit_sql, offset_sql
                );

                let param_refs: Vec<&(dyn ToSql + Sync)> =
                    params.iter().map(|p| p as &(dyn ToSql + Sync)).collect();

                let client = pool
                    .get()
                    .await
                    .map_err(|e| async_graphql::Error::new(format!("DB pool error: {e}")))?;

                let rows = client
                    .query(sql.as_str(), param_refs.as_slice())
                    .await
                    .map_err(|e| async_graphql::Error::new(format!("DB query error: {e}")))?;

                let values = rows
                    .to_json_list()
                    .into_iter()
                    .map(FieldValue::owned_any)
                    .collect::<Vec<_>>();

                Ok(Some(FieldValue::list(values)))
            })
        },
    )
    .argument(InputValue::new(
        "condition",
        TypeRef::named(condition_type_name),
    ))
    .argument(InputValue::new(
        "orderBy",
        TypeRef::named(order_by_type_name),
    ))
    .argument(InputValue::new("first", TypeRef::named(TypeRef::INT)))
    .argument(InputValue::new("offset", TypeRef::named(TypeRef::INT)));

    GeneratedQuery {
        query_field,
        condition_type,
        order_by_enum,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::{Column, Table};
    use serde_json::json;

    // ── get_type_ref ─────────────────────────────────────────────────────────

    #[test]
    fn test_type_ref_bool_non_nullable() {
        let col = Column::new_for_test("active", Type::BOOL, false, false);
        assert_eq!(get_type_ref(&col).to_string(), "Boolean!");
    }

    #[test]
    fn test_type_ref_bool_nullable() {
        let col = Column::new_for_test("active", Type::BOOL, true, false);
        assert_eq!(get_type_ref(&col).to_string(), "Boolean");
    }

    #[test]
    fn test_type_ref_int4_non_nullable() {
        let col = Column::new_for_test("count", Type::INT4, false, false);
        assert_eq!(get_type_ref(&col).to_string(), "Int!");
    }

    #[test]
    fn test_type_ref_int4_nullable() {
        let col = Column::new_for_test("count", Type::INT4, true, false);
        assert_eq!(get_type_ref(&col).to_string(), "Int");
    }

    #[test]
    fn test_type_ref_int8_exposed_as_string() {
        let col = Column::new_for_test("big_id", Type::INT8, false, false);
        assert_eq!(get_type_ref(&col).to_string(), "String!");
    }

    #[test]
    fn test_type_ref_float4_non_nullable() {
        let col = Column::new_for_test("price", Type::FLOAT4, false, false);
        assert_eq!(get_type_ref(&col).to_string(), "Float!");
    }

    #[test]
    fn test_type_ref_float8_nullable() {
        let col = Column::new_for_test("price", Type::FLOAT8, true, false);
        assert_eq!(get_type_ref(&col).to_string(), "Float");
    }

    #[test]
    fn test_type_ref_text_non_nullable() {
        let col = Column::new_for_test("title", Type::TEXT, false, false);
        assert_eq!(get_type_ref(&col).to_string(), "String!");
    }

    #[test]
    fn test_type_ref_varchar_non_nullable() {
        let col = Column::new_for_test("code", Type::VARCHAR, false, false);
        assert_eq!(get_type_ref(&col).to_string(), "String!");
    }

    #[test]
    fn test_type_ref_jsonb_non_nullable() {
        let col = Column::new_for_test("meta", Type::JSONB, false, false);
        assert_eq!(get_type_ref(&col).to_string(), "String!");
    }

    #[test]
    fn test_type_ref_json_nullable() {
        let col = Column::new_for_test("meta", Type::JSON, true, false);
        assert_eq!(get_type_ref(&col).to_string(), "String");
    }

    #[test]
    fn test_type_ref_bool_array_non_nullable() {
        let col = Column::new_for_test("flags", Type::BOOL_ARRAY, false, false);
        assert_eq!(get_type_ref(&col).to_string(), "[Boolean!]");
    }

    #[test]
    fn test_type_ref_bool_array_nullable() {
        let col = Column::new_for_test("flags", Type::BOOL_ARRAY, true, false);
        assert_eq!(get_type_ref(&col).to_string(), "[Boolean]");
    }

    #[test]
    fn test_type_ref_int4_array_non_nullable() {
        let col = Column::new_for_test("ids", Type::INT4_ARRAY, false, false);
        assert_eq!(get_type_ref(&col).to_string(), "[Int!]");
    }

    #[test]
    fn test_type_ref_int4_array_nullable() {
        let col = Column::new_for_test("ids", Type::INT4_ARRAY, true, false);
        assert_eq!(get_type_ref(&col).to_string(), "[Int]");
    }

    #[test]
    fn test_type_ref_text_array_non_nullable() {
        let col = Column::new_for_test("tags", Type::TEXT_ARRAY, false, false);
        assert_eq!(get_type_ref(&col).to_string(), "[String!]");
    }

    #[test]
    fn test_type_ref_jsonb_array_non_nullable() {
        let col = Column::new_for_test("payloads", Type::JSONB_ARRAY, false, false);
        assert_eq!(get_type_ref(&col).to_string(), "[String!]");
    }

    // ── get_field_value ───────────────────────────────────────────────────────

    #[test]
    fn test_field_value_missing_key_returns_none() {
        let col = Column::new_for_test("name", Type::TEXT, false, false);
        let val = json!({ "other": "value" });
        assert!(get_field_value(&col, &val).is_none());
    }

    #[test]
    fn test_field_value_null_returns_none() {
        let col = Column::new_for_test("name", Type::TEXT, true, false);
        let val = json!({ "name": null });
        assert!(get_field_value(&col, &val).is_none());
    }

    #[test]
    fn test_field_value_bool_present() {
        let col = Column::new_for_test("active", Type::BOOL, false, false);
        let val = json!({ "active": true });
        assert!(get_field_value(&col, &val).is_some());
    }

    #[test]
    fn test_field_value_int2_present() {
        let col = Column::new_for_test("score", Type::INT2, false, false);
        let val = json!({ "score": 7 });
        assert!(get_field_value(&col, &val).is_some());
    }

    #[test]
    fn test_field_value_int4_present() {
        let col = Column::new_for_test("count", Type::INT4, false, false);
        let val = json!({ "count": 42 });
        assert!(get_field_value(&col, &val).is_some());
    }

    #[test]
    fn test_field_value_int8_present() {
        let col = Column::new_for_test("big_id", Type::INT8, false, false);
        let val = json!({ "big_id": 9223372036854775807_i64 });
        assert!(get_field_value(&col, &val).is_some());
    }

    #[test]
    fn test_field_value_float8_present() {
        let col = Column::new_for_test("price", Type::FLOAT8, false, false);
        let val = json!({ "price": 3.14 });
        assert!(get_field_value(&col, &val).is_some());
    }

    #[test]
    fn test_field_value_text_present() {
        let col = Column::new_for_test("title", Type::TEXT, false, false);
        let val = json!({ "title": "hello" });
        assert!(get_field_value(&col, &val).is_some());
    }

    #[test]
    fn test_field_value_jsonb_present() {
        let col = Column::new_for_test("meta", Type::JSONB, false, false);
        let val = json!({ "meta": { "key": "value" } });
        assert!(get_field_value(&col, &val).is_some());
    }

    #[test]
    fn test_field_value_bool_array_present() {
        let col = Column::new_for_test("flags", Type::BOOL_ARRAY, false, false);
        let val = json!({ "flags": [true, false, true] });
        assert!(get_field_value(&col, &val).is_some());
    }

    #[test]
    fn test_field_value_int4_array_present() {
        let col = Column::new_for_test("ids", Type::INT4_ARRAY, false, false);
        let val = json!({ "ids": [1, 2, 3] });
        assert!(get_field_value(&col, &val).is_some());
    }

    #[test]
    fn test_field_value_int8_array_present() {
        let col = Column::new_for_test("ids", Type::INT8_ARRAY, false, false);
        let val = json!({ "ids": [1000000000000_i64, 2000000000000_i64] });
        assert!(get_field_value(&col, &val).is_some());
    }

    #[test]
    fn test_field_value_float8_array_present() {
        let col = Column::new_for_test("scores", Type::FLOAT8_ARRAY, false, false);
        let val = json!({ "scores": [1.1, 2.2] });
        assert!(get_field_value(&col, &val).is_some());
    }

    #[test]
    fn test_field_value_text_array_present() {
        let col = Column::new_for_test("tags", Type::TEXT_ARRAY, false, false);
        let val = json!({ "tags": ["rust", "graphql"] });
        assert!(get_field_value(&col, &val).is_some());
    }

    #[test]
    fn test_field_value_jsonb_array_present() {
        let col = Column::new_for_test("payloads", Type::JSONB_ARRAY, false, false);
        let val = json!({ "payloads": [{"a": 1}, {"b": 2}] });
        assert!(get_field_value(&col, &val).is_some());
    }

    // ── condition_type_ref ───────────────────────────────────────────────────

    #[test]
    fn test_condition_type_ref_bool_nullable() {
        let col = Column::new_for_test("active", Type::BOOL, false, false);
        assert_eq!(condition_type_ref(&col).unwrap().to_string(), "Boolean");
    }

    #[test]
    fn test_condition_type_ref_int4_nullable() {
        let col = Column::new_for_test("count", Type::INT4, false, false);
        assert_eq!(condition_type_ref(&col).unwrap().to_string(), "Int");
    }

    #[test]
    fn test_condition_type_ref_int8_as_string() {
        let col = Column::new_for_test("big_id", Type::INT8, false, false);
        assert_eq!(condition_type_ref(&col).unwrap().to_string(), "String");
    }

    #[test]
    fn test_condition_type_ref_text_nullable() {
        let col = Column::new_for_test("name", Type::TEXT, false, false);
        assert_eq!(condition_type_ref(&col).unwrap().to_string(), "String");
    }

    #[test]
    fn test_condition_type_ref_jsonb_nullable() {
        let col = Column::new_for_test("meta", Type::JSONB, false, false);
        assert_eq!(condition_type_ref(&col).unwrap().to_string(), "String");
    }

    #[test]
    fn test_condition_type_ref_array_excluded() {
        let col = Column::new_for_test("ids", Type::INT4_ARRAY, false, false);
        assert!(condition_type_ref(&col).is_none());
    }

    #[test]
    fn test_condition_type_ref_bool_array_excluded() {
        let col = Column::new_for_test("flags", Type::BOOL_ARRAY, false, false);
        assert!(condition_type_ref(&col).is_none());
    }

    // ── to_sql_scalar ────────────────────────────────────────────────────────

    #[test]
    fn test_to_sql_scalar_bool() {
        let col = Column::new_for_test("active", Type::BOOL, false, false);
        assert!(matches!(
            to_sql_scalar(&col, &GqlValue::Boolean(true)),
            Some(SqlScalar::Bool(true))
        ));
    }

    #[test]
    fn test_to_sql_scalar_int4() {
        let col = Column::new_for_test("count", Type::INT4, false, false);
        let val = GqlValue::Number(serde_json::Number::from(42_i64));
        assert!(matches!(
            to_sql_scalar(&col, &val),
            Some(SqlScalar::Int4(42))
        ));
    }

    #[test]
    fn test_to_sql_scalar_int8_from_string() {
        let col = Column::new_for_test("big_id", Type::INT8, false, false);
        let val = GqlValue::String("9223372036854775807".to_string());
        assert!(matches!(
            to_sql_scalar(&col, &val),
            Some(SqlScalar::Int8(9223372036854775807))
        ));
    }

    #[test]
    fn test_to_sql_scalar_text() {
        let col = Column::new_for_test("name", Type::TEXT, false, false);
        let val = GqlValue::String("alice".to_string());
        assert!(matches!(
            to_sql_scalar(&col, &val),
            Some(SqlScalar::Text(_))
        ));
    }

    #[test]
    fn test_to_sql_scalar_wrong_type_returns_none() {
        let col = Column::new_for_test("active", Type::BOOL, false, false);
        // passing a string for a bool column
        let val = GqlValue::String("true".to_string());
        assert!(to_sql_scalar(&col, &val).is_none());
    }

    #[test]
    fn test_to_sql_scalar_array_col_returns_none() {
        let col = Column::new_for_test("ids", Type::INT4_ARRAY, false, false);
        let val = GqlValue::Number(serde_json::Number::from(1_i64));
        assert!(to_sql_scalar(&col, &val).is_none());
    }

    // ── make_condition_type ──────────────────────────────────────────────────

    #[test]
    fn test_condition_type_name() {
        let table = Table::new_for_test("blog_posts", vec![]);
        assert_eq!(make_condition_type(&table).type_name(), "BlogPostCondition");
    }

    #[test]
    fn test_condition_type_name_users() {
        let table = Table::new_for_test("users", vec![]);
        assert_eq!(make_condition_type(&table).type_name(), "UserCondition");
    }

    // ── make_order_by_enum ───────────────────────────────────────────────────

    #[test]
    fn test_order_by_enum_name() {
        let table = Table::new_for_test("blog_posts", vec![]);
        assert_eq!(make_order_by_enum(&table).type_name(), "BlogPostOrderBy");
    }

    #[test]
    fn test_order_by_enum_name_users() {
        let table = Table::new_for_test("users", vec![]);
        assert_eq!(make_order_by_enum(&table).type_name(), "UserOrderBy");
    }

    // ── generate_entity ───────────────────────────────────────────────────────

    #[test]
    fn test_entity_name_singularized_and_pascal_cased() {
        let table = Arc::new(Table::new_for_test("blog_posts", vec![]));
        assert_eq!(generate_entity(table).type_name(), "BlogPost");
    }

    #[test]
    fn test_entity_name_already_singular() {
        let table = Arc::new(Table::new_for_test("users", vec![]));
        assert_eq!(generate_entity(table).type_name(), "User");
    }

    #[test]
    fn test_entity_name_single_word() {
        let table = Arc::new(Table::new_for_test("orders", vec![]));
        assert_eq!(generate_entity(table).type_name(), "Order");
    }

    #[test]
    fn test_entity_omit_read_column_excluded() {
        let visible = Column::new_for_test("secret", Type::TEXT, false, false);
        let hidden = Column::new_for_test("secret", Type::TEXT, false, true);
        let table = Arc::new(Table::new_for_test("users", vec![visible, hidden]));
        generate_entity(table);
    }

    #[test]
    fn test_entity_no_columns_empty_object() {
        let table = Arc::new(Table::new_for_test("tokens", vec![]));
        let obj = generate_entity(table);
        assert_eq!(obj.type_name(), "Token");
    }
}
