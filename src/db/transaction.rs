use std::fmt::Write;
use std::future::Future;
use std::pin::Pin;

use deadpool_postgres::Pool;
use tokio_postgres::types::ToSql;

use crate::db::error::DbError;
use crate::models::transaction::TransactionConfig;

/// Executes a query (INSERT, UPDATE, DELETE) within a transaction.
/// Returns the number of affected rows.
pub async fn execute_query(
    pool: &Pool,
    tx_config: &Option<TransactionConfig>,
    query: &str,
    params: &[&(dyn ToSql + Sync)],
) -> Result<u64, DbError> {
    let client = pool.get().await.map_err(|e| DbError::Pool(e.to_string()))?;

    let begin = build_begin_statement(tx_config);
    client
        .batch_execute(&begin)
        .await
        .map_err(|e| DbError::Transaction(format!("BEGIN error: {e}")))?;

    if let Some(cfg) = tx_config.as_ref() {
        apply_settings(&*client, cfg).await?;
    }

    let result = client
        .execute(query, params)
        .await
        .map_err(|e| DbError::Query(e.to_string()));

    match &result {
        Ok(_) => {
            client
                .batch_execute("COMMIT")
                .await
                .map_err(|e| DbError::Transaction(format!("COMMIT error: {e}")))?;
        }
        Err(_) => {
            let _ = client.batch_execute("ROLLBACK").await;
        }
    }

    result
}

/// Executes a query (INSERT, UPDATE, DELETE with RETURNING) within a transaction.
/// Returns the rows from the RETURNING clause.
pub async fn execute_query_with_returning(
    pool: &Pool,
    tx_config: &Option<TransactionConfig>,
    query: &str,
    params: &[&(dyn ToSql + Sync)],
) -> Result<Vec<tokio_postgres::Row>, DbError> {
    let client = pool.get().await.map_err(|e| DbError::Pool(e.to_string()))?;

    let begin = build_begin_statement(tx_config);
    client
        .batch_execute(&begin)
        .await
        .map_err(|e| DbError::Transaction(format!("BEGIN error: {e}")))?;

    if let Some(cfg) = tx_config.as_ref() {
        apply_settings(&*client, cfg).await?;
    }

    let result = client
        .query(query, params)
        .await
        .map_err(|e| DbError::Query(e.to_string()));

    match &result {
        Ok(_) => {
            client
                .batch_execute("COMMIT")
                .await
                .map_err(|e| DbError::Transaction(format!("COMMIT error: {e}")))?;
        }
        Err(_) => {
            let _ = client.batch_execute("ROLLBACK").await;
        }
    }

    result
}

pub(super) fn build_begin_statement(tx_config: &Option<TransactionConfig>) -> String {
    let mut begin = String::from("BEGIN");
    if let Some(cfg) = tx_config {
        if let Some(level) = cfg.isolation_level {
            let lvl_str = match level {
                tokio_postgres::IsolationLevel::ReadUncommitted => "READ UNCOMMITTED",
                tokio_postgres::IsolationLevel::ReadCommitted => "READ COMMITTED",
                tokio_postgres::IsolationLevel::RepeatableRead => "REPEATABLE READ",
                tokio_postgres::IsolationLevel::Serializable => "SERIALIZABLE",
                _ => "READ COMMITTED",
            };
            write!(begin, " ISOLATION LEVEL {lvl_str}").unwrap();
        }
        if cfg.read_only {
            begin.push_str(" READ ONLY");
        }
        if cfg.deferrable {
            begin.push_str(" DEFERRABLE");
        }
    }
    begin
}

/// Applies `SET LOCAL` directives (role, custom settings, timeout) inside
/// an already-open transaction.
pub(super) async fn apply_settings(
    client: &tokio_postgres::Client,
    cfg: &TransactionConfig,
) -> Result<(), DbError> {
    if let Some(ref role) = cfg.role {
        client
            .query("SELECT set_config('role', $1, true)", &[role])
            .await
            .map_err(|e| DbError::Transaction(format!("SET ROLE error: {e}")))?;
    }

    for (key, val) in &cfg.settings {
        client
            .query("SELECT set_config($1, $2, true)", &[key, val])
            .await
            .map_err(|e| DbError::Transaction(format!("set_config error: {e}")))?;
    }

    if let Some(secs) = cfg.timeout_seconds {
        let ms = (secs * 1000).to_string();
        client
            .query("SELECT set_config('statement_timeout', $1, true)", &[&ms])
            .await
            .map_err(|e| DbError::Transaction(format!("SET timeout error: {e}")))?;
    }

    Ok(())
}
