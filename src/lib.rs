//! # Turbograph
//!
//! PostGraphile-style GraphQL API generation for PostgreSQL, built in Rust.
//!
//! Turbograph introspects your PostgreSQL schema and automatically builds a
//! full-featured [`async_graphql`] schema with:
//!
//! - **Queries** — paginated `allXxx` root fields with filtering, ordering, and
//!   cursor-based pagination.
//! - **Mutations** — `createXxx`, `updateXxx`, and `deleteXxx` root fields.
//! - **Per-request transaction settings** — inject a [`TransactionConfig`] into
//!   the GraphQL context to set the role, isolation level, timeouts, and
//!   arbitrary `SET LOCAL` variables for row-level security.
//! - **Live schema reloading** — when `watch_pg` is enabled, the schema is
//!   automatically rebuilt whenever DDL changes are detected.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use turbograph::{Config, PoolConfig, TurboGraph};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//!     let graph = TurboGraph::new(Config {
//!         pool: PoolConfig::ConnectionString(
//!             "postgres://user:pass@localhost:5432/mydb".into(),
//!         ),
//!         schemas: vec!["public".into()],
//!         watch_pg: false,
//!     })
//!     .await?;
//!
//!     // Execute a GraphQL request
//!     let response = graph
//!         .execute(async_graphql::Request::new("{ __typename }"))
//!         .await;
//!     println!("{:?}", response);
//!     Ok(())
//! }
//! ```
//!
//! For a complete HTTP server example using Axum, see the
//! `examples/server` directory in the repository.

mod db;
mod error;
mod graphql;
mod models;
mod schema;
mod utils;

pub use db::error::DbError;
pub use models::config::*;
pub use models::transaction::TransactionConfig;
pub use schema::TurboGraph;

/// Convenience wrapper around [`TurboGraph::new`].
///
/// Builds the GraphQL schema by introspecting the database described by
/// `config`.  Identical to calling `TurboGraph::new(config).await`.
///
/// # Errors
///
/// Returns an error if the database cannot be reached, the pool cannot be
/// created, or schema introspection fails.
///
/// # Example
///
/// ```rust,no_run
/// use turbograph::{build_schema, Config, PoolConfig};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
///     let graph = build_schema(Config {
///         pool: PoolConfig::ConnectionString(
///             "postgres://user:pass@localhost:5432/mydb".into(),
///         ),
///         schemas: vec!["public".into()],
///         watch_pg: false,
///     })
///     .await?;
///     Ok(())
/// }
/// ```
pub async fn build_schema(
    config: Config,
) -> Result<TurboGraph, Box<dyn std::error::Error + Send + Sync>> {
    TurboGraph::new(config).await
}
