use async_graphql::dynamic::{
    Enum, EnumItem, Field, FieldFuture, FieldValue, InputObject, InputValue, Object, TypeRef,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, LazyLock};
use tokio_postgres::types::Type;

use super::connection::{ConnectionPayload, EdgePayload};
use crate::graphql::condition_type_ref;
use crate::utils::inflection::{
    singularize, to_camel_case, to_pascal_case, to_screaming_snake_case,
};

/// Controls which CRUD operations should be generated for a table or column.
///
/// Turbograph reads the `@omit` annotation from PostgreSQL object comments.
/// Adding it to a table or column comment will suppress the corresponding
/// GraphQL field or mutation.
///
/// ## PostgreSQL comment syntax
///
/// ```sql
/// -- Omit all operations for a table:
/// COMMENT ON TABLE private_data IS '@omit';
///
/// -- Omit specific operations (comma-separated, no spaces):
/// COMMENT ON TABLE audit_log IS '@omit create,update,delete';
///
/// -- Omit a column from reads:
/// COMMENT ON COLUMN users.password_hash IS '@omit';
/// ```
///
/// From this struct, `false` means the operation is **included**, `true` means
/// it is **omitted**.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Omit {
    create: bool,
    read: bool,
    update: bool,
    delete: bool,
}

impl Omit {
    pub(crate) fn new(comment: &str) -> Self {
        static OMIT_REGEX: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"@omit\s+([^\s]+)").unwrap());

        let have_omit = comment.contains("@omit");

        // omit all if there is only omit string
        let mut omit = Omit {
            read: have_omit,
            create: have_omit,
            update: have_omit,
            delete: have_omit,
        };

        if let Some(caps) = OMIT_REGEX.captures(comment) {
            let res = &caps[1];
            let parts = res.split(",").collect::<Vec<&str>>();

            omit.read = parts.contains(&"read");
            omit.create = parts.contains(&"create");
            omit.update = parts.contains(&"update");
            omit.delete = parts.contains(&"delete");
        }

        omit
    }
}

/// The kind of PostgreSQL relation that a [`Table`] represents.
#[derive(Clone, Debug, PartialEq)]
pub enum Relkind {
    /// A regular `r`elation — the most common case.
    Table,
    /// A materialized view (`m`).  Write mutations (create, update, delete)
    /// are automatically suppressed for materialized views.
    MaterializedView,
}

#[cfg(test)]
impl Omit {
    pub fn for_test(read: bool) -> Self {
        Self {
            create: false,
            read,
            update: false,
            delete: false,
        }
    }
}

/// Metadata for a single PostgreSQL column, obtained from `pg_catalog`.
///
/// Columns are owned by a [`Table`] and are used to generate both the
/// GraphQL entity type and the input types for mutations.
#[derive(Clone, Debug)]
pub struct Column {
    #[allow(dead_code)]
    id: u32,
    table_oid: u32,
    name: String,
    #[allow(dead_code)]
    comment: String,
    r#type: Type,
    nullable: bool,
    has_default: bool,
    omit: Omit,
}

impl Column {
    pub(crate) fn form_row(row: &tokio_postgres::Row) -> Result<Self, crate::db::error::DbError> {
        let table_oid = row
            .try_get::<_, u32>(0)
            .map_err(|e| crate::db::error::DbError::Query(e.to_string()))?;
        let column_id =
            row.try_get::<_, i32>(1)
                .map_err(|e| crate::db::error::DbError::Query(e.to_string()))? as u32;
        let column_name = row
            .try_get::<_, String>(2)
            .map_err(|e| crate::db::error::DbError::Query(e.to_string()))?;
        let type_oid = row
            .try_get::<_, u32>(3)
            .map_err(|e| crate::db::error::DbError::Query(e.to_string()))?;
        let nullable = row
            .try_get::<_, bool>(4)
            .map_err(|e| crate::db::error::DbError::Query(e.to_string()))?;
        let has_default = row
            .try_get::<_, bool>(5)
            .map_err(|e| crate::db::error::DbError::Query(e.to_string()))?;
        let comment = row
            .try_get::<_, String>(6)
            .unwrap_or_else(|_| "".to_string());
        let data_type = Type::from_oid(type_oid).expect("Data type is not supported");
        let omit = Omit::new(&comment);

        Ok(Self {
            id: column_id,
            table_oid,
            name: column_name,
            comment,
            r#type: data_type,
            nullable,
            has_default,
            omit,
        })
    }

    /// OID of the table that owns this column.
    pub fn table_oid(&self) -> &u32 {
        &self.table_oid
    }

    /// The column name as it appears in `pg_attribute.attname`.
    pub fn name(&self) -> &String {
        &self.name
    }

