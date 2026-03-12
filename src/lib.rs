mod db;
mod graphql;
mod models;
mod utils;

use std::sync::Arc;

use async_graphql::dynamic::{Object, Schema};
use deadpool_postgres::Pool;

pub use models::config::{Config, PoolConfig, TransactionConfig, TransactionSettingsValue};

/// A receiver for live schema updates when `watch_pg` is enabled.
///
/// Use [`current()`](SchemaWatcher::current) to get the latest schema, or
/// [`next()`](SchemaWatcher::next) to wait for the next DDL-triggered rebuild.
#[derive(Clone)]
pub struct SchemaWatcher {
    rx: tokio::sync::watch::Receiver<Schema>,
}

impl SchemaWatcher {
    /// Returns the latest schema (cheap — clones an internal `Arc`).
    pub fn current(&self) -> Schema {
        self.rx.borrow().clone()
    }

    /// Waits until a DDL change triggers a schema rebuild, then returns the
    /// new schema. Returns `None` if the watcher background task has exited.
    pub async fn next(&mut self) -> Option<Schema> {
        self.rx.changed().await.ok()?;
        Some(self.rx.borrow_and_update().clone())
    }
}

/// Introspects the database described by `config` and returns a fully
/// constructed [`async_graphql::dynamic::Schema`] ready to execute queries.
///
/// When [`Config::watch_pg`] is `true` the function also installs PostgreSQL
/// event triggers and spawns a background listener. The returned
/// [`SchemaWatcher`] yields a new schema whenever a DDL change is detected.
pub async fn build_schema(
    config: Config,
) -> Result<(Schema, Option<SchemaWatcher>), Box<dyn std::error::Error + Send + Sync>> {
    let watch_pg = config.watch_pg;

    // Save the connection URL before consuming config.pool — needed for the
    // dedicated LISTEN connection when watch_pg is enabled.
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

    // ── Resolve pool ────────────────────────────────────────────────────────
    let pool = Arc::new(match config.pool {
        PoolConfig::ConnectionString(url) => {
            let mut cfg = deadpool_postgres::Config::new();
            cfg.url = Some(url);
            cfg.create_pool(
                Some(deadpool_postgres::Runtime::Tokio1),
                tokio_postgres::NoTls,
            )?
        }
        PoolConfig::Pool(pool) => pool,
    });

    // ── Build initial schema ────────────────────────────────────────────────
    let schema = rebuild_schema(&pool, &config.schemas).await?;

    // ── Optionally start DDL watcher ────────────────────────────────────────
    let watcher = if watch_pg {
        let url = connection_url.unwrap();
        db::watch::install_triggers(&pool).await?;

        let (tx, rx) = tokio::sync::watch::channel(schema.clone());
        db::watch::start_watching(url, pool, config.schemas, tx).await?;

        Some(SchemaWatcher { rx })
    } else {
        None
    };

    Ok((schema, watcher))
}

/// Builds a schema from the current database state. Used for the initial build
/// and for automatic rebuilds triggered by DDL changes.
pub(crate) async fn rebuild_schema(
    pool: &Arc<Pool>,
    schemas: &[String],
) -> Result<Schema, Box<dyn std::error::Error + Send + Sync>> {
    let schemas_vec = schemas.to_vec();
    let tables = db::introspect::get_tables(pool, &schemas_vec).await;

    let mut query_root = Object::new("Query");
    let mut builder = Schema::build("Query", None, None);

    // PageInfo is shared across all connection types — register it once.
    builder = builder.register(graphql::make_page_info_type());

    for table in tables {
        if table.omit_read() {
            continue;
        }

        let table = Arc::new(table);
        let entity = graphql::generate_entity(table.clone());
        let gq = graphql::generate_query(table, pool.clone());

        query_root = query_root.field(gq.query_field);
        builder = builder
            .register(entity)
            .register(gq.condition_type)
            .register(gq.order_by_enum)
            .register(gq.connection_type)
            .register(gq.edge_type);

        for ft in gq.condition_filter_types {
            builder = builder.register(ft);
        }
    }

    let schema = builder.register(query_root).finish()?;
    Ok(schema)
}
