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

/// A PostgreSQL connection URL used exclusively for the DDL-watch
/// `LISTEN`/`NOTIFY` connection.
///
/// Pass this to [`Config::watch_pg`] to enable automatic schema reloading
/// whenever a DDL change is detected.
///
/// # Example
///
/// ```rust
/// use turbograph::WatchPg;
/// let w = WatchPg::new("postgres://user:pass@localhost:5432/mydb");
/// ```
#[derive(Debug)]
pub struct WatchPg(pub String);

impl WatchPg {
    /// Creates a new `WatchPg` from any string-like value.
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
    /// Creates a new `Config` with the given pool configuration.
    ///
    /// `schemas` defaults to an empty list (you must add at least one via
    /// [`Config::schema`]) and `watch_pg` defaults to `None`.
    pub fn new(pool: PoolConfig) -> Self {
        Self {
            pool,
            schemas: Vec::new(),
            watch_pg: None,
        }
    }

    /// Set the database connection using a `postgres://` connection string.
    ///
    /// Replaces any previously set [`PoolConfig`].
    pub fn connection_string(mut self, url: impl Into<String>) -> Self {
        self.pool = PoolConfig::ConnectionString(url.into());
        self
    }

    /// Set the database connection using a pre-built `deadpool_postgres::Pool`.
    ///
    /// Replaces any previously set [`PoolConfig`].
    pub fn pool(mut self, pool: deadpool_postgres::Pool) -> Self {
        self.pool = PoolConfig::Pool(pool);
        self
    }

    /// Append a PostgreSQL schema name to the list of schemas to introspect.
    ///
    /// Call this once per schema.  At least one schema must be added before
    /// calling [`TurboGraph::new`](crate::TurboGraph::new).
    pub fn schema(mut self, schema: impl Into<String>) -> Self {
        self.schemas.push(schema.into());
        self
    }

    /// Enable DDL-watch mode using the given PostgreSQL connection URL.
    ///
    /// When set, Turbograph installs event triggers on first use and spawns a
    /// background task that rebuilds the schema whenever a DDL notification
    /// arrives on the `turbograph_watch` channel.
    pub fn watch_pg(mut self, url: impl Into<String>) -> Self {
        self.watch_pg = Some(WatchPg(url.into()));
        self
    }
}