    // The camelCase field name derived from the column name, used in GraphQL types and inputs.
    pub fn field_name(&self) -> String {
        to_camel_case(self.name())
    }

    /// The PostgreSQL data type of this column.
    pub fn _type(&self) -> &Type {
        &self.r#type
    }

    /// Returns `true` when `NOT NULL` is *not* set on this column.
    pub fn nullable(&self) -> bool {
        self.nullable
    }

    /// Returns `true` when the `read` operation is suppressed for this column.
    pub fn omit_read(&self) -> bool {
        self.omit.read
    }

    /// Returns `true` when the `create` operation is suppressed for this column.
    pub fn omit_create(&self) -> bool {
        self.omit.create
    }

    /// Returns `true` when the `update` operation is suppressed for this column.
    pub fn omit_update(&self) -> bool {
        self.omit.update
    }

    /// Returns `true` when the `delete` operation is suppressed for this column.
    #[allow(dead_code)]
    pub fn omit_delete(&self) -> bool {
        self.omit.delete
    }

    /// Returns `true` when the column has a `DEFAULT` expression, which means
    /// it may be omitted from `CreateXxxInput`.
    pub fn has_default(&self) -> bool {
        self.has_default
    }
}

#[cfg(test)]
impl Column {
    pub fn new_for_test(name: &str, r#type: Type, nullable: bool, omit_read: bool) -> Self {
        Self {
            id: 0,
            table_oid: 0,
            name: name.to_string(),
            comment: String::new(),
            r#type,
            nullable,
            has_default: false,
            omit: Omit::for_test(omit_read),
        }
    }
}

/// Metadata for a single PostgreSQL table or materialized view, obtained from
/// `pg_catalog`.
///
/// A `Table` aggregates its columns and exposes helper methods for generating
/// the corresponding GraphQL types and input objects.
#[derive(Clone, Debug)]
pub struct Table {
    oid: u32,
    name: String,
    schema_name: String,
    relkind: Relkind,
    #[allow(dead_code)]
    comment: String,
    columns: Vec<Arc<Column>>,
    omit: Omit,
}

impl Table {
    pub(crate) fn from_row(row: &tokio_postgres::Row) -> Result<Self, crate::db::error::DbError> {
        let oid = row
            .try_get::<_, u32>(0)
            .map_err(|e| crate::db::error::DbError::Query(e.to_string()))?;
        let schema_name = row
            .try_get::<_, String>(1)
            .map_err(|e| crate::db::error::DbError::Query(e.to_string()))?;
        let table_name = row
            .try_get::<_, String>(2)
            .map_err(|e| crate::db::error::DbError::Query(e.to_string()))?;
        let relkind_str = row
            .try_get::<_, String>(3)
            .map_err(|e| crate::db::error::DbError::Query(e.to_string()))?;
        let comment = row
            .try_get::<_, String>(4)
            .unwrap_or_else(|_| "".to_string());
        let omit = Omit::new(&comment);

        Ok(Self {
            oid,
            schema_name,
            name: table_name,
            relkind: if relkind_str == "r" {
                Relkind::Table
            } else {
                Relkind::MaterializedView
            },
            comment,
            columns: Vec::new(),
            omit,
        })
    }

    pub(crate) fn push_column(&mut self, column: Column) {
        self.columns.push(Arc::new(column));
    }

    pub fn columns(&self) -> &[Arc<Column>] {
        &self.columns
    }

    /// The PostgreSQL object identifier (OID) of this table.
    pub fn oid(&self) -> &u32 {
        &self.oid
    }

    /// The unquoted table name as it appears in `pg_class.relname`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The schema this table belongs to (e.g. `"public"`).
    pub fn schema_name(&self) -> &str {
        &self.schema_name
    }

    /// The PascalCase, singularized GraphQL type name derived from the table name.
    ///
    /// For example, `blog_posts` becomes `BlogPost`.
    pub fn type_name(&self) -> String {
        to_pascal_case(&singularize(self.name()))
    }

    /// Returns `true` when the `read` operation should be excluded from the schema.
    pub fn omit_read(&self) -> bool {
        self.omit.read
    }

    /// Returns `true` when the `create` mutation should be excluded from the schema.
    ///
    /// Always `true` for materialized views.
    pub fn omit_create(&self) -> bool {
        self.omit.create || self.relkind == Relkind::MaterializedView
    }

    /// Returns `true` when the `update` mutation should be excluded from the schema.
    ///
    /// Always `true` for materialized views.
    pub fn omit_update(&self) -> bool {
        self.omit.update || self.relkind == Relkind::MaterializedView
    }

    /// Returns `true` when the `delete` mutation should be excluded from the schema.
    ///
    /// Always `true` for materialized views.
    pub fn omit_delete(&self) -> bool {
        self.omit.delete || self.relkind == Relkind::MaterializedView
    }

