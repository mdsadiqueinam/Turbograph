#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterOp {
    Eq,
    NotEqual,
    In,
    Gt,
    Gte,
    Lt,
    Lte,
}

impl FilterOp {
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

    pub fn sql_operator(self) -> &'static str {
        match self {
            Self::Eq => "=",
            Self::NotEqual => "<>",
            Self::Gt => ">",
            Self::Gte => ">=",
            Self::Lt => "<",
            Self::Lte => "<=",
            Self::In => unreachable!("IN is not a simple binary operator"),
        }
    }

    pub fn is_range(self) -> bool {
        matches!(self, Self::Gt | Self::Gte | Self::Lt | Self::Lte)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::table::supports_range;
    use tokio_postgres::types::Type;

    // #[test]
    // fn test_condition_type_name() {
    //     let table = Table::new_for_test("blog_posts", vec![]);
    //     assert_eq!(make_condition_type(&table).type_name(), "BlogPostCondition");
    // }

    // #[test]
    // fn test_condition_type_name_users() {
    //     let table = Table::new_for_test("users", vec![]);
    //     assert_eq!(make_condition_type(&table).type_name(), "UserCondition");
    // }

    #[test]
    fn test_filter_op_from_key_not_equal() {
        assert_eq!(FilterOp::from_key("notEqual"), Some(FilterOp::NotEqual));
    }

    #[test]
    fn test_filter_op_from_key_range() {
        assert_eq!(FilterOp::from_key("greaterThanEqual"), Some(FilterOp::Gte));
        assert_eq!(FilterOp::from_key("lessThan"), Some(FilterOp::Lt));
    }

    #[test]
    fn test_filter_op_from_key_default_eq() {
        assert_eq!(FilterOp::from_key("equal"), Some(FilterOp::Eq));
    }

    #[test]
    fn test_filter_op_from_key_unknown() {
        assert_eq!(FilterOp::from_key("between"), None);
    }

    #[test]
    fn test_filter_op_sql_operator() {
        assert_eq!(FilterOp::Eq.sql_operator(), "=");
        assert_eq!(FilterOp::NotEqual.sql_operator(), "<>");
        assert_eq!(FilterOp::Gt.sql_operator(), ">");
        assert_eq!(FilterOp::Gte.sql_operator(), ">=");
        assert_eq!(FilterOp::Lt.sql_operator(), "<");
        assert_eq!(FilterOp::Lte.sql_operator(), "<=");
    }

    #[test]
    fn test_filter_op_is_range() {
        assert!(!FilterOp::Eq.is_range());
        assert!(!FilterOp::NotEqual.is_range());
        assert!(!FilterOp::In.is_range());
        assert!(FilterOp::Gt.is_range());
        assert!(FilterOp::Gte.is_range());
        assert!(FilterOp::Lt.is_range());
        assert!(FilterOp::Lte.is_range());
    }

    #[test]
    fn test_supports_range_for_numeric() {
        assert!(supports_range(&Type::INT2));
        assert!(supports_range(&Type::INT4));
        assert!(supports_range(&Type::INT8));
        assert!(supports_range(&Type::FLOAT4));
        assert!(supports_range(&Type::FLOAT8));
        assert!(supports_range(&Type::NUMERIC));
    }

    #[test]
    fn test_supports_range_for_datetime() {
        assert!(supports_range(&Type::DATE));
        assert!(supports_range(&Type::TIME));
        assert!(supports_range(&Type::TIMESTAMP));
        assert!(supports_range(&Type::TIMESTAMPTZ));
        // TIMETZ is excluded — no simple ToSql mapping available
        assert!(!supports_range(&Type::TIMETZ));
    }

    #[test]
    fn test_supports_range_for_non_numeric() {
        assert!(!supports_range(&Type::TEXT));
        assert!(!supports_range(&Type::BOOL));
        assert!(!supports_range(&Type::JSON));
    }
}
