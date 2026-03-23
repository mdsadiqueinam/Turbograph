use bytes::BytesMut;
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use tokio_postgres::types::{IsNull, ToSql, Type};
use uuid::Uuid;

/// Typed SQL array parameter wrapper.
///
/// Wraps a homogeneous Rust `Vec` so it can be passed as a single
/// `$n` parameter in a `WHERE column = ANY($n)` clause.
///
/// The inner vec must match the target PostgreSQL array type
/// (e.g. `SqlArray::Text` → `TEXT[]`).
#[derive(Debug, Clone)]
pub enum SqlArray {
    /// `BOOL[]`
    Bool(Vec<bool>),
    /// `INT2[]`
    Int2(Vec<i16>),
    /// `INT4[]`
    Int4(Vec<i32>),
    /// `INT8[]`
    Int8(Vec<i64>),
    /// `FLOAT4[]`
    Float4(Vec<f32>),
    /// `FLOAT8[]`
    Float8(Vec<f64>),
    /// `TEXT[]`
    Text(Vec<String>),
    /// `UUID[]`
    Uuid(Vec<Uuid>),
    /// `NUMERIC[]`
    Numeric(Vec<f64>),
    /// `DATE[]`
    Date(Vec<NaiveDate>),
    /// `TIME[]`
    Time(Vec<NaiveTime>),
    /// `TIMESTAMP[]`
    Timestamp(Vec<NaiveDateTime>),
    /// `TIMESTAMPTZ[]`
    Timestamptz(Vec<DateTime<Utc>>),
    /// `TIMETZ[]`
    Timetz(Vec<NaiveTime>),
}

impl ToSql for SqlArray {
    fn to_sql(
        &self,
        ty: &Type,
        out: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        match self {
            SqlArray::Bool(v) => v.to_sql(ty, out),
            SqlArray::Int2(v) => v.to_sql(ty, out),
            SqlArray::Int4(v) => v.to_sql(ty, out),
            SqlArray::Int8(v) => v.to_sql(ty, out),
            SqlArray::Float4(v) => v.to_sql(ty, out),
            SqlArray::Float8(v) => v.to_sql(ty, out),
            SqlArray::Text(v) => v.to_sql(ty, out),
            SqlArray::Uuid(v) => v.to_sql(ty, out),
            SqlArray::Numeric(v) => v.to_sql(ty, out),
            SqlArray::Date(v) => v.to_sql(ty, out),
            SqlArray::Time(v) => v.to_sql(ty, out),
            SqlArray::Timestamp(v) => v.to_sql(ty, out),
            SqlArray::Timestamptz(v) => v.to_sql(ty, out),
            SqlArray::Timetz(v) => v.to_sql(ty, out),
        }
    }

    fn accepts(ty: &Type) -> bool {
        matches!(
            *ty,
            Type::BOOL_ARRAY
                | Type::INT2_ARRAY
                | Type::INT4_ARRAY
                | Type::INT8_ARRAY
                | Type::FLOAT4_ARRAY
                | Type::FLOAT8_ARRAY
                | Type::TEXT_ARRAY
                | Type::UUID_ARRAY
                | Type::NUMERIC_ARRAY
                | Type::DATE_ARRAY
                | Type::TIME_ARRAY
                | Type::TIMESTAMP_ARRAY
                | Type::TIMESTAMPTZ_ARRAY
                | Type::TIMETZ_ARRAY
        )
    }

    tokio_postgres::types::to_sql_checked!();
}

/// Typed SQL parameter wrapper.
///
/// `SqlScalar` lets callers build a heterogeneous `Vec<SqlScalar>` and then
/// borrow it as `&[&(dyn ToSql + Sync)]` for
/// `tokio_postgres::Client::query` / `execute`.
///
/// Each variant maps to a specific PostgreSQL type:
///
/// | Variant        | PostgreSQL type(s)              |
/// |----------------|---------------------------------|
/// | `Bool`         | `BOOL`                          |
/// | `Int2`         | `INT2` / `SMALLINT`             |
/// | `Int4`         | `INT4` / `INTEGER`              |
/// | `Int8`         | `INT8` / `BIGINT`               |
/// | `Float4`       | `FLOAT4` / `REAL`               |
/// | `Float8`       | `FLOAT8` / `DOUBLE PRECISION`   |
/// | `Numeric`      | `NUMERIC`                       |
/// | `Text`         | `TEXT`, `VARCHAR`, `BPCHAR`     |
/// | `Json`         | `JSON`, `JSONB`                 |
/// | `Date`         | `DATE`                          |
/// | `Time`         | `TIME`                          |
/// | `Timestamp`    | `TIMESTAMP`                     |
/// | `Timestamptz`  | `TIMESTAMPTZ`                   |
/// | `Array`        | Array types — see [`SqlArray`]  |
#[derive(Debug, Clone)]
pub(crate) enum SqlScalar {
    Bool(bool),
    Int2(i16),
    Int4(i32),
    Int8(i64),
    Float4(f32),
    Float8(f64),
    Numeric(f64),
    Text(String),
    Json(serde_json::Value),
    Date(NaiveDate),
    Time(NaiveTime),
    Timestamp(NaiveDateTime),
    Timestamptz(DateTime<Utc>),
    Uuid(Uuid),
    Array(SqlArray),
}

impl ToSql for SqlScalar {
    fn to_sql(
        &self,
        ty: &Type,
        out: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        match self {
            SqlScalar::Bool(v) => v.to_sql(ty, out),
            SqlScalar::Int2(v) => v.to_sql(ty, out),
            SqlScalar::Int4(v) => v.to_sql(ty, out),
            SqlScalar::Int8(v) => v.to_sql(ty, out),
            SqlScalar::Float4(v) => v.to_sql(ty, out),
            SqlScalar::Float8(v) => v.to_sql(ty, out),
            SqlScalar::Numeric(v) => v.to_sql(ty, out),
            SqlScalar::Text(v) => v.to_sql(ty, out),
            SqlScalar::Json(v) => v.to_sql(ty, out),
            SqlScalar::Date(v) => v.to_sql(ty, out),
            SqlScalar::Time(v) => v.to_sql(ty, out),
            SqlScalar::Timestamp(v) => v.to_sql(ty, out),
            SqlScalar::Timestamptz(v) => v.to_sql(ty, out),
            SqlScalar::Uuid(v) => v.to_sql(ty, out),
            SqlScalar::Array(v) => v.to_sql(ty, out),
        }
    }

    fn accepts(ty: &Type) -> bool {
        // Check scalar types
        matches!(
            *ty,
            Type::BOOL
                | Type::INT2
                | Type::INT4
                | Type::INT8
                | Type::FLOAT4
                | Type::FLOAT8
                | Type::NUMERIC
                | Type::TEXT
                | Type::VARCHAR
                | Type::BPCHAR
                | Type::JSON
                | Type::JSONB
                | Type::DATE
                | Type::TIME
                | Type::TIMESTAMP
                | Type::TIMESTAMPTZ
                | Type::UUID
        ) || SqlArray::accepts(ty)
    }

    tokio_postgres::types::to_sql_checked!();
}
