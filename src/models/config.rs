/// How the library should obtain a database connection.
///
/// Pass one of these variants as the `pool` field of [`Config`].
pub enum PoolConfig {
    /// A `postgres://` (or `postgresql://`) connection string.
    ///
    /// Turbograph will create and manage a `deadpool_postgres::Pool`
    /// from this URL.  Required when `watch_pg` is `true`.
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
    /// Note that `watch_pg` cannot be used with a pre-built pool.
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

/// Top-level configuration passed to [`crate::TurboGraph::new`] or [`crate::build_schema`].
///
/// # Example
///
/// ```rust
/// use turbograph::{Config, PoolConfig};
///
/// let config = Config {
///     pool: PoolConfig::ConnectionString(
///         "postgres://user:pass@localhost:5432/mydb".into(),
///     ),
///     schemas: vec!["public".into()],
///     watch_pg: false,
/// };
/// ```
pub struct Config {
    /// Database connection — either a DSN or an existing pool.
    pub pool: PoolConfig,
    /// PostgreSQL schemas to introspect (e.g. `vec!["public".into()]`).
    pub schemas: Vec<String>,
    /// When `true`, the library installs PostgreSQL event triggers and spawns
    /// a background listener that rebuilds the schema on DDL changes.
    ///
    /// Requires [`PoolConfig::ConnectionString`]; returns an error if
    /// [`PoolConfig::Pool`] is used together with this flag.
    pub watch_pg: bool,
}
