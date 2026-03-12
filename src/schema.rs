use std::sync::Arc;

use async_graphql::dynamic::{Object, Schema};
use deadpool_postgres::Pool;

use crate::graphql;

/// A receiver for live schema updates when `watch_pg` is enabled.
///
/// Use [`current()`](SchemaWatcher::current) to get the latest schema, or
/// [`next()`](SchemaWatcher::next) to wait for the next DDL-triggered rebuild.
#[derive(Clone)]
pub struct SchemaWatcher {
    rx: tokio::sync::watch::Receiver<Schema>,
}

impl SchemaWatcher {
    pub(crate) fn new(rx: tokio::sync::watch::Receiver<Schema>) -> Self {
        Self { rx }
    }

    /// Returns the latest schema.
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

/// Builds a schema from the current database state.
///
/// Used for the initial build and for automatic rebuilds triggered by DDL
/// changes.
pub(crate) async fn rebuild_schema(
    pool: &Arc<Pool>,
    schemas: &[String],
) -> Result<Schema, Box<dyn std::error::Error + Send + Sync>> {
    let tables = crate::db::introspect::get_tables(pool, schemas).await;

    let mut query_root = Object::new("Query");
    let mut builder = Schema::build("Query", None, None);

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
