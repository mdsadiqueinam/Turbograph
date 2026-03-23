/// How the library should obtain a database connection.
///
/// Pass one of these variants as the `pool` field of [`Config`].
#[derive(Debug)]
pub enum PoolConfig {
    /// A `postgres://` (or `postgresql://`) connection string.
    ///
    /// Turbograph will create and manage a `deadpool_postgres::Pool`
    /// from this URL.
    ///
    /// # Example
    ///
    /// ```rust
    /// use turbograph::PoolConfig;
    /// let cfg = PoolConfig::ConnectionString(
    ///     "postgres://user:pass@localhost:5432/mydb".into(),
    /// );
    /// ```
    ConnectionString(String),
    /// An already-configured pool managed by the caller.
    ///
    /// Use this variant when your application already owns a
    /// `deadpool_postgres::Pool` and you want Turbograph to share it.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use turbograph::PoolConfig;
    /// use deadpool_postgres::{Config as PoolCfg, Runtime};
    ///
    /// let mut pg_cfg = PoolCfg::new();
    /// pg_cfg.url = Some("postgres://user:pass@localhost:5432/mydb".into());
    /// let pool = pg_cfg
    ///     .create_pool(Some(Runtime::Tokio1), tokio_postgres::NoTls)
    ///     .unwrap();
    ///
    /// let cfg = PoolConfig::Pool(pool);
    /// ```
    Pool(deadpool_postgres::Pool),
}

impl Default for PoolConfig {
    fn default() -> Self {
        PoolConfig::ConnectionString(String::new())
    }
}

#[derive(Debug)]
pub struct WatchPg(pub String);

impl WatchPg {
    pub fn new(url: impl Into<String>) -> Self {
        WatchPg(url.into())
    }
}

/// Top-level configuration passed to [`crate::TurboGraph::new`] or [`crate::build_schema`].
///
/// # Example
///
/// ```rust
/// use turbograph::{Config, PoolConfig, WatchPg};
///
/// let config = Config {
///     pool: PoolConfig::ConnectionString(
///         "postgres://user:pass@localhost:5432/mydb".into(),
///     ),
///     schemas: vec!["public".into()],
///     watch_pg: Some(WatchPg("postgres://user:pass@localhost:5432/mydb".into())),
/// };
/// ```
///
/// # Example with pre-built pool
///
/// ```rust,no_run
/// use turbograph::{Config, PoolConfig, WatchPg};
/// use deadpool_postgres::{Config as PoolCfg, Runtime};
///
/// let mut pg_cfg = PoolCfg::new();
/// pg_cfg.url = Some("postgres://user:pass@localhost:5432/mydb".into());
/// let pool = pg_cfg
///     .create_pool(Some(Runtime::Tokio1), tokio_postgres::NoTls)
///     .unwrap();
///
/// let config = Config {
///     pool: PoolConfig::Pool(pool),
///     schemas: vec!["public".into()],
///     watch_pg: Some(WatchPg("postgres://user:pass@localhost:5432/mydb".into())),
/// };
/// ```
#[derive(Debug, Default)]
pub struct Config {
    /// Database connection — either a DSN or an existing pool.
    pub pool: PoolConfig,
    /// PostgreSQL schemas to introspect (e.g. `vec!["public".into()]`).
    pub schemas: Vec<String>,
    /// When `Some(WatchPg(url))`, the library installs PostgreSQL event triggers
    /// and spawns a background listener that rebuilds the schema on DDL changes.
    /// The URL is used exclusively for the LISTEN/NOTIFY connection and is
    /// independent of the [`PoolConfig`] variant used for regular queries.
    pub watch_pg: Option<WatchPg>,
}

impl Config {
    pub fn new(pool: PoolConfig) -> Self {
        Self {
            pool,
            schemas: Vec::new(),
            watch_pg: None,
        }
    }
    pub fn connection_string(mut self, url: impl Into<String>) -> Self {
        self.pool = PoolConfig::ConnectionString(url.into());
        self
    }

    pub fn pool(mut self, pool: deadpool_postgres::Pool) -> Self {
        self.pool = PoolConfig::Pool(pool);
        self
    }

    pub fn schema(mut self, schema: impl Into<String>) -> Self {
        self.schemas.push(schema.into());
        self
    }

    pub fn watch_pg(mut self, url: impl Into<String>) -> Self {
        self.watch_pg = Some(WatchPg(url.into()));
        self
    }
}
