use crate::TransactionConfig;
use deadpool_postgres::Pool;
use tokio_postgres::types::ToSql;

use crate::db::scalar::SqlScalar;
use crate::db::transaction::{apply_settings, build_begin_statement};

use super::{QueryBase, SupportsWhere};

// ── Delete struct ─────────────────────────────────────────────────────────────

pub struct Delete {
    table: String,
    params: Vec<Option<SqlScalar>>,
    where_clause: String,
    pool: Pool,
}

// ── QueryBase + SupportsWhere ─────────────────────────────────────────────────

impl QueryBase for Delete {
    fn table(&self) -> &str { &self.table }
    fn get_where_clause(&self) -> &str { &self.where_clause }
    fn get_where_clause_mut(&mut self) -> &mut String { &mut self.where_clause }
    fn params(&self) -> &[Option<SqlScalar>] { &self.params }
    fn params_mut(&mut self) -> &mut Vec<Option<SqlScalar>> { &mut self.params }
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
        }
    }

    fn where_params(&self) -> Vec<&(dyn ToSql + Sync)> {
        self.params
            .iter()
            .map(|p| p as &(dyn ToSql + Sync))
            .collect()
    }

    pub fn get_query(&self) -> String {
        if self.where_clause.is_empty() {
            format!("DELETE FROM {}", self.table)
        } else {
            format!("DELETE FROM {}{}", self.table, self.where_clause)
        }
    }

    pub async fn execute(
        &self,
        tx_config: &Option<TransactionConfig>,
    ) -> Result<u64, async_graphql::Error> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| async_graphql::Error::new(format!("Pool error: {e}")))?;

        let begin = build_begin_statement(tx_config);
        client
            .batch_execute(&begin)
            .await
            .map_err(|e| async_graphql::Error::new(format!("BEGIN error: {e}")))?;

        if let Some(cfg) = tx_config {
            apply_settings(&*client, cfg).await?;
        }

        let query = self.get_query();
        let params = self.where_params();

        let result = client
            .execute(&query, &params)
            .await
            .map_err(|e| async_graphql::Error::new(format!("DB query error: {e}")));

        match &result {
            Ok(_) => {
                client
                    .batch_execute("COMMIT")
                    .await
                    .map_err(|e| async_graphql::Error::new(format!("COMMIT error: {e}")))?;
            }
            Err(_) => {
                let _ = client.batch_execute("ROLLBACK").await;
            }
        }

        result
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
        let q = Delete::new("users", test_pool());
        assert_eq!(q.get_query(), "DELETE FROM users");
    }

    #[test]
    fn test_delete_with_where() {
        let mut q = Delete::new("users", test_pool());
        q.where_clause("id", Op::Eq, Some(SqlScalar::Int4(1)));
        let sql = q.get_query();
        assert!(sql.starts_with("DELETE FROM users"));
        assert!(sql.contains("WHERE"));
        assert!(sql.contains("$1"));
    }

    #[test]
    fn test_delete_with_complex_where() {
        let mut q = Delete::new("sessions", test_pool());
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
        let q = Delete::new("public.logs", test_pool());
        assert!(q.get_query().contains("public.logs"));
    }
}
