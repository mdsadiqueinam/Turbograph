use std::sync::Arc;

use crate::table::{Column, Table};
use crate::utils::inflection::{singularize, to_pascal_case};
use async_graphql::dynamic::{Field, FieldFuture, FieldValue, Object, TypeRef};
use tokio_postgres::types::Type;

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

pub fn generate_entity(table: Arc<Table>) -> Object {
    let type_name = to_pascal_case(&singularize(table.name()));
    let obj = Object::new(type_name.as_str());

    table
        .columns()
        .iter()
        .filter(|col| !col.omit_read())
        .fold(obj, |obj, col| {
            obj.field(generate_field(Arc::new(col.clone())))
        })
}
