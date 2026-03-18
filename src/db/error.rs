use std::fmt;

/// Errors that can occur during database operations.
#[derive(Debug, Clone)]
pub enum DbError {
    /// The connection pool could not supply a client.
    ///
    /// Typically indicates exhausted connections or an unreachable server.
    Pool(String),
    /// An error occurred while managing the transaction (`BEGIN`, `COMMIT`,
    /// or `ROLLBACK`), or while applying `SET LOCAL` settings.
    Transaction(String),
    /// The SQL query itself failed (e.g. constraint violation, syntax error).
    Query(String),
    /// Input validation failed before a query was sent to the database.
    Validation(String),
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbError::Pool(msg) => write!(f, "Pool error: {}", msg),
            DbError::Transaction(msg) => write!(f, "Transaction error: {}", msg),
            DbError::Query(msg) => write!(f, "Query error: {}", msg),
            DbError::Validation(msg) => write!(f, "Validation error: {}", msg),
        }
    }
}

impl std::error::Error for DbError {}
