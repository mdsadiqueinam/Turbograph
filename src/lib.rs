mod db;
mod error;
mod graphql;
mod models;
mod schema;
mod utils;

use std::sync::Arc;

pub use models::config::{Config, PoolConfig};
pub use models::transaction::{TransactionConfig, TransactionSettingsValue};
pub use schema::SchemaWatcher;

/// Introspects the database described by `config` and returns a fully
/// constructed GraphQL schema ready to execute queries.
///
/// When [`Config::watch_pg`] is `true`, event triggers are installed and a
/// background listener is spawned. The returned [`SchemaWatcher`] yields a new
/// schema whenever a DDL change is detected.
pub async fn build_schema(
    config: Config,
) -> Result<
    (async_graphql::dynamic::Schema, Option<SchemaWatcher>),
    Box<dyn std::error::Error + Send + Sync>,
> {
    let watch_pg = config.watch_pg;

    let connection_url = if watch_pg {
        match &config.pool {
            PoolConfig::ConnectionString(url) => Some(url.clone()),
            PoolConfig::Pool(_) => {
                return Err("watch_pg requires PoolConfig::ConnectionString".into());
            }
        }
    } else {
        None
    };

    let pool = Arc::new(db::pool::resolve(config.pool)?);
    let built_schema = schema::rebuild_schema(&pool, &config.schemas).await?;

    let watcher = if watch_pg {
        let url = connection_url.unwrap();
        db::watch::install_triggers(&pool).await?;

        let (tx, rx) = tokio::sync::watch::channel(built_schema.clone());
        db::watch::start_watching(url, pool, config.schemas, tx).await?;

        Some(SchemaWatcher::new(rx))
    } else {
        None
    };

    Ok((built_schema, watcher))
}
