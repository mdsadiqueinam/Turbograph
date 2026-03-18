/// A single node inside a [`ConnectionPayload`], carrying a cursor and the
/// serialised row data.
#[derive(Clone, Debug)]
pub struct EdgePayload {
    /// Opaque, base64-encoded cursor that identifies this row's position in
    /// the result set.  Suitable for use with `after`/`before` pagination
    /// arguments.
    pub cursor: String,
    /// The resolved row data serialised as a [`serde_json::Value`].
    pub node: serde_json::Value,
}

/// The result returned by every Turbograph list query.
///
/// Maps directly to the GraphQL `XxxConnection` type that is generated for
/// each introspected table.
///
/// ```graphql
/// type UserConnection {
///   totalCount:  Int!
///   pageInfo:    PageInfo!
///   edges:       [UserEdge!]!
///   nodes:       [User!]!
/// }
/// ```
#[derive(Clone, Debug)]
pub struct ConnectionPayload {
    /// Total number of rows matching the applied filter (before pagination).
    pub total_count: i64,
    /// `true` when there are more rows after the current page.
    pub has_next_page: bool,
    /// `true` when there are rows before the current page.
    pub has_previous_page: bool,
    /// The edges in the current page, each containing a cursor and the node data.
    pub edges: Vec<EdgePayload>,
}
