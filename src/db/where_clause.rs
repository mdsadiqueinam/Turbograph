use super::operator::Op;
use super::scalar::{SqlArray, SqlScalar};

/// Wraps a column name in double quotes for PostgreSQL identifier quoting.
#[inline]
fn quote_ident(name: &str) -> String {
    format!("\"{name}\"")
}

/// Public API for building SQL `WHERE` clauses incrementally.
///
/// Methods append conditions to the query using `AND` by default.  Use the
/// `or_` variants to append with `OR`.  Conditions can be grouped into
/// sub-expressions with [`where_block`](Self::where_block) /
/// [`or_where_block`](Self::or_where_block).
///
/// This trait is implemented for any type that also implements the internal
/// [`WhereInternal`] trait — in practice that means [`Select<NoOrder>`],
/// [`Update`], and [`Delete`].
///
/// # Example
///
/// ```rust,ignore
/// use turbograph::db::pool::PoolExt;
/// use turbograph::db::operator::Op;
/// use turbograph::db::scalar::SqlScalar;
/// use turbograph::db::where_clause::WhereBuilder;
///
/// # fn example(pool: deadpool_postgres::Pool) {
/// let mut q = pool.select("users");
/// // WHERE active = $1 AND age > $2
/// q.where_clause("active", Op::Eq, Some(SqlScalar::Bool(true)));
/// q.where_clause("age", Op::Gt, Some(SqlScalar::Int4(18)));
/// # }
/// ```
pub trait WhereBuilder {
    /// Append an `AND column op $n` condition.
    ///
    /// When `scalar` is `None`, the condition becomes `column IS NULL`
    /// (no parameter is added).
    ///
    /// The column name is automatically quoted using double quotes.
    fn where_clause(&mut self, column: &str, op: Op, scalar: Option<SqlScalar>) -> &mut Self;

    /// Append an `OR column op $n` condition.
    ///
    /// When `scalar` is `None`, the condition becomes `OR column IS NULL`.
    #[allow(dead_code)]
    fn or_where_clause(&mut self, column: &str, op: Op, scalar: Option<SqlScalar>) -> &mut Self;

    /// Append an `AND column = ANY($n)` condition for an array of values.
    ///
    /// The column name is automatically quoted using double quotes.
    fn where_in(&mut self, column: &str, scalars: SqlArray) -> &mut Self;

    /// Append a grouped `AND (...)` sub-expression.
    ///
    /// Conditions built inside `block` are wrapped in parentheses and joined
    /// to the outer clause with `AND`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// # use turbograph::db::pool::PoolExt;
    /// # use turbograph::db::operator::Op;
    /// # use turbograph::db::scalar::SqlScalar;
    /// # use turbograph::db::where_clause::WhereBuilder;
    /// # fn example(pool: deadpool_postgres::Pool) {
    /// let mut q = pool.select("products");
    /// // WHERE (price > $1 OR price < $2)
    /// q.where_block(|q| {
    ///     q.where_clause("price", Op::Gt, Some(SqlScalar::Int4(100)));
    ///     q.or_where_clause("price", Op::Lt, Some(SqlScalar::Int4(10)));
    /// });
    /// # }
    /// ```
    #[allow(dead_code)]
    fn where_block<F>(&mut self, block: F) -> &mut Self
    where
        F: FnOnce(&mut Self);

    /// Append a grouped `OR (...)` sub-expression.
    #[allow(dead_code)]
    fn or_where_block<F>(&mut self, block: F) -> &mut Self
    where
        F: FnOnce(&mut Self);
}

/// Internal-only trait. Provides the low-level storage for WHERE clause building.
/// `has_where` is derived from `get_query().is_empty()` — no separate flag needed.
pub(super) trait WhereInternal {
    fn get_query(&self) -> &str;
    fn push_to_query(&mut self, query: String);
    fn push_param(&mut self, scalar: SqlScalar) -> usize;

    fn has_where(&self) -> bool {
        !self.get_query().is_empty()
    }

    fn get_logical_sep(&mut self) -> &str {
        if !self.has_where() {
            " WHERE "
        } else {
            let query = self.get_query().trim();
            if query.ends_with("AND") || query.ends_with("OR") || query.ends_with('(') {
                ""
            } else {
                " AND "
            }
        }
    }

    fn push_query_with_logical_sep(&mut self, query: String) {
        let sep = self.get_logical_sep().to_string();
        self.push_to_query(format!("{sep}{query}"));
    }
}

impl<T: WhereInternal> WhereBuilder for T {
    fn where_clause(&mut self, column: &str, op: Op, scalar: Option<SqlScalar>) -> &mut Self {
        let quoted = quote_ident(column);
        if let Some(scalar) = scalar {
            let param_num = self.push_param(scalar);
            let operator_str = op.sql_operator();
            self.push_query_with_logical_sep(format!(" {quoted} {operator_str} ${param_num}"));
        } else {
            // NULL check - no parameter needed
            self.push_query_with_logical_sep(format!(" {quoted} IS NULL"));
        }
        self
    }

    fn or_where_clause(&mut self, column: &str, op: Op, scalar: Option<SqlScalar>) -> &mut Self {
        if self.has_where() {
            self.push_to_query(" OR ".to_string());
        }
        self.where_clause(column, op, scalar)
    }

    fn where_in(&mut self, column: &str, scalars: SqlArray) -> &mut Self {
        let quoted = quote_ident(column);
        let param_num = self.push_param(SqlScalar::Array(scalars));
        self.push_query_with_logical_sep(format!(" {quoted} = ANY(${param_num})"));
        self
    }

    fn where_block<F>(&mut self, block: F) -> &mut Self
    where
        F: FnOnce(&mut Self),
    {
        if !self.has_where() {
            self.push_to_query(" WHERE ".to_string());
        }
        self.push_to_query(" (".to_string());
        block(self);
        self.push_to_query(")".to_string());
        self
    }

    fn or_where_block<F>(&mut self, block: F) -> &mut Self
    where
        F: FnOnce(&mut Self),
    {
        if self.has_where() {
            self.push_to_query(" OR ".to_string());
        }
        self.where_block(block)
    }
}
