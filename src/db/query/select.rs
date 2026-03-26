use std::fmt::Write;
use std::marker::PhantomData;

use crate::TransactionConfig;
use crate::db::JsonListExt;
use crate::db::error::DbError;
use deadpool_postgres::Pool;
use tokio_postgres::types::ToSql;

use crate::db::scalar::SqlScalar;
use crate::db::transaction::{apply_settings, build_begin_statement};

use super::{QueryBase, SupportsWhere};

/// Wraps a column name in double quotes for PostgreSQL identifier quoting.
#[inline]
fn quote_ident(name: &str) -> String {
    format!("\"{name}\"")
}

// ── Order-phase markers ───────────────────────────────────────────────────────

/// Type-state marker indicating that no `ORDER BY` has been applied yet.
///
/// In this phase [`WhereBuilder`](crate::db::where_clause::WhereBuilder)
/// methods are available on the `Select`.
pub struct NoOrder;

/// Type-state marker indicating that at least one `ORDER BY` clause has been
/// applied.
///
/// Once a `Select` transitions to `Ordered`, `WHERE` mutations are locked
/// out at compile time to prevent accidentally appending conditions after the
/// order columns have been committed.  `limit`, `offset`, and additional
/// `order_by` calls are still available.
pub struct Ordered;

// ── ORDER BY direction ────────────────────────────────────────────────────────

/// Sort direction for an `ORDER BY` column.
pub enum OrderDirection {
    /// Ascending order (`ASC`).
    Asc,
    /// Descending order (`DESC`).
    Desc,
}

impl OrderDirection {
    /// Returns the SQL keyword for this direction (`"ASC"` or `"DESC"`).
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderDirection::Asc => "ASC",
            OrderDirection::Desc => "DESC",
        }
    }
}

// ── Select struct ─────────────────────────────────────────────────────────────

/// SQL `SELECT` query builder.
///
/// Create instances via [`PoolExt::select`](crate::db::pool::PoolExt::select).
///
/// The type parameter `O` is a compile-time phase marker:
/// - [`Select<NoOrder>`] — `WHERE` clauses are permitted.
/// - [`Select<Ordered>`] — transitions after the first [`order_by`] call;
///   `WHERE` mutations are no longer allowed.
///
/// # Example
///
/// ```rust,ignore
/// use turbograph::db::pool::PoolExt;
/// use turbograph::db::operator::Op;
/// use turbograph::db::scalar::SqlScalar;
/// use turbograph::db::where_clause::WhereBuilder;
/// use turbograph::db::query::select::OrderDirection;
///
/// # async fn example(pool: deadpool_postgres::Pool) -> Result<(), turbograph::DbError> {
/// let mut q = pool.select("users");
/// q.where_clause("active", Op::Eq, Some(SqlScalar::Bool(true)));
/// let q = q.order_by("created_at", OrderDirection::Desc).limit(20);
/// let (total, rows) = q.execute(None).await?;
/// # Ok(()) }
/// ```
pub struct Select<O = NoOrder> {
    schema: Option<String>,
    table: String,
    params: Vec<SqlScalar>,
    where_clause: String,
    pool: Pool,
    limit: Option<SqlScalar>,
    offset: Option<SqlScalar>,
    orders: Vec<(String, OrderDirection)>,
    _order: PhantomData<O>,
}

// ── QueryBase ─────────────────────────────────────────────────────────────────

