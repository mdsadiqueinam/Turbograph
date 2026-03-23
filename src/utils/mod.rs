//! String inflection utilities used during GraphQL schema generation.
//!
//! Provides conversions between common naming conventions (camelCase,
//! PascalCase, snake_case, SCREAMING_SNAKE_CASE) and English singularisation,
//! used to derive GraphQL type and field names from PostgreSQL table and
//! column names.

pub mod inflection;
