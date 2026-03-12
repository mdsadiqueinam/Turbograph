use std::sync::Arc;

use async_graphql::http::GraphiQLSource;
use async_graphql_axum::*;
use axum::{
    Router,
    extract::State,
    response::{Html, IntoResponse},
    routing::get,
};
use tokio::sync::RwLock;
use turbograph::{Config, PoolConfig, build_schema};

type SharedSchema = Arc<RwLock<async_graphql::dynamic::Schema>>;

#[tokio::main]
async fn main() {
    let (schema, watcher) = build_schema(Config {
        pool: PoolConfig::ConnectionString(
            "postgres://postgres:Aa123456@localhost:5432/app-db".into(),
        ),
        schemas: vec!["public".into()],
        watch_pg: true,
    })
    .await
    .expect("failed to build schema");

    let live_schema: SharedSchema = Arc::new(RwLock::new(schema));

    if let Some(mut watcher) = watcher {
        let live = live_schema.clone();
        tokio::spawn(async move {
            while let Some(new_schema) = watcher.next().await {
                println!("Schema updated due to DDL change");
                *live.write().await = new_schema;
            }
        });
    }

    let app = Router::new()
        .route("/graphql", get(graphiql).post(graphql_handler))
        .with_state(live_schema);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:4000").await.unwrap();
    println!("GraphQL playground: http://localhost:4000/graphql");
    axum::serve(listener, app).await.unwrap();
}

async fn graphql_handler(
    State(schema): State<SharedSchema>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    let schema = schema.read().await.clone();
    schema.execute(req.into_inner()).await.into()
}

async fn graphiql() -> impl IntoResponse {
    Html(GraphiQLSource::build().endpoint("/graphql").finish())
}
