use deadpool_postgres::Pool;

use super::query::{delete::Delete, insert::Insert, select::Select, update::Update};
use crate::models::config::PoolConfig;

/// Resolves a [`PoolConfig`] into a concrete `deadpool_postgres::Pool`.
///
/// When the config is [`PoolConfig::ConnectionString`], a new pool is created
/// using `tokio_postgres::NoTls`.  When it is [`PoolConfig::Pool`], the
/// already-built pool is returned unchanged.
pub(crate) fn resolve(
    config: PoolConfig,
) -> Result<deadpool_postgres::Pool, Box<dyn std::error::Error + Send + Sync>> {
    match config {
        PoolConfig::ConnectionString(url) => {
            let mut cfg = deadpool_postgres::Config::new();
            cfg.url = Some(url);
            Ok(cfg.create_pool(
                Some(deadpool_postgres::Runtime::Tokio1),
                tokio_postgres::NoTls,
            )?)
        }
        PoolConfig::Pool(pool) => Ok(pool),
    }
}

/// Extension trait that adds query-builder factory methods to
/// `deadpool_postgres::Pool`.
///
/// Import this trait to construct [`Select`], [`Insert`], [`Update`], and
/// [`Delete`] builders directly from a pool reference.
///
/// # Example
///
/// ```rust,ignore
/// use turbograph::db::pool::PoolExt;
///
/// // pool: deadpool_postgres::Pool
/// # async fn example(pool: deadpool_postgres::Pool) {
/// let select = pool.select("users");
/// let insert = pool.insert("users");
/// let update = pool.update("users");
/// let delete = pool.delete("users");
/// # }
/// ```
pub trait PoolExt {
    /// Creates a [`Select`] builder for `table`.
    fn select(&self, table: &str) -> Select;
    /// Creates an [`Insert`] builder for `table`.
    fn insert(&self, table: &str) -> Insert;
    /// Creates an [`Update`] builder for `table`.
    fn update(&self, table: &str) -> Update;
    /// Creates a [`Delete`] builder for `table`.
    fn delete(&self, table: &str) -> Delete;
}

impl PoolExt for Pool {
    fn select(&self, table: &str) -> Select {
        Select::new(table, self.clone())
    }
    fn insert(&self, table: &str) -> Insert {
        Insert::new(table, self.clone())
    }
    fn update(&self, table: &str) -> Update {
        Update::new(table, self.clone())
    }
    fn delete(&self, table: &str) -> Delete {
        Delete::new(table, self.clone())
    }
}
