# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Turbograph is a PostGraphile-style GraphQL API generator for PostgreSQL, built in Rust. It introspects a PostgreSQL schema and automatically generates a full GraphQL API with queries, mutations, and relational connections using `async-graphql`.

## Build Commands

```bash
# Build the project
cargo build

# Run tests (requires PostgreSQL running with init.sql schema)
cargo test --package turbograph --lib --tests

# Run tests with locked dependencies
cargo test --package turbograph --lib --tests --locked

# Run a single test
cargo test --package turbograph <test_name> -- --nocapture

# Run the example server
docker compose up -d postgres  # Start PostgreSQL first
cargo run --manifest-path examples/server/Cargo.toml
```

## Architecture

### Core Layers

1. **Schema Generation** (`src/schema.rs`): Entry point that builds the GraphQL schema by introspecting the database and generating entity types, queries, and mutations.

2. **Database Layer** (`src/db/`):
   - `introspect.rs`: Reads PostgreSQL schema (tables, columns, types)
   - `query/`: SQL query builders (Select, Insert, Update, Delete) with builder pattern
   - `pool.rs`: Connection pool management with `PoolExt` trait
   - `transaction.rs`: Transaction handling with `TransactionConfig` support
   - `where_clause.rs`: WHERE clause builder with operators

3. **GraphQL Layer** (`src/graphql/`):
   - `entity.rs`: Generates GraphQL types for database tables
   - `query/`: Generates root Query fields and executes SQL
   - `mutation/`: Generates root Mutation fields (create, update, delete)
   - `filter.rs`: Filter input types for queries
   - `type_mapping.rs`: PostgreSQL to GraphQL type mapping

4. **Models** (`src/models/`):
   - `config.rs`: Library configuration (`Config`, `PoolConfig`)
   - `table.rs`: Table/Column metadata from introspection
   - `transaction.rs`: `TransactionConfig` for per-request transaction settings

### Query Execution Flow

GraphQL resolvers (in `src/graphql/query/mod.rs`) extract arguments, build SQL using query builders (`Select`, etc.), and execute with optional `TransactionConfig`:

```rust
// TransactionConfig is passed via GraphQL context
let tx_config = ctx.data_opt::<TransactionConfig>().cloned();
select.execute(tx_config).await
```

### Key Patterns

- **Builder Pattern**: Query builders (`Select`, `Insert`, `Update`, `Delete`) use method chaining
- **Phase Markers**: `Select<NoOrder>` vs `Select<Ordered>` prevents invalid operations
- **SqlScalar**: Type-safe SQL parameter wrapper in `src/db/scalar.rs`
- **PoolExt**: Extension trait on `deadpool_postgres::Pool` adds `.select()`, `.insert()`, etc.
