/// Creates an [`async_graphql::Error`] from any displayable message.
///
/// This is a convenience wrapper used in GraphQL resolvers to convert an
/// arbitrary error string into the `async_graphql` error type.
#[inline]
pub(crate) fn gql_err(msg: impl std::fmt::Display) -> async_graphql::Error {
    async_graphql::Error::new(msg.to_string())
}
