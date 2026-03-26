use crate::TransactionConfig;
use crate::db::error::DbError;
use deadpool_postgres::Pool;
use tokio_postgres::Row;
use tokio_postgres::types::ToSql;

use crate::db::scalar::SqlScalar;
use crate::db::transaction::{execute_query, execute_query_with_returning};

use super::{QueryBase, SupportsWhere};

// ── Delete struct ─────────────────────────────────────────────────────────────

/// SQL `DELETE FROM` query builder.
///
/// Create instances via [`PoolExt::delete`](crate::db::pool::PoolExt::delete).
///
/// Use [`WhereBuilder`](crate::db::where_clause::WhereBuilder) methods to add
/// a `WHERE` clause.
///
/// # Example
///
/// ```rust,ignore
/// use turbograph::db::pool::PoolExt;
/// use turbograph::db::operator::Op;
/// use turbograph::db::scalar::SqlScalar;
/// use turbograph::db::where_clause::WhereBuilder;
///
/// # async fn example(pool: deadpool_postgres::Pool) -> Result<(), turbograph::DbError> {
/// let mut q = pool.delete("users");
/// q.where_clause("id", Op::Eq, Some(SqlScalar::Int4(42)));
/// let affected = q.execute(None).await?;
/// # Ok(()) }
/// ```
pub struct Delete {
    schema: Option<String>,
    table: String,
    params: Vec<SqlScalar>,
    where_clause: String,
    pool: Pool,
}

// ── QueryBase + SupportsWhere ─────────────────────────────────────────────────

impl QueryBase for Delete {
    fn get_where_clause(&self) -> &str {
        &self.where_clause
    }
    fn get_where_clause_mut(&mut self) -> &mut String {
        &mut self.where_clause
    }
    fn params(&self) -> &[SqlScalar] {
        &self.params
    }
    fn params_mut(&mut self) -> &mut Vec<SqlScalar> {
        &mut self.params
    }
}

impl SupportsWhere for Delete {}

// ── Constructor & methods ─────────────────────────────────────────────────────

impl Delete {
    pub fn new(table: &str, pool: Pool) -> Self {
        Self {
            schema: None,
            table: table.to_string(),
            params: Vec::new(),
            where_clause: String::new(),
            pool,
        }
    }

    /// Returns the fully-qualified, quoted table reference: `"schema"."table"` or
    /// just `"table"` if no schema is set.
    fn table_ref(&self) -> String {
        match &self.schema {
            Some(schema) => format!("\"{}\".\"{}\"", schema, self.table),
            None => format!("\"{}\"", self.table),
        }
    }

    /// Set the schema for this query. This allows queries like
    /// `DELETE FROM "schema"."table" ...`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// # use turbograph::db::pool::PoolExt;
    /// # async fn example(pool: deadpool_postgres::Pool) {
    /// let mut q = pool.delete("users").schema("public");
    /// // SQL: DELETE FROM "public"."users" ...
    /// # }
    /// ```
    pub fn schema(mut self, schema: &str) -> Self {
        self.schema = Some(schema.to_string());
        self
    }

    /// Returns the `WHERE`-clause parameters as trait objects.
    pub fn where_params(&self) -> Vec<&(dyn ToSql + Sync)> {
        self.params
            .iter()
            .map(|p| p as &(dyn ToSql + Sync))
            .collect()
    }

    /// Returns the full `DELETE FROM … [WHERE …]` SQL string.
    pub fn get_query(&self) -> String {
        let table_ref = self.table_ref();
        if self.where_clause.is_empty() {
            format!("DELETE FROM {table_ref}")
        } else {
            format!("DELETE FROM {table_ref}{}", self.where_clause)
        }
    }

    /// Execute the delete and return the number of rows affected.
    #[allow(dead_code)]
    pub async fn execute(&self, tx_config: Option<TransactionConfig>) -> Result<u64, DbError> {
        let query = self.get_query();
        let params = self.where_params();
        execute_query(&self.pool, &tx_config, &query, &params).await
    }

    /// Execute the query and return rows (for queries with RETURNING *).
    pub async fn execute_with_returning(
        &self,
        tx_config: Option<TransactionConfig>,
    ) -> Result<Vec<Row>, DbError> {
        let mut query = self.get_query();
        query.push_str(" RETURNING *");
        let params = self.where_params();
        execute_query_with_returning(&self.pool, &tx_config, &query, &params).await
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::operator::Op;
    use crate::db::pool::PoolExt;
    use crate::db::where_clause::WhereBuilder;

    fn test_pool() -> Pool {
        let cfg = deadpool_postgres::Config {
            url: Some("postgres://test:test@localhost/test".to_string()),
            ..Default::default()
        };
        cfg.create_pool(
            Some(deadpool_postgres::Runtime::Tokio1),
            tokio_postgres::NoTls,
        )
        .expect("failed to create test pool")
    }

    #[test]
    fn test_delete_simple() {
        let pool = test_pool();
        let q = pool.delete("users");
        assert_eq!(q.get_query(), "DELETE FROM \"users\"");
    }

    #[test]
    fn test_delete_with_where() {
        let pool = test_pool();
        let mut q = pool.delete("users");
        q.where_clause("id", Op::Eq, Some(SqlScalar::Int4(1)));
        let sql = q.get_query();
        assert!(sql.starts_with("DELETE FROM \"users\""));
        assert!(sql.contains("WHERE"));
        assert!(sql.contains("$1"));
    }

    #[test]
    fn test_delete_with_complex_where() {
        let pool = test_pool();
        let mut q = pool.delete("sessions");
        q.where_block(|q| {
            q.where_clause("id", Op::Eq, Some(SqlScalar::Int4(1)));
            q.or_where_clause("id", Op::Eq, Some(SqlScalar::Int4(2)));
        });
        q.where_clause("status", Op::Eq, Some(SqlScalar::Text("expired".into())));

        let sql = q.get_query();
        assert!(sql.starts_with("DELETE FROM \"sessions\""));
        assert!(sql.contains("WHERE"));
        assert!(sql.contains("("));
        assert!(sql.contains("OR"));
        assert!(sql.contains("AND"));
    }

    #[test]
    fn test_delete_schema_qualified() {
        let pool = test_pool();
        let q = pool.delete("logs").schema("public");
        assert!(q.get_query().contains("\"public\".\"logs\""));
    }
}
