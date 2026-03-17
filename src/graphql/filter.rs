use tokio_postgres::types::Type;

pub fn supports_range(column_type: &Type) -> bool {
    matches!(
        *column_type,
        Type::INT2
            | Type::INT4
            | Type::INT8
            | Type::FLOAT4
            | Type::FLOAT8
            | Type::NUMERIC
            | Type::DATE
            | Type::TIME
            | Type::TIMESTAMP
            | Type::TIMESTAMPTZ
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::table::supports_range;
    use tokio_postgres::types::Type;

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
