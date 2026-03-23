use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use serde_json::{Map, Value};
use tokio_postgres::{Row, types::Type};
use uuid::Uuid;

/// Extension trait for converting a single `tokio_postgres::Row` to a
/// [`serde_json::Value`].
///
/// Each column is converted based on its PostgreSQL type.  Unsupported types
/// fall back to their string representation; `NULL` values become
/// `Value::Null`.
pub trait JsonExt {
    /// Convert `self` into a JSON object (`Value::Object`) keyed by column name.
    fn to_json(&self) -> Value;
}

/// Extension trait for converting a `Vec<tokio_postgres::Row>` to a
/// `Vec<serde_json::Value>`.
pub trait JsonListExt {
    /// Convert each row in `self` to a JSON value using [`JsonExt::to_json`].
    fn to_json_list(&self) -> Vec<Value>;
}

impl JsonExt for Row {
    fn to_json(&self) -> Value {
        let mut map = Map::new();

        for (i, col) in self.columns().iter().enumerate() {
            let name = col.name().to_string();

            let type_ = col.type_().clone();
            let value = match type_ {
                Type::BOOL => self
                    .try_get::<_, bool>(i)
                    .map(Value::Bool)
                    .unwrap_or(Value::Null),

                Type::INT2 => self
                    .try_get::<usize, i16>(i)
                    .map(|v| Value::Number(v.into()))
                    .unwrap_or(Value::Null),

                Type::INT4 => self
                    .try_get::<usize, i32>(i)
                    .map(|v| Value::Number(v.into()))
                    .unwrap_or(Value::Null),

                Type::INT8 => self
                    .try_get::<usize, i64>(i)
                    .map(|v| Value::Number(v.into()))
                    .unwrap_or(Value::Null),

                Type::FLOAT4 => self
                    .try_get::<usize, f32>(i)
                    .ok()
                    .and_then(|v| serde_json::Number::from_f64(v as f64))
                    .map(Value::Number)
                    .unwrap_or(Value::Null),

                Type::FLOAT8 | Type::NUMERIC => self
                    .try_get::<usize, f64>(i)
                    .ok()
                    .and_then(serde_json::Number::from_f64)
                    .map(Value::Number)
                    .unwrap_or(Value::Null),

                Type::TEXT | Type::VARCHAR | Type::CHAR | Type::CHAR_ARRAY => self
                    .try_get::<usize, String>(i)
                    .map(Value::String)
                    .unwrap_or(Value::Null),

                Type::JSON | Type::JSONB => self.try_get::<usize, Value>(i).unwrap_or(Value::Null),

                // UUID type
                Type::UUID => self
                    .try_get::<_, Uuid>(i)
                    .map(|v| Value::String(v.to_string()))
                    .unwrap_or(Value::Null),

                // TIMESTAMPTZ type - PostgreSQL stores as UTC, convert to DateTime<Utc>
                Type::TIMESTAMPTZ => self
                    .try_get::<_, DateTime<Utc>>(i)
                    .map(|v| Value::String(v.to_rfc3339()))
                    .unwrap_or(Value::Null),

                // TIMESTAMP type - stored as server timezone, convert to NaiveDateTime and format as iso 8601 string without timezone
                Type::TIMESTAMP => self
                    .try_get::<_, NaiveDateTime>(i)
                    .map(|v| Value::String(v.format("%Y-%m-%d:%H:%M:%S").to_string()))
                    .unwrap_or(Value::Null),

                // DATE type - convert to NaiveDate and format as ISO date string
                Type::DATE => self
                    .try_get::<_, NaiveDate>(i)
                    .map(|v| Value::String(v.format("%Y-%m-%d").to_string()))
                    .unwrap_or(Value::Null),

                // TIME type - convert to NaiveTime and format as ISO time string
                Type::TIME => self
                    .try_get::<_, NaiveTime>(i)
                    .map(|v| Value::String(v.format("%H:%M:%S%.f").to_string()))
                    .unwrap_or(Value::Null),

                // TIMETZ type - PostgreSQL TIME WITH TIME ZONE, chrono reads as NaiveTime
                // (timezone offset is not preserved by chrono deserialization)
                Type::TIMETZ => self
                    .try_get::<_, NaiveTime>(i)
                    .map(|v| Value::String(v.format("%H:%M:%S%.f").to_string()))
                    .unwrap_or(Value::Null),

                _ => self
                    .try_get::<usize, String>(i)
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            };

            map.insert(name, value);
        }

        Value::Object(map)
    }
}

impl JsonExt for Vec<Row> {
    fn to_json(&self) -> Value {
        let values = self.to_json_list();
        Value::Array(values)
    }
}

impl JsonListExt for Vec<Row> {
    fn to_json_list(&self) -> Vec<Value> {
        self.iter().map(|row| row.to_json()).collect::<Vec<Value>>()
    }
}
