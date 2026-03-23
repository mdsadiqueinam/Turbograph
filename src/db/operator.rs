/// SQL comparison operators used when building `WHERE` clauses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// `=`  — exact equality.
    Eq,
    /// `<>` — inequality.
    NotEqual,
    /// `= ANY($n)` — membership in an array.  Handled via
    /// [`WhereBuilder::where_in`](crate::db::where_clause::WhereBuilder::where_in)
    /// rather than [`Op::sql_operator`].
    In,
    /// `>` — strictly greater than.
    Gt,
    /// `>=` — greater than or equal.
    Gte,
    /// `<` — strictly less than.
    Lt,
    /// `<=` — less than or equal.
    Lte,
}

impl Op {
    /// Maps a GraphQL filter argument key to the corresponding [`Op`] variant.
    ///
    /// Returns `None` for unrecognised keys.
    ///
    /// | GraphQL key          | `Op` variant  |
    /// |----------------------|---------------|
    /// | `"equal"`            | `Eq`          |
    /// | `"notEqual"`         | `NotEqual`    |
    /// | `"in"`               | `In`          |
    /// | `"greaterThan"`      | `Gt`          |
    /// | `"greaterThanEqual"` | `Gte`         |
    /// | `"lessThan"`         | `Lt`          |
    /// | `"lessThanEqual"`    | `Lte`         |
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "equal" => Some(Self::Eq),
            "notEqual" => Some(Self::NotEqual),
            "in" => Some(Self::In),
            "greaterThan" => Some(Self::Gt),
            "greaterThanEqual" => Some(Self::Gte),
            "lessThan" => Some(Self::Lt),
            "lessThanEqual" => Some(Self::Lte),
            _ => None,
        }
    }

    /// Returns the SQL operator string for simple binary comparisons.
    /// Note: `In` is handled separately via `WhereBuilder::where_in()`, not as a binary operator.
    /// This method will panic if called with `Op::In` to catch programming errors.
    pub fn sql_operator(self) -> &'static str {
        match self {
            Self::Eq => "=",
            Self::NotEqual => "<>",
            Self::Gt => ">",
            Self::Gte => ">=",
            Self::Lt => "<",
            Self::Lte => "<=",
            Self::In => unreachable!(
                "Op::In should be handled via WhereBuilder::where_in(), not sql_operator()"
            ),
        }
    }

    /// Returns `true` when the operator is a range comparison (`>`, `>=`,
    /// `<`, `<=`).
    pub fn is_range(self) -> bool {
        matches!(self, Self::Gt | Self::Gte | Self::Lt | Self::Lte)
    }
}
