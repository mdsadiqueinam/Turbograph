use async_graphql::dynamic::FieldValue;

use crate::models::connection::{ConnectionPayload, EdgePayload};

use super::super::connection::encode_cursor;

/// Builds a `ConnectionPayload` from already-fetched query results.
///
/// Computes `hasNextPage` and `hasPreviousPage` from the `total_count` and
/// `offset`, assigns base64-encoded cursors to each edge, and wraps the result
/// in a [`FieldValue`] ready for the GraphQL resolver.
pub(super) fn build_connection_payload(
    total_count: i64,
    json_rows: Vec<serde_json::Value>,
    order_by: &[String],
    offset: i64,
) -> Option<FieldValue<'static>> {
    let edge_count = json_rows.len() as i64;

    let edges = json_rows
        .into_iter()
        .enumerate()
        .map(|(i, node)| EdgePayload {
            cursor: encode_cursor(order_by, (offset as usize) + i),
            node,
        })
        .collect();

    Some(FieldValue::owned_any(ConnectionPayload {
        total_count,
        has_next_page: (offset + edge_count) < total_count,
        has_previous_page: offset > 0,
        edges,
    }))
}
