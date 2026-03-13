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

```bash
cargo add turbograph
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

```rust
use turbograph::{Config, PoolConfig, TurboGraph};

#[tokio::main]
async fn main() {
	let schema = TurboGraph::new(Config {
		pool: PoolConfig::ConnectionString(
			"postgres://postgres:Aa123456@localhost:5432/app-db".into(),
		),
		schemas: vec!["public".into()],
		watch_pg: true,
	})
	.await
	.expect("failed to build schema");

	// Execute GraphQL requests with your transport/framework of choice.
	let _ = schema;
}
```

For a complete HTTP integration with Axum and GraphiQL, see `examples/server/src/main.rs`.

## Request Transaction Context

Turbograph supports per-request transaction settings via `TransactionConfig`.
This is useful for row-level security (RLS), role switching, and session-like values.

```rust
use turbograph::TransactionConfig;

let tx_config = TransactionConfig {
	isolation_level: None,
	read_only: false,
	deferrable: false,
	role: Some("app_user".into()),
	timeout_seconds: None,
	settings: vec![("app.current_user_id".into(), "1".into())],
};
```

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

MIT

## Acknowledgements

Inspired by [PostGraphile](https://www.postgraphile.org/).
