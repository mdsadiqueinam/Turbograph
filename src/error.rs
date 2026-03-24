/// Creates an [`async_graphql::Error`] from any displayable message.
///
/// This is a convenience wrapper used in GraphQL resolvers to convert an
/// arbitrary error string into the `async_graphql` error type.
#[inline]
pub(crate) fn gql_err(msg: impl std::fmt::Display) -> async_graphql::Error {
    async_graphql::Error::new(msg.to_string())
}

/// Converts a [`DbError`] into an [`async_graphql::Error`] for use in GraphQL resolvers.
#[inline]
pub(crate) fn db_err_to_gql(err: crate::db::error::DbError) -> async_graphql::Error {
    async_graphql::Error::new(err.to_string())
}
