use std::sync::Arc;

use async_graphql::dynamic::{Object, Schema};
use deadpool_postgres::Pool;
use tokio::sync::RwLock;

use crate::graphql;
use crate::models::config::{Config, PoolConfig};
use crate::models::table::Table;

/// The main entry point for consuming the library.
///
/// `TurboGraph` wraps the dynamically-built GraphQL schema and, when
/// `watch_pg` is enabled, transparently handles live-reloading in the
/// background. It is cheaply cloneable (backed by `Arc`) and can be
/// used as shared state in any async web framework (axum, actix-web,
/// poem, etc.).
///
/// # Example (axum)
///
/// ```rust,ignore
/// let server = TurboGraph::new(config).await?;
/// let app = Router::new()
///     .route("/graphql", get(graphiql).post(handler))
///     .with_state(server);
/// ```
#[derive(Clone)]
pub struct TurboGraph {
    schema: Arc<RwLock<Schema>>,
}

impl TurboGraph {
    /// Build the GraphQL schema from the database described by `config`.
    ///
    /// When [`Config::watch_pg`] is `true`, event triggers are installed and a
    /// background task is spawned that automatically swaps in a freshly built
    /// schema whenever a DDL change is detected.
    pub async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
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

        let pool = Arc::new(crate::db::pool::resolve(config.pool)?);
        let built_schema = rebuild_schema(&pool, &config.schemas).await?;
        let schema = Arc::new(RwLock::new(built_schema));

        if watch_pg {
            let url = connection_url.unwrap();
            crate::db::watch::install_triggers(&pool).await?;
            crate::db::watch::start_watching(url, pool, config.schemas, schema.clone()).await?;
        }

        Ok(Self { schema })
    }

    /// Execute a GraphQL request against the current schema.
    pub async fn execute(&self, request: async_graphql::Request) -> async_graphql::Response {
        // SAFETY: The schema is only swapped out in its entirety after a fresh build completes,
        // so there are no concerns about concurrent mutation. Readers will always see a consistent schema,
        // albeit possibly an older one if a rebuild is in progress.
        let schema = self.schema.read().await;
        schema.execute(request).await
    }

    /// Returns the GraphiQL HTML page pointing at the given `endpoint`.
    pub fn graphiql(endpoint: &str) -> String {
        async_graphql::http::GraphiQLSource::build()
            .endpoint(endpoint)
            .finish()
    }

    /// Returns a clone of the current underlying dynamic schema.
    pub async fn schema(&self) -> Schema {
        self.schema.read().await.clone()
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
    let tables: Vec<Arc<Table>> = tables.into_iter().map(|t| Arc::new(t)).collect();

    let mut query_root = Object::new("Query");
    let mut mutation_root = Object::new("Mutation");

    // First pass: collect entity, query, and mutation artefacts per table.
    struct TableArtefacts {
        entity: Object,
        query: async_graphql::dynamic::Field,
        mutation: Option<crate::graphql::mutation::GeneratedMutation>,
    }

    let mut artefacts = Vec::new();

    for table in tables.iter() {
        if table.omit_read() {
            continue;
        }

        let entity = graphql::generate_entity(table.clone());
        let gq = graphql::generate_query(table.clone(), pool.clone());
        let gm = if !table.omit_create() || !table.omit_update() || !table.omit_delete() {
            Some(graphql::generate_mutation(table.clone(), pool.clone()))
        } else {
            None
        };

        artefacts.push(TableArtefacts {
            entity,
            query: gq,
            mutation: gm,
        });
    }

    let has_mutations = artefacts
        .iter()
        .any(|a| a.mutation.as_ref().is_some_and(|m| !m.fields.is_empty()));

    let mut builder = Schema::build(
        "Query",
        if has_mutations {
            Some("Mutation")
        } else {
            None
        },
        None,
    );

    builder = builder.register(graphql::make_page_info_type());

    for table in tables.iter() {
        builder = builder
            .register(table.condition_type())
            .register(table.order_by_enum())
            .register(table.connection_type())
            .register(table.edge_type());

        for ft in table.condition_filter_types() {
            builder = builder.register(ft);
        }
    }

    for a in artefacts {
        query_root = query_root.field(a.query);
        builder = builder.register(a.entity);

        if let Some(gm) = a.mutation {
            for field in gm.fields {
                mutation_root = mutation_root.field(field);
            }
            for input in gm.input_objects {
                builder = builder.register(input);
            }
        }
    }

    builder = builder.register(query_root);
    if has_mutations {
        builder = builder.register(mutation_root);
    }

    let schema = builder.finish()?;
    Ok(schema)
}