    /// Name of the GraphQL `XxxCondition` input type used for filtering.
    ///
    /// For example, `User` → `UserCondition`.
    pub fn condition_type_name(&self) -> String {
        format!("{}Condition", self.type_name())
    }

    /// Name of the GraphQL `XxxOrderBy` enum used for sorting.
    ///
    /// For example, `User` → `UserOrderBy`.
    pub fn order_by_enum_name(&self) -> String {
        format!("{}OrderBy", self.type_name())
    }

    /// Name of the GraphQL `XxxConnection` type returned by list queries.
    ///
    /// For example, `User` → `UserConnection`.
    pub fn connection_type_name(&self) -> String {
        format!("{}Connection", self.type_name())
    }

    /// Name of the GraphQL `XxxEdge` type used inside a connection.
    ///
    /// For example, `User` → `UserEdge`.
    pub fn edge_type_name(&self) -> String {
        format!("{}Edge", self.type_name())
    }

    /// Name of the GraphQL `CreateXxxInput` mutation input type.
    ///
    /// For example, `User` → `CreateUserInput`.
    pub fn create_type_name(&self) -> String {
        format!("Create{}Input", self.type_name())
    }

    /// Name of the GraphQL `UpdateXxxPatch` mutation input type.
    ///
    /// For example, `User` → `UpdateUserPatch`.
    pub fn update_type_name(&self) -> String {
        format!("Update{}Patch", self.type_name())
    }

    /// Builds the GraphQL `XxxEdge` object type for this table.
    pub fn edge_type(&self) -> Object {
        let edge_type_name = self.edge_type_name();
        let node_type = self.type_name();

        Object::new(&edge_type_name)
            .field(Field::new(
                "cursor",
                TypeRef::named_nn(TypeRef::STRING),
                |ctx| {
                    FieldFuture::new(async move {
                        let edge = ctx.parent_value.try_downcast_ref::<EdgePayload>()?;
                        Ok(Some(FieldValue::value(edge.cursor.clone())))
                    })
                },
            ))
            .field(Field::new("node", TypeRef::named_nn(node_type), |ctx| {
                FieldFuture::new(async move {
                    let edge = ctx.parent_value.try_downcast_ref::<EdgePayload>()?;
                    Ok(Some(FieldValue::owned_any(edge.node.clone())))
                })
            }))
    }

    /// Builds the GraphQL `XxxConnection` object type for this table.
    ///
    /// The connection exposes `totalCount`, `pageInfo`, `edges`, and `nodes`.
    pub fn connection_type(&self) -> Object {
        let type_name = self.type_name();
        let edge_type_name = self.edge_type_name();
        let connection_type_name = self.connection_type_name();

        let edge_ref = edge_type_name.clone();
        Object::new(&connection_type_name)
            .field(Field::new(
                "totalCount",
                TypeRef::named_nn(TypeRef::INT),
                |ctx| {
                    FieldFuture::new(async move {
                        let payload = ctx.parent_value.try_downcast_ref::<ConnectionPayload>()?;
                        Ok(Some(FieldValue::value(payload.total_count as i32)))
                    })
                },
            ))
            .field(Field::new(
                "pageInfo",
                TypeRef::named_nn("PageInfo"),
                |ctx| {
                    FieldFuture::new(async move {
                        let payload = ctx.parent_value.try_downcast_ref::<ConnectionPayload>()?;
                        Ok(Some(FieldValue::owned_any(payload.clone())))
                    })
                },
            ))
            .field(Field::new(
                "edges",
                TypeRef::named_nn_list_nn(edge_ref),
                |ctx| {
                    FieldFuture::new(async move {
                        let payload = ctx.parent_value.try_downcast_ref::<ConnectionPayload>()?;
                        let list: Vec<FieldValue> = payload
                            .edges
                            .iter()
                            .map(|e| FieldValue::owned_any(e.clone()))
                            .collect();
                        Ok(Some(FieldValue::list(list)))
                    })
                },
            ))
            .field(Field::new(
                "nodes",
                TypeRef::named_nn_list_nn(type_name),
                |ctx| {
                    FieldFuture::new(async move {
                        let payload = ctx.parent_value.try_downcast_ref::<ConnectionPayload>()?;
                        let list: Vec<FieldValue> = payload
                            .edges
                            .iter()
                            .map(|e| FieldValue::owned_any(e.node.clone()))
                            .collect();
                        Ok(Some(FieldValue::list(list)))
                    })
                },
            ))
    }

    fn generate_condition_filter_type_name(&self, column: &Column) -> String {
        format!(
            "{}{}Filter",
            self.type_name(),
            to_pascal_case(column.name())
        )
    }

