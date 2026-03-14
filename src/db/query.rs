use crate::TransactionConfig;
use deadpool_postgres::Pool;
use tokio_postgres::types::ToSql;

use super::scalar::SqlScalar;
use super::transaction::{apply_settings, build_begin_statement};
use super::where_clause::WhereInternal;

// ── Mode markers ─────────────────────────────────────────────────────────────

pub struct MutationMode;
pub struct SelectMode;

// ── Order-phase markers ───────────────────────────────────────────────────────

/// The query has not yet had ORDER BY applied – WHERE clauses are still allowed.
pub struct NoOrder;
/// ORDER BY has been applied; only `.execute()` is legal now.
pub struct Ordered;

// ── Query struct ──────────────────────────────────────────────────────────────

pub struct Query<M, O = NoOrder> {
    query: String,
    params: Vec<Option<SqlScalar>>,
    has_where: bool,
    pool: Pool,
    limit: Option<usize>,
    offset: Option<usize>,
    orders: Vec<(String, OrderDirection)>,
    _mode: std::marker::PhantomData<M>,
    _order: std::marker::PhantomData<O>,
}

// ── Internal helpers (available to both modes / both order phases) ─────────────

impl<M, O> Query<M, O> {
    fn new(base_sql: String, pool: Pool) -> Self {
        Self {
            query: base_sql,
            params: Vec::new(),
            pool,
            has_where: false,
            limit: None,
            offset: None,
            orders: Vec::new(),
            _mode: std::marker::PhantomData,
            _order: std::marker::PhantomData,
        }
    }

    /// Transition into a different order-phase without copying any data.
    fn into_phase<O2>(self) -> Query<M, O2> {
        Query {
            query: self.query,
            params: self.params,
            has_where: self.has_where,
            pool: self.pool,
            limit: self.limit,
            offset: self.offset,
            orders: self.orders,
            _mode: std::marker::PhantomData,
            _order: std::marker::PhantomData,
        }
    }

    fn count_params(&self) -> Vec<&(dyn ToSql + Sync)> {
        self.params
            .iter()
            .map(|p| p as &(dyn ToSql + Sync))
            .collect()
    }
}

// ── execute is available in all states ────────────────────────────────────────

impl<M, O> Query<M, O> {
    pub async fn execute(
        &self,
        tx_config: &Option<TransactionConfig>,
    ) -> Result<(), async_graphql::Error> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| async_graphql::Error::new(format!("Pool error: {e}")))?;

        let begin = build_begin_statement(tx_config);
        client
            .batch_execute(&begin)
            .await
            .map_err(|e| format!("BEGIN error: {e}"))?;

        if let Some(cfg) = tx_config {
            apply_settings(&*client, cfg).await?;
        }

        // let params = self.data_params();

        // match &result {
        //     Ok(_) => {
        //         client
        //             .batch_execute("COMMIT")
        //             .await
        //             .map_err(|e| format!("COMMIT error: {e}"))?;
        //     }
        //     Err(_) => {
        //         let _ = client.batch_execute("ROLLBACK").await;
        //     }
        // }

        // result

        Ok(())
    }
}

// ── WhereInternal (internal plumbing, both modes, NoOrder only) ───────────────

impl<M> WhereInternal for Query<M, NoOrder> {
    fn get_has_where(&self) -> bool {
        self.has_where
    }
    fn set_has_where(&mut self, val: bool) {
        self.has_where = val;
    }
    fn get_query(&self) -> &str {
        &self.query
    }
    fn push_to_query(&mut self, q: String) {
        self.query.push_str(&q);
    }
    fn push_param(&mut self, scalar: Option<SqlScalar>) -> usize {
        self.params.push(scalar);
        self.params.len()
    }
}

// ── order_by is only available on SELECT queries that haven't been ordered ────

impl Query<SelectMode, NoOrder> {
    /// Apply ORDER BY and advance to the `Ordered` phase.
    /// After this call only `.execute()` is available – WHERE clauses are locked out.
    pub fn order_by(
        mut self,
        column: &str,
        direction: OrderDirection,
    ) -> Query<SelectMode, Ordered> {
        self.orders.push((column.to_string(), direction));
        // old phase will drop here
        self.into_phase()
    }
}

// ── ORDER BY direction ────────────────────────────────────────────────────────

pub enum OrderDirection {
    Asc,
    Desc,
}

impl OrderDirection {
    fn as_str(&self) -> &'static str {
        match self {
            OrderDirection::Asc => "ASC",
            OrderDirection::Desc => "DESC",
        }
    }
}

// ── Constructors (one per mode) ───────────────────────────────────────────────

impl Query<SelectMode, NoOrder> {
    pub fn select(table: &str, pool: Pool) -> Self {
        Self::new(format!("SELECT * FROM {table}"), pool)
    }
}

impl Query<MutationMode, NoOrder> {
    pub fn mutation(sql: String, pool: Pool) -> Self {
        Self::new(sql, pool)
    }
}
