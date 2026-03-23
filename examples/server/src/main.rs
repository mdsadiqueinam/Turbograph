use async_graphql_axum::*;
use axum::{
    Router,
    extract::State,
    response::{Html, IntoResponse},
    routing::get,
};
use turbograph::{Config, TransactionConfig, TurboGraph};

#[tokio::main]
async fn main() {
    let server = TurboGraph::new(
        Config::default()
            .connection_string("postgres://postgres:Aa123456@localhost:5432/app-db")
            .schema("public")
            .watch_pg("postgres://postgres:Aa123456@localhost:5432/app-db"),
    )
    .await
    .expect("failed to build schema");

    let app = Router::new()
        .route("/graphql", get(graphiql).post(graphql_handler))
        .with_state(server);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:4000").await.unwrap();
    println!("GraphQL playground: http://localhost:4000/graphql");
    axum::serve(listener, app).await.unwrap();
}

async fn graphql_handler(State(server): State<TurboGraph>, req: GraphQLRequest) -> GraphQLResponse {
    let tx_config = TransactionConfig::default()
        .role("app_user")
        .setting("app.current_user_id", "1");
    server
        .execute(req.into_inner().data(tx_config))
        .await
        .into()
}

async fn graphiql() -> impl IntoResponse {
    Html(TurboGraph::graphiql("/graphql"))
}
