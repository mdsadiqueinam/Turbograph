pub mod delete;
pub mod insert;
pub mod select;
pub mod update;

use deadpool_postgres::Pool;

use super::scalar::SqlScalar;
use super::where_clause::WhereInternal;

// ── Shared traits ─────────────────────────────────────────────────────────────

/// Common accessors for all query builder structs.
pub(super) trait QueryBase {
    fn table(&self) -> &str;
    fn get_where_clause(&self) -> &str;
    fn get_where_clause_mut(&mut self) -> &mut String;
    fn params(&self) -> &[SqlScalar];
    fn params_mut(&mut self) -> &mut Vec<SqlScalar>;
    fn pool(&self) -> &Pool;
}

/// Marker trait: opt-in for WHERE clause support.
/// Implemented by `Select<NoOrder>`, `Delete`, `Update` — but NOT `Insert` or `Select<Ordered>`.
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