impl<O> QueryBase for Select<O> {
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

// Only NoOrder gets WHERE support
impl SupportsWhere for Select<NoOrder> {}

// ── Constructor ───────────────────────────────────────────────────────────────

impl Select<NoOrder> {
    pub fn new(table: &str, pool: Pool) -> Self {
        Self {
            schema: None,
            table: table.to_string(),
            params: Vec::new(),
            where_clause: String::new(),
            pool,
            limit: None,
            offset: None,
            orders: Vec::new(),
            _order: PhantomData,
        }
    }
}

// ── Methods available in ANY order phase ──────────────────────────────────────

impl<O> Select<O> {
    /// Transition into a different order-phase without copying data.
    fn into_phase<O2>(self) -> Select<O2> {
        Select {
            schema: self.schema,
            table: self.table,
            params: self.params,
            where_clause: self.where_clause,
            pool: self.pool,
            limit: self.limit,
            offset: self.offset,
            orders: self.orders,
            _order: PhantomData,
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
    /// `SELECT * FROM "schema"."table"`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// # use turbograph::db::pool::PoolExt;
    /// # async fn example(pool: deadpool_postgres::Pool) {
    /// let q = pool.select("users").schema("public");
    /// // SQL: SELECT * FROM "public"."users"
    /// # }
    /// ```
    pub fn schema(mut self, schema: &str) -> Self {
        self.schema = Some(schema.to_string());
        self
    }

    /// Returns the `WHERE`-clause parameters as trait objects suitable for
    /// passing directly to `tokio_postgres`.
    pub fn where_params(&self) -> Vec<&(dyn ToSql + Sync)> {
        self.params
            .iter()
            .map(|p| p as &(dyn ToSql + Sync))
            .collect()
    }

    /// Returns all parameters for the full `SELECT` query: `WHERE` params
    /// followed by the optional `LIMIT` and `OFFSET` params.
    pub fn select_params(&self) -> Vec<&(dyn ToSql + Sync)> {
        let mut params = self.where_params();
        if let Some(limit) = &self.limit {
            params.push(limit as &(dyn ToSql + Sync));
        }
        if let Some(offset) = &self.offset {
            params.push(offset as &(dyn ToSql + Sync));
        }
        params
    }

    /// Set the maximum number of rows to return (`LIMIT $n`).
    pub fn limit(mut self, limit: i64) -> Self {
        self.limit = Some(SqlScalar::Int8(limit));
        self
    }

    /// Skip the first `offset` rows (`OFFSET $n`).
    pub fn offset(mut self, offset: i64) -> Self {
        self.offset = Some(SqlScalar::Int8(offset));
        self
    }

    /// Append an `ORDER BY column direction` clause and transition to
    /// [`Select<Ordered>`].
    ///
    /// After this call, `WHERE` mutations are no longer available at the
    /// type level, but `order_by`, `limit`, and `offset` remain usable.
    pub fn order_by(mut self, column: &str, direction: OrderDirection) -> Select<Ordered> {
        self.orders.push((quote_ident(column), direction));
        self.into_phase()
    }

    /// Returns the `SELECT COUNT(*) FROM …` SQL string (with any `WHERE`
    /// clause appended).  Used internally to fetch the total row count in
    /// parallel with the data query.
    pub fn get_count_query(&self) -> String {
        let table_ref = self.table_ref();
        if self.where_clause.is_empty() {
            format!("SELECT COUNT(*) FROM {table_ref}")
        } else {
            format!("SELECT COUNT(*) FROM {table_ref}{}", self.where_clause)
        }
    }

    /// Returns the `ORDER BY …` fragment, or an empty string when no ordering
    /// has been applied.
    pub fn get_order_clause(&self) -> String {
        if self.orders.is_empty() {
            String::new()
        } else {
            let parts: Vec<String> = self
                .orders
                .iter()
                .map(|(col, dir)| format!("{col} {}", dir.as_str()))
                .collect();
            format!(" ORDER BY {}", parts.join(", "))
        }
    }

    /// Returns the full `SELECT * FROM … [WHERE …] [ORDER BY …] [LIMIT $n] [OFFSET $n]`
    /// SQL string.
    pub fn get_select_query(&self) -> String {
        let table_ref = self.table_ref();
        let mut q = if self.where_clause.is_empty() {
            format!("SELECT * FROM {table_ref}")
        } else {
            format!("SELECT * FROM {table_ref}{}", self.where_clause)
        };

        let order = self.get_order_clause();
        if !order.is_empty() {
            write!(q, "{order}").expect("write to String cannot fail");
        }

        let mut next_param = self.params.len() + 1;
        if self.limit.is_some() {
            write!(q, " LIMIT ${next_param}").expect("write to String cannot fail");
            next_param += 1;
        }
        if self.offset.is_some() {
            write!(q, " OFFSET ${next_param}").expect("write to String cannot fail");
        }
        q
    }

    /// Execute the query and return `(total_count, rows)`.
    ///
    /// Runs the count query and data query concurrently inside a single
    /// transaction.  The total count reflects all matching rows before
    /// pagination; `rows` contains only the current page.
    ///
    /// Pass a [`TransactionConfig`] to apply role, isolation level, or
    /// `SET LOCAL` settings.
    pub async fn execute(
        &self,
        tx_config: Option<TransactionConfig>,
    ) -> Result<(i64, Vec<serde_json::Value>), DbError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| DbError::Pool(e.to_string()))?;

        let begin = build_begin_statement(&tx_config);
        client
            .batch_execute(&begin)
            .await
            .map_err(|e| DbError::Transaction(format!("BEGIN error: {e}")))?;

        if let Some(ref cfg) = tx_config {
            apply_settings(&client, cfg)
                .await
                .map_err(|e| DbError::Transaction(e.to_string()))?;
        }

        let count_q = self.get_count_query();
        let data_q = self.get_select_query();
        let where_p = self.where_params();
        let select_p = self.select_params();

        let result = tokio::try_join!(
            client.query_one(&count_q, &where_p),
            client.query(&data_q, &select_p),
        )
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

        result.map(|(count_row, data_rows)| {
            let total_count: i64 = count_row.get(0);
            let rows = data_rows.to_json_list();
            (total_count, rows)
        })
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
    fn test_select_simple() {
        let pool = test_pool();
        let q = pool.select("users");
        assert_eq!(q.get_select_query(), "SELECT * FROM \"users\"");
        assert_eq!(q.get_count_query(), "SELECT COUNT(*) FROM \"users\"");
    }

    #[test]
    fn test_select_with_where() {
        let pool = test_pool();
        let mut q = pool.select("users");
        q.where_clause("id", Op::Eq, Some(SqlScalar::Int4(42)));
        let sql = q.get_select_query();
        assert!(sql.starts_with("SELECT * FROM \"users\""));
        assert!(sql.contains("WHERE"));
        assert!(sql.contains("$1"));
    }

    #[test]
    fn test_select_with_multiple_where() {
        let pool = test_pool();
        let mut q = pool.select("orders");
        q.where_clause("status", Op::Eq, Some(SqlScalar::Text("active".into())));
        q.where_clause("amount", Op::Gt, Some(SqlScalar::Int4(100)));
        let sql = q.get_select_query();
        assert!(sql.contains("WHERE"));
        assert!(sql.contains("AND"));
        assert!(sql.contains("$2"));
    }

    #[test]
    fn test_select_with_or_where() {
        let pool = test_pool();
        let mut q = pool.select("users");
        q.where_clause("status", Op::Eq, Some(SqlScalar::Text("active".into())));
        q.or_where_clause("status", Op::Eq, Some(SqlScalar::Text("pending".into())));
        let sql = q.get_select_query();
        assert!(sql.contains("WHERE"));
        assert!(sql.contains("OR"));
    }

    #[test]
    fn test_select_with_where_block() {
        let pool = test_pool();
        let mut q = pool.select("products");
        q.where_block(|q| {
            q.where_clause("id", Op::Gt, Some(SqlScalar::Int4(1)));
            q.or_where_clause("id", Op::Lt, Some(SqlScalar::Int4(100)));
        });
        let sql = q.get_select_query();
        assert!(sql.contains("WHERE"));
        assert!(sql.contains("("));
        assert!(sql.contains(")"));
        assert!(sql.contains("OR"));
    }

    #[test]
    fn test_select_with_limit() {
        let pool = test_pool();
        let q = pool.select("users").limit(10);
        assert!(q.get_select_query().contains("LIMIT $1"));
    }

    #[test]
    fn test_select_with_offset() {
        let pool = test_pool();
        let q = pool.select("users").offset(20);
        assert!(q.get_select_query().contains("OFFSET $1"));
    }

    #[test]
    fn test_select_with_limit_and_offset() {
        let pool = test_pool();
        let q = pool.select("users").limit(10).offset(20);
        let sql = q.get_select_query();
        let limit_pos = sql.find("LIMIT").expect("no LIMIT");
        let offset_pos = sql.find("OFFSET").expect("no OFFSET");
        assert!(limit_pos < offset_pos);
    }

    #[test]
    fn test_select_with_where_limit_offset() {
        let pool = test_pool();
        let mut q = pool.select("users");
        q.where_clause("active", Op::Eq, Some(SqlScalar::Bool(true)));
        let q = q.limit(10).offset(5);
        let sql = q.get_select_query();
        assert!(sql.contains("WHERE"));
        assert!(sql.contains("LIMIT $2"));
        assert!(sql.contains("OFFSET $3"));
    }

    #[test]
    fn test_order_direction() {
        assert_eq!(OrderDirection::Asc.as_str(), "ASC");
        assert_eq!(OrderDirection::Desc.as_str(), "DESC");
    }

    #[test]
    fn test_select_no_order() {
        let pool = test_pool();
        let q = pool.select("users");
        assert_eq!(q.get_order_clause(), "");
    }

    #[test]
    fn test_select_single_order() {
        let pool = test_pool();
        let q = pool.select("users").order_by("name", OrderDirection::Asc);
        let clause = q.get_order_clause();
        assert!(clause.contains("ORDER BY"));
        assert!(clause.contains("\"name\" ASC"));
    }

    #[test]
    fn test_select_multiple_order() {
        let pool = test_pool();
        let q = pool
            .select("users")
            .order_by("created_at", OrderDirection::Desc)
            .order_by("id", OrderDirection::Asc);
        let clause = q.get_order_clause();
        assert!(clause.contains("\"created_at\" DESC"));
        assert!(clause.contains("\"id\" ASC"));
        assert!(clause.contains(","));
    }

    #[test]
    fn test_select_order_appears_in_query() {
        let pool = test_pool();
        let q = pool.select("posts").order_by("date", OrderDirection::Desc);
        assert!(q.get_select_query().contains("ORDER BY \"date\" DESC"));
    }

    #[test]
    fn test_select_order_before_limit() {
        let pool = test_pool();
        let q = pool
            .select("posts")
            .order_by("id", OrderDirection::Asc)
            .limit(10);
        let sql = q.get_select_query();
        assert!(sql.find("ORDER BY").unwrap() < sql.find("LIMIT").unwrap());
    }

    #[test]
    fn test_select_full_pipeline() {
        let pool = test_pool();
        let mut q = pool.select("orders");
        q.where_clause("status", Op::Eq, Some(SqlScalar::Text("paid".into())));
        let q = q
            .order_by("created_at", OrderDirection::Desc)
            .order_by("id", OrderDirection::Asc)
            .limit(25)
            .offset(50);

        let sql = q.get_select_query();
        let select_pos = sql.find("SELECT").unwrap();
        let where_pos = sql.find("WHERE").unwrap();
        let order_pos = sql.find("ORDER BY").unwrap();
        let limit_pos = sql.find("LIMIT").unwrap();
        let offset_pos = sql.find("OFFSET").unwrap();

        assert!(select_pos < where_pos);
        assert!(where_pos < order_pos);
        assert!(order_pos < limit_pos);
        assert!(limit_pos < offset_pos);
    }

    #[test]
    fn test_count_query_with_where() {
        let pool = test_pool();
        let mut q = pool.select("users");
        q.where_clause("active", Op::Eq, Some(SqlScalar::Bool(true)));
        let sql = q.get_count_query();
        assert!(sql.starts_with("SELECT COUNT(*) FROM \"users\""));
        assert!(sql.contains("WHERE"));
    }

    #[test]
    fn test_schema_qualified() {
        let pool = test_pool();
        let q = pool.select("users").schema("public");
        assert!(q.get_select_query().contains("\"public\".\"users\""));
    }

    #[test]
    fn test_where_is_null() {
        let pool = test_pool();
        let mut q = pool.select("users");
        q.where_clause("deleted_at", Op::Eq, None);
        assert!(q.get_select_query().contains("\"deleted_at\" IS"));
    }
}
