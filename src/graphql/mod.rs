//! GraphQL schema generation layer.
//!
//! Assembles the `async_graphql` dynamic schema from introspected
//! [`Table`](crate::models::table::Table) metadata.  The sub-modules handle:
//!
//! - [`entity`] — object types (one per table/view)
//! - [`query`] — root `Query` fields (`allXxx`)
//! - [`mutation`] — root `Mutation` fields (`createXxx`, `updateXxx`, `deleteXxx`)
//! - [`connection`] — shared `PageInfo` type and cursor encoding
//! - [`filter`] — helpers for determining which column types support range operators
//! - [`type_mapping`] — mapping between PostgreSQL types and GraphQL scalars

mod connection;
mod entity;
mod filter;
pub(crate) mod mutation;
pub(crate) mod query;
mod type_mapping;

pub(crate) use connection::make_page_info_type;
pub(crate) use entity::generate_entity;
pub(crate) use mutation::generate_mutation;
pub(crate) use query::generate_query;
pub(crate) use type_mapping::*;
