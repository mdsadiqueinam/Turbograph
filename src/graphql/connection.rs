use async_graphql::dynamic::{Field, FieldFuture, FieldValue, Object, TypeRef};
use base64::Engine;

use crate::models::connection::ConnectionPayload;

/// Encodes a cursor for a row at `abs_index` (0-based) in the result set.
///
/// The cursor is a base64-encoded JSON value.  When `order_by` is empty the
/// JSON is `[abs_index + 1]`; otherwise it is `[[col1, col2, …], abs_index + 1]`
/// where the columns are lowercased.
///
/// Clients pass the returned string back to the `after` / `before` pagination
/// arguments.
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

/// Builds the shared `PageInfo` GraphQL object type.
///
/// This type is registered once in the schema builder and referenced by every
/// `XxxConnection` type.  It exposes `hasNextPage`, `hasPreviousPage`,
/// `startCursor`, and `endCursor` fields backed by [`ConnectionPayload`].
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
