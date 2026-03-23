/// Per-request transaction configuration.
///
/// Inject an instance of this struct into the `async_graphql` request data so
/// that Turbograph can apply the settings inside the `BEGIN` / `COMMIT` block
/// that wraps every query or mutation.
///
/// # Example
///
/// ```rust
/// use turbograph::TransactionConfig;
///
/// // Minimal config — use default values and override only what you need.
/// let tx_config = TransactionConfig {
///     role: Some("app_user".into()),
///     settings: vec![
///         ("app.current_user_id".into(), "42".into()),
///         ("app.tenant_id".into(), "acme".into()),
///     ],
///     ..TransactionConfig::default()
/// };
/// ```
///
/// Pass it to a request:
///
/// ```rust,no_run
/// use turbograph::{TransactionConfig, TurboGraph};
///
/// async fn handle(graph: &TurboGraph, gql_query: &str) {
///     let tx = TransactionConfig {
///         read_only: true,
///         ..TransactionConfig::default()
///     };
///
///     let request = async_graphql::Request::new(gql_query).data(tx);
///     let response = graph.execute(request).await;
///     println!("{:?}", response);
/// }
/// ```
#[derive(Clone, Debug)]
pub struct TransactionConfig {
    /// Transaction isolation level.  `None` uses the server default
    /// (`READ COMMITTED`).
    pub isolation_level: Option<tokio_postgres::IsolationLevel>,
    /// When `true`, the transaction is opened with `READ ONLY`.
    pub read_only: bool,
    /// When `true`, the transaction is opened with `DEFERRABLE`.
    pub deferrable: bool,
    /// PostgreSQL role to switch to inside the transaction via
    /// `SET LOCAL ROLE`.
    pub role: Option<String>,
    /// Statement timeout in seconds.  `None` leaves the server default
    /// unchanged.
    pub timeout_seconds: Option<u64>,
    /// Arbitrary `SET LOCAL` key-value pairs applied after `BEGIN`.
    ///
    /// These are forwarded to PostgreSQL via `SELECT set_config($1, $2, true)`,
    /// which means they are visible to row-level security policies and
    /// PostgreSQL functions as session-like values.
    pub settings: Vec<(String, String)>,
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
    pub fn isolation_level(mut self, level: tokio_postgres::IsolationLevel) -> Self {
        self.isolation_level = Some(level);
        self
    }
    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }
    pub fn deferrable(mut self) -> Self {
        self.deferrable = true;
        self
    }
    pub fn role(mut self, role: impl Into<String>) -> Self {
        self.role = Some(role.into());
        self
    }
    pub fn timeout_seconds(mut self, secs: u64) -> Self {
        self.timeout_seconds = Some(secs);
        self
    }
    pub fn setting(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.settings.push((key.into(), value.into()));
        self
    }
}
