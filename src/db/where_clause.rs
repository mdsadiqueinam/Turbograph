use super::operator::Op;
use super::scalar::SqlScalar;

/// The Public API
pub trait WhereBuilder {
    fn where_clause(&mut self, column: &str, op: Op, scalar: Option<SqlScalar>) -> &mut Self;
    fn or_where_clause(&mut self, column: &str, op: Op, scalar: Option<SqlScalar>) -> &mut Self;
    fn where_in(&mut self, column: &str, scalars: Vec<SqlScalar>) -> &mut Self;
    fn where_block<F>(&mut self, block: F) -> &mut Self
    where
        F: FnOnce(&mut Self);
    fn or_where_block<F>(&mut self, block: F) -> &mut Self
    where
        F: FnOnce(&mut Self);
}

/// Internal-only trait. Provides the low-level storage for WHERE clause building.
/// `has_where` is derived from `get_query().is_empty()` — no separate flag needed.
pub(super) trait WhereInternal {
    fn get_query(&self) -> &str;
    fn push_to_query(&mut self, query: String);
    fn push_param(&mut self, scalar: Option<SqlScalar>) -> usize;

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
        let operator_str = scalar
            .is_some()
            .then_some(op.sql_operator())
            .unwrap_or("IS");

        if scalar.is_some() {
            let param_num = self.push_param(scalar);
            self.push_query_with_logical_sep(format!(" {column} {operator_str} ${param_num}"));
        } else {
            // NULL check - no parameter needed
            self.push_query_with_logical_sep(format!(" {column} IS NULL"));
        }
        self
    }

    fn or_where_clause(&mut self, column: &str, op: Op, scalar: Option<SqlScalar>) -> &mut Self {
        if self.has_where() {
            self.push_to_query(" OR ".to_string());
        }
        self.where_clause(column, op, scalar)
    }

    fn where_in(&mut self, column: &str, scalars: Vec<SqlScalar>) -> &mut Self {
        if scalars.is_empty() {
            return self;
        }
        // Use = ANY($1) for proper parameterization
        let param_num = self.push_param(Some(SqlScalar::Array(scalars)));
        let fragment = format!(" {column} = ANY(${param_num})");
        self.push_query_with_logical_sep(fragment);
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
