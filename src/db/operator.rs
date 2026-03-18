#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Eq,
    NotEqual,
    In,
    Gt,
    Gte,
    Lt,
    Lte,
}

impl Op {
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
            Self::In => unreachable!("Op::In should be handled via WhereBuilder::where_in(), not sql_operator()"),
        }
    }

    pub fn is_range(self) -> bool {
        matches!(self, Self::Gt | Self::Gte | Self::Lt | Self::Lte)
    }
}
