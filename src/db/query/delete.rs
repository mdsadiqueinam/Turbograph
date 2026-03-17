use crate::db::pool::PoolExt;
use crate::TransactionConfig;
use crate::db::error::DbError;
use deadpool_postgres::Pool;
use tokio_postgres::types::ToSql;
use tokio_postgres::Row;

use crate::db::scalar::SqlScalar;
use crate::db::transaction::execute_query;

use super::{QueryBase, SupportsWhere};

// ── Delete struct ─────────────────────────────────────────────────────────────

pub struct Delete {
    table: String,
    params: Vec<SqlScalar>,
    where_clause: String,
    pool: Pool,
    returning: bool,
}

// ── QueryBase + SupportsWhere ─────────────────────────────────────────────────

impl QueryBase for Delete {
    fn table(&self) -> &str { &self.table }
    fn get_where_clause(&self) -> &str { &self.where_clause }
    fn get_where_clause_mut(&mut self) -> &mut String { &mut self.where_clause }
    fn params(&self) -> &[SqlScalar] { &self.params }
    fn params_mut(&mut self) -> &mut Vec<SqlScalar> { &mut self.params }
    fn pool(&self) -> &Pool { &self.pool }
}

impl SupportsWhere for Delete {}

// ── Constructor & methods ─────────────────────────────────────────────────────

impl Delete {
    pub fn new(table: &str, pool: Pool) -> Self {
        Self {
            table: table.to_string(),
            params: Vec::new(),
            where_clause: String::new(),
            pool,
            returning: false,
        }
    }

    /// Append `RETURNING *` to the generated SQL.
    pub fn returning_all(&mut self) -> &mut Self {
        self.returning = true;
        self
    }

    pub fn where_params(&self) -> Vec<&(dyn ToSql + Sync)> {
        self.params
            .iter()
            .map(|p| p as &(dyn ToSql + Sync))
            .collect()
    }

    pub fn get_query(&self) -> String {
        let mut q = if self.where_clause.is_empty() {
            format!("DELETE FROM {}", self.table)
        } else {
            format!("DELETE FROM {}{}", self.table, self.where_clause)
        };

        if self.returning {
            q.push_str(" RETURNING *");
        }

        q
    }

    pub async fn execute(
        &self,
        tx_config: Option<TransactionConfig>,
    ) -> Result<u64, DbError> {
        let query = self.get_query();
        let params = self.where_params();
        execute_query(&self.pool, &tx_config, &query, &params).await
    }

    /// Execute the query and return rows (for queries with RETURNING *).
    pub async fn execute_with_returning(
        &self,
        tx_config: Option<TransactionConfig>,
    ) -> Result<Vec<Row>, DbError> {
        let client = self.pool
            .get()
            .await
            .map_err(|e| DbError::Pool(e.to_string()))?;

        let query = self.get_query();
        let params = self.where_params();

        let rows = client
            .query(&query, &params)
            .await
            .map_err(|e| DbError::Query(format!("DELETE error: {e}")))?;

        Ok(rows)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::operator::Op;
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
        assert_eq!(q.get_query(), "DELETE FROM users");
    }

    #[test]
    fn test_delete_with_where() {
        let pool = test_pool();
        let mut q = pool.delete("users");
        q.where_clause("id", Op::Eq, Some(SqlScalar::Int4(1)));
        let sql = q.get_query();
        assert!(sql.starts_with("DELETE FROM users"));
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
        assert!(sql.starts_with("DELETE FROM sessions"));
        assert!(sql.contains("WHERE"));
        assert!(sql.contains("("));
        assert!(sql.contains("OR"));
        assert!(sql.contains("AND"));
    }

    #[test]
    fn test_delete_schema_qualified() {
        let pool = test_pool();
        let q = pool.delete("public.logs");
        assert!(q.get_query().contains("public.logs"));
    }
}
