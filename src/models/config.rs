/// How the library should obtain a database connection.
pub enum PoolConfig {
    /// A `postgres://` (or `postgresql://`) connection string.
    /// The library will create and own a `deadpool_postgres::Pool` from it.
    ConnectionString(String),
    /// An already-configured pool managed by the caller.
    Pool(deadpool_postgres::Pool),
}

/// Top-level configuration passed to the schema builder.
pub struct Config {
    /// Database connection — either a DSN or an existing pool.
    pub pool: PoolConfig,
    /// PostgreSQL schemas to introspect (e.g. `vec!["public".into()]`).
    pub schemas: Vec<String>,
}

#[derive(Clone)]
pub enum TransactionSettingsValue {
    String(String),
    Integer(i64),
    Boolean(bool),
}

#[derive(Clone)]
pub struct TransactionConfig {
    pub isolation_level: Option<tokio_postgres::IsolationLevel>,
    pub read_only: bool,
    pub deferrable: bool,
    pub role: Option<String>,
    pub timeout_seconds: Option<u64>,
    pub settings: Vec<(String, TransactionSettingsValue)>,
}

impl Default for TransactionConfig {
    fn default() -> Self {
        Self {
            isolation_level: None,
            read_only: false,
            deferrable: false,
            role: None,
            timeout_seconds: None,
            settings: Vec::new(),
        }
    }
}

impl TransactionConfig {
    pub(crate) async fn apply(
        &self,
        client: &tokio_postgres::Client,
    ) -> Result<(), async_graphql::Error> {
        if let Some(ref role) = self.role {
            client
                .query("SELECT set_config('role', $1, true)", &[role])
                .await
                .map_err(|e| gql_err(format!("SET ROLE error: {e}")))?;
        }

        for (key, val) in &self.settings {
            use crate::models::config::TransactionSettingsValue;
            let val_str = match val {
                TransactionSettingsValue::String(s) => s.clone(),
                TransactionSettingsValue::Integer(i) => i.to_string(),
                TransactionSettingsValue::Boolean(b) => b.to_string(),
            };
            client
                .query("SELECT set_config($1, $2, true)", &[key, &val_str])
                .await
                .map_err(|e| gql_err(format!("set_config error: {e}")))?;
        }

        if let Some(secs) = self.timeout_seconds {
            let ms = (secs * 1000).to_string();
            client
                .query("SELECT set_config('statement_timeout', $1, true)", &[&ms])
                .await
                .map_err(|e| gql_err(format!("SET timeout error: {e}")))?;
        }

        Ok(())
    }
}

#[inline]
fn gql_err(msg: impl std::fmt::Display) -> async_graphql::Error {
    async_graphql::Error::new(msg.to_string())
}
