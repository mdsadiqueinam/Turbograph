use deadpool_postgres::Pool;

use super::query::{delete::Delete, insert::Insert, select::Select, update::Update};
use crate::models::config::PoolConfig;

/// Resolves a [`PoolConfig`] into a concrete `deadpool_postgres::Pool`.
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

pub trait PoolExt {
    fn select(&self, table: &str) -> Select;
    fn insert(&self, table: &str) -> Insert;
    fn update(&self, table: &str) -> Update;
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
