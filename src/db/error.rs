/// Errors that can occur during database operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum DbError {
    /// The connection pool could not supply a client.
    ///
    /// Typically indicates exhausted connections or an unreachable server.
    #[error("Pool error: {0}")]
    Pool(String),
    /// An error occurred while managing the transaction (`BEGIN`, `COMMIT`,
    /// or `ROLLBACK`), or while applying `SET LOCAL` settings.
    #[error("Transaction error: {0}")]
    Transaction(String),
    /// The SQL query itself failed (e.g. constraint violation, syntax error).
    #[error("Query error: {0}")]
    Query(String),
    /// Input validation failed before a query was sent to the database.
    #[error("Validation error: {0}")]
    Validation(String),
}