    /// Builds the GraphQL `XxxCondition` input type for this table.
    ///
    /// Each readable column becomes an optional field whose value is a
    /// per-column filter input (e.g. `UserEmailFilter`).
    pub fn condition_type(&self) -> InputObject {
        self.columns().iter().filter(|c| !c.omit_read()).fold(
            InputObject::new(self.condition_type_name()),
            |obj, col| {
                if condition_type_ref(col).is_some() {
                    let filter_name = self.generate_condition_filter_type_name(col);
                    obj.field(InputValue::new(
                        col.field_name(),
                        TypeRef::named(filter_name),
                    ))
                } else {
                    obj
                }
            },
        )
    }

    /// Builds the GraphQL `CreateXxxInput` input type for this table.
    ///
    /// Non-nullable columns without a default value are required fields;
    /// all others are optional.
    pub fn create_type(&self) -> InputObject {
        self.columns().iter().filter(|c| !c.omit_create()).fold(
            InputObject::new(self.create_type_name()),
            |obj, col| {
                if let Some(tr) = condition_type_ref(col) {
                    let type_ref: TypeRef = if !col.nullable() && !col.has_default() {
                        TypeRef::named_nn(tr.to_string())
                    } else {
                        tr
                    };
                    obj.field(InputValue::new(col.field_name(), type_ref))
                } else {
                    obj
                }
            },
        )
    }

    /// Builds the GraphQL `UpdateXxxPatch` input type for this table.
    ///
    /// All fields are optional so that callers can perform partial updates.
    pub fn update_type(&self) -> InputObject {
        self.columns().iter().filter(|c| !c.omit_update()).fold(
            InputObject::new(self.update_type_name()),
            |obj, col| {
                if let Some(tr) = condition_type_ref(col) {
                    obj.field(InputValue::new(col.field_name(), tr))
                } else {
                    obj
                }
            },
        )
    }

    /// Builds the per-column filter input type for `column` (e.g. `UserEmailFilter`).
    ///
    /// Returns `None` if the column type has no supported GraphQL mapping.
    ///
    /// All filter types expose `equal`, `notEqual`, and `in`.
    /// Numeric and date/time columns additionally expose `greaterThan`,
    /// `greaterThanEqual`, `lessThan`, and `lessThanEqual`.
    pub fn condition_filter_type(&self, column: &Column) -> Option<InputObject> {
        condition_type_ref(column).map(|tr| {
            let scalar_name = tr.to_string();
            let filter_name = self.generate_condition_filter_type_name(column);

            // example generated input object for a "email" column of type String:
            // input UserEmailFilter {
            //   equal: String
            // }
            let mut input = InputObject::new(filter_name)
                .field(InputValue::new("equal", tr.clone()))
                .field(InputValue::new("notEqual", tr.clone()))
                .field(InputValue::new("in", TypeRef::named_list(scalar_name)));

            if supports_range(column._type()) {
                input = input
                    .field(InputValue::new("greaterThan", tr.clone()))
                    .field(InputValue::new("greaterThanEqual", tr.clone()))
                    .field(InputValue::new("lessThan", tr.clone()))
                    .field(InputValue::new("lessThanEqual", tr));
            }

            input
        })
    }

    /// Returns all per-column filter input types for this table's readable columns.
    pub fn condition_filter_types(&self) -> Vec<InputObject> {
        self.columns()
            .iter()
            .filter(|c| !c.omit_read())
            .filter_map(|col| self.condition_filter_type(col))
            .collect()
    }

    /// Builds the GraphQL `XxxOrderBy` enum for this table.
    ///
    /// Each readable column contributes two variants: `COLUMN_ASC` and
    /// `COLUMN_DESC`.
    pub fn order_by_enum(&self) -> Enum {
        let name = self.order_by_enum_name();
        self.columns()
            .iter()
            .filter(|c| !c.omit_read())
            .fold(Enum::new(name), |e, col| {
                e.item(EnumItem::new(format!(
                    "{}_ASC",
                    to_screaming_snake_case(col.name())
                )))
                .item(EnumItem::new(format!(
                    "{}_DESC",
                    to_screaming_snake_case(col.name())
                )))
            })
    }
}

/// Returns `true` when `column_type` supports range operators (`>`, `>=`,
/// `<`, `<=`) in GraphQL filters.
///
/// Only numeric types and date/time types support range comparisons.
/// Notably, `TIMETZ` is excluded because `tokio_postgres` does not provide a
/// simple `ToSql` mapping for it.
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
impl Table {
    pub fn new_for_test(name: &str, columns: Vec<Column>) -> Self {
        Self {
            oid: 0,
            name: name.to_string(),
            schema_name: "public".to_string(),
            relkind: Relkind::Table,
            comment: String::new(),
            columns: columns.into_iter().map(Arc::new).collect(),
            omit: Omit::for_test(false),
        }
    }
}
