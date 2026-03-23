# Turbograph

PostGraphile-style GraphQL generation for PostgreSQL, implemented in Rust.

Turbograph introspects your PostgreSQL schema and builds an `async-graphql` schema automatically, so you can ship a production-grade GraphQL API without hand-writing CRUD resolvers.

## Why Turbograph

- PostgreSQL-first approach with schema introspection.
- Rust performance and safety, powered by Tokio.
- GraphQL schema generated from database structure and relationships.
- Optional PostgreSQL watch mode for schema rebuilds on DDL changes.
- Request-level transaction settings (role, isolation, timeout, local settings).

## Installation

Add the crate to your project:

```bash
cargo add turbograph
```

Or add it manually to `Cargo.toml`:

```toml
[dependencies]
turbograph = "0.1"
```

## Quick Start (Example Server)

The repository includes a runnable example server under `examples/server`.

1. Start PostgreSQL:

```bash
docker compose up -d postgres
```

2. Run the example GraphQL server:

```bash
cargo run --manifest-path examples/server/Cargo.toml
```

3. Open GraphiQL:

```text
http://localhost:4000/graphql
```

The sample database schema and seed data are in `db/init.sql`.

## Library Usage

### Minimal setup

```rust
use turbograph::{Config, PoolConfig, TurboGraph};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let graph = TurboGraph::new(Config {
        pool: PoolConfig::ConnectionString(
            "postgres://postgres:secret@localhost:5432/mydb".into(),
        ),
        schemas: vec!["public".into()],
        watch_pg: None,
    })
    .await?;

    // Execute a raw GraphQL request
    let response = graph
        .execute(async_graphql::Request::new("{ __typename }"))
        .await;
    println!("{:?}", response);
    Ok(())
}
```

### Integration with Axum

```rust,no_run
use axum::{Router, extract::State, response::{Html, IntoResponse}, routing::get};
use turbograph::{Config, PoolConfig, TurboGraph, WatchPg};

async fn graphiql() -> Html<String> {
    Html(TurboGraph::graphiql("/graphql"))
}

async fn graphql_handler(
    State(graph): State<TurboGraph>,
    req: axum::extract::Json<async_graphql::Request>,
) -> axum::response::Json<async_graphql::Response> {
    axum::response::Json(graph.execute(req.0).await)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let graph = TurboGraph::new(Config {
        pool: PoolConfig::ConnectionString(
            "postgres://postgres:secret@localhost:5432/mydb".into(),
        ),
        schemas: vec!["public".into()],
        watch_pg: Some(WatchPg("postgres://postgres:secret@localhost:5432/mydb".into())), // rebuild schema on DDL changes
    })
    .await?;

    let app = Router::new()
        .route("/graphql", get(graphiql).post(graphql_handler))
        .with_state(graph);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:4000").await?;
    axum::serve(listener, app).await?;
    Ok(())
}
```

## Generated GraphQL API

For every table or view that Turbograph discovers it generates:

### Queries

```graphql
# List all rows with optional filtering, ordering, and pagination.
allUsers(
  condition: UserCondition   # per-column filter
  orderBy:   [UserOrderBy!]  # e.g. [ID_DESC, NAME_ASC]
  first:     Int             # LIMIT
  offset:    Int             # OFFSET
): UserConnection!

# UserConnection exposes pagination metadata and the rows themselves.
type UserConnection {
  totalCount:    Int!
  pageInfo:      PageInfo!
  edges:         [UserEdge!]!
  nodes:         [User!]!
}

type PageInfo {
  hasNextPage:     Boolean!
  hasPreviousPage: Boolean!
  startCursor:     String
  endCursor:       String
}
```

### Mutations

```graphql
# Insert a single row.
createUser(input: CreateUserInput!): User

# Update rows matching the condition.
updateUser(patch: UpdateUserPatch!, condition: UserCondition): [User!]!

# Delete rows matching the condition.
deleteUser(condition: UserCondition): [User!]!
```

### Filtering

```graphql
query {
  allUsers(
    condition: {
      email: { equal: "alice@example.com" }
      age:   { greaterThanEqual: 18 }
    }
    orderBy: [CREATED_AT_DESC]
    first: 10
    offset: 0
  ) {
    totalCount
    nodes { id name email }
    pageInfo { hasNextPage endCursor }
  }
}
```

Every column filter supports `equal`, `notEqual`, and `in`.
Numeric and date/time columns also support `greaterThan`, `greaterThanEqual`,
`lessThan`, and `lessThanEqual`.

## Controlling Generated Fields with `@omit`

Add `@omit` to a PostgreSQL object comment to suppress specific operations:

```sql
-- Hide all operations on a table (e.g. internal/audit tables).
COMMENT ON TABLE audit_log IS '@omit';

-- Suppress only write mutations; the table is still queryable.
COMMENT ON TABLE view_only IS '@omit create,update,delete';

-- Hide a column from the API (e.g. a password hash).
COMMENT ON COLUMN users.password_hash IS '@omit';
```

Materialized views automatically suppress create, update, and delete.

## Request Transaction Context

Turbograph supports per-request transaction settings via `TransactionConfig`.
This is useful for row-level security (RLS), role switching, and session-like
values forwarded to PostgreSQL functions.

```rust
use turbograph::TransactionConfig;

let tx_config = TransactionConfig {
    role: Some("app_user".into()),
    settings: vec![
        ("app.current_user_id".into(), "42".into()),
        ("app.tenant_id".into(), "acme".into()),
    ],
    read_only: false,
    deferrable: false,
    isolation_level: None,
    timeout_seconds: Some(5),
};
```

Inject it into the GraphQL request so Turbograph picks it up automatically:

```rust,no_run
use turbograph::{TransactionConfig, TurboGraph};

async fn handle(graph: &TurboGraph, query: &str, user_id: i64) {
    let tx = TransactionConfig {
        role: Some("app_user".into()),
        settings: vec![("app.current_user_id".into(), user_id.to_string())],
        ..TransactionConfig::default()
    };
    let request = async_graphql::Request::new(query).data(tx);
    let response = graph.execute(request).await;
    println!("{:?}", response);
}
```

Any PostgreSQL row-level security policy that reads `current_setting('app.current_user_id')`
will automatically see the value you set.

## Release Process

Crates.io publishing is automated with GitHub Actions.

- Publishing runs when a GitHub Release is published.
- The release tag (for example `v0.1.0`) must match the crate version in `Cargo.toml`.
- The workflow performs `cargo publish --dry-run` before publishing.

Required repository secret:

- `CARGO_REGISTRY_TOKEN`

## Project Status

Turbograph is in early development and already supports:

- Database introspection and GraphQL schema generation.
- Query and mutation execution.
- Row-level security patterns through transaction-scoped context.
- Example server and integration tests.

Planned improvements include broader PostgreSQL feature coverage, richer filtering/ordering support, and more extension hooks.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgements

Inspired by [PostGraphile](https://www.postgraphile.org/).
