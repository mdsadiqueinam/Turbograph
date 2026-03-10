use bytes::BytesMut;
use tokio_postgres::types::{IsNull, ToSql, Type};

/// Typed SQL parameter wrapper.
/// Lets callers build a `Vec<SqlScalar>` and borrow as
/// `&[&(dyn ToSql + Sync)]` for `tokio_postgres::Client::query`.
#[derive(Debug)]
pub(crate) enum SqlScalar {
    Bool(bool),
    Int2(i16),
    Int4(i32),
    Int8(i64),
    Float4(f32),
    Float8(f64),
    Text(String),
    Json(serde_json::Value),
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
            SqlScalar::Text(v) => v.to_sql(ty, out),
            SqlScalar::Json(v) => v.to_sql(ty, out),
        }
    }

    fn accepts(ty: &Type) -> bool {
        matches!(
            *ty,
            Type::BOOL
                | Type::INT2
                | Type::INT4
                | Type::INT8
                | Type::FLOAT4
                | Type::FLOAT8
                | Type::TEXT
                | Type::VARCHAR
                | Type::BPCHAR
                | Type::JSON
                | Type::JSONB
        )
    }

    tokio_postgres::types::to_sql_checked!();
}
