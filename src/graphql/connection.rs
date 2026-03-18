use async_graphql::dynamic::{Field, FieldFuture, FieldValue, Object, TypeRef};
use base64::Engine;

use crate::models::connection::ConnectionPayload;

pub fn encode_cursor(order_by: &[String], abs_index: usize) -> String {
    let json = if order_by.is_empty() {
        serde_json::json!([abs_index + 1])
    } else {
        let keys: Vec<String> = order_by.iter().map(|s| s.to_lowercase()).collect();
        serde_json::json!([keys, abs_index + 1])
    };
    base64::engine::general_purpose::STANDARD.encode(json.to_string())
}

// ── Shared PageInfo type (register once globally) ───────────────────────────

pub fn make_page_info_type() -> Object {
    Object::new("PageInfo")
        .field(Field::new(
            "hasNextPage",
            TypeRef::named_nn(TypeRef::BOOLEAN),
            |ctx| {
                FieldFuture::new(async move {
                    let payload = ctx.parent_value.try_downcast_ref::<ConnectionPayload>()?;
                    Ok(Some(FieldValue::value(payload.has_next_page)))
                })
            },
        ))
        .field(Field::new(
            "hasPreviousPage",
            TypeRef::named_nn(TypeRef::BOOLEAN),
            |ctx| {
                FieldFuture::new(async move {
                    let payload = ctx.parent_value.try_downcast_ref::<ConnectionPayload>()?;
                    Ok(Some(FieldValue::value(payload.has_previous_page)))
                })
            },
        ))
        .field(Field::new(
            "startCursor",
            TypeRef::named(TypeRef::STRING),
            |ctx| {
                FieldFuture::new(async move {
                    let payload = ctx.parent_value.try_downcast_ref::<ConnectionPayload>()?;
                    let val = payload
                        .edges
                        .first()
                        .map(|e| FieldValue::value(e.cursor.clone()));
                    Ok(val)
                })
            },
        ))
        .field(Field::new(
            "endCursor",
            TypeRef::named(TypeRef::STRING),
            |ctx| {
                FieldFuture::new(async move {
                    let payload = ctx.parent_value.try_downcast_ref::<ConnectionPayload>()?;
                    let val = payload
                        .edges
                        .last()
                        .map(|e| FieldValue::value(e.cursor.clone()));
                    Ok(val)
                })
            },
        ))
}
