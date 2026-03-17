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
    use crate::models::table::supports_range as column_supports_range;
    use tokio_postgres::types::Type;

    #[test]
    fn test_supports_range_for_numeric() {
        assert!(column_supports_range(&Type::INT2));
        assert!(column_supports_range(&Type::INT4));
        assert!(column_supports_range(&Type::INT8));
        assert!(column_supports_range(&Type::FLOAT4));
        assert!(column_supports_range(&Type::FLOAT8));
        assert!(column_supports_range(&Type::NUMERIC));
    }

    #[test]
    fn test_supports_range_for_datetime() {
        assert!(column_supports_range(&Type::DATE));
        assert!(column_supports_range(&Type::TIME));
        assert!(column_supports_range(&Type::TIMESTAMP));
        assert!(column_supports_range(&Type::TIMESTAMPTZ));
        // TIMETZ is excluded — no simple ToSql mapping available
        assert!(!column_supports_range(&Type::TIMETZ));
    }

    #[test]
    fn test_supports_range_for_non_numeric() {
        assert!(!column_supports_range(&Type::TEXT));
        assert!(!column_supports_range(&Type::BOOL));
        assert!(!column_supports_range(&Type::JSON));
    }
}
