//! SQL query builder types used by the GraphQL resolvers.
//!
//! Each builder (`Select`, `Insert`, `Update`, `Delete`) follows a fluent
//! interface and constructs parameterised SQL that is executed via
//! `tokio_postgres`.
//!
//! Builders are created through [`PoolExt`](crate::db::pool::PoolExt) factory
//! methods on a `deadpool_postgres::Pool`.

pub mod delete;
pub mod insert;
pub mod select;
pub mod update;

use super::scalar::SqlScalar;
use super::where_clause::WhereInternal;

// ── Shared traits ─────────────────────────────────────────────────────────────

/// Common accessors shared by all query builder structs.
///
/// This is a sealed trait used internally to provide the blanket
/// [`WhereInternal`] implementation.  It is not intended for use outside this
/// crate.
pub(super) trait QueryBase {
    fn get_where_clause(&self) -> &str;
    fn get_where_clause_mut(&mut self) -> &mut String;
    fn params(&self) -> &[SqlScalar];
    fn params_mut(&mut self) -> &mut Vec<SqlScalar>;
}

/// Marker trait that opts a query builder into [`WhereBuilder`](crate::db::where_clause::WhereBuilder) support.
///
/// Implemented by [`Select<NoOrder>`](select::Select), [`Delete`](delete::Delete),
/// and [`Update`](update::Update).  **Not** implemented by [`Insert`](insert::Insert)
/// (inserts have no `WHERE` clause) or [`Select<Ordered>`](select::Select)
/// (ordering locks out further `WHERE` mutations).
pub(super) trait SupportsWhere: QueryBase {}

// ── Blanket WhereInternal for any type that opts into SupportsWhere ────────────

impl<T: SupportsWhere> WhereInternal for T {
    fn get_query(&self) -> &str {
        self.get_where_clause()
    }
    fn push_to_query(&mut self, q: String) {
        self.get_where_clause_mut().push_str(&q);
    }
    fn push_param(&mut self, scalar: SqlScalar) -> usize {
        self.params_mut().push(scalar);
        self.params().len()
    }
}
