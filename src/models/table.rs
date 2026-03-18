use async_graphql::dynamic::{
    Enum, EnumItem, Field, FieldFuture, FieldValue, InputObject, InputValue, Object, TypeRef,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, LazyLock};
use tokio_postgres::types::Type;

use super::connection::{ConnectionPayload, EdgePayload};
use crate::graphql::condition_type_ref;
use crate::utils::inflection::{singularize, to_pascal_case, to_screaming_snake_case};

/// Omit is used to determine which operations (create, read, update, delete) should be omitted for a given table or column based on its comment.
/// The comment can contain an @omit annotation followed by a comma-separated list of operations to omit. For example:
/// - `@omit read,update` would indicate that the read and update operations should be omitted for that table or column.
/// - `@omit` without any operations would indicate that all operations
/// from this struct false means it is not omitted, true means it is omitted
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

#[derive(Clone, Debug, PartialEq)]
pub enum Relkind {
    Table,
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

#[derive(Clone, Debug)]
pub struct Column {
    id: u32,
    table_oid: u32,
    name: String,
    comment: String,
    r#type: Type,
    nullable: bool,
    has_default: bool,
    omit: Omit,
}

impl Column {
    pub(crate) fn form_row(row: &tokio_postgres::Row) -> Self {
        let table_oid = row.try_get::<_, u32>(0).unwrap();
        let column_id = row.try_get::<_, i32>(1).unwrap() as u32;
        let column_name = row.try_get::<_, String>(2).unwrap();
        let type_oid = row.try_get::<_, u32>(3).unwrap();
        let nullable = row.try_get::<_, bool>(4).unwrap();
        let has_default = row.try_get::<_, bool>(5).unwrap();
        let comment = row.try_get::<_, String>(6).unwrap_or("".to_string());
        let data_type = Type::from_oid(type_oid).expect("Data type is not supported");
        let omit = Omit::new(&comment);

        Self {
            id: column_id,
            table_oid,
            name: column_name,
            comment,
            r#type: data_type,
            nullable,
            has_default,
            omit,
        }
    }

    pub fn table_oid(&self) -> &u32 {
        &self.table_oid
    }

    pub fn name(&self) -> &String {
        &self.name
    }

    pub fn _type(&self) -> &Type {
        &self.r#type
    }

    pub fn nullable(&self) -> bool {
        self.nullable
    }

    pub fn omit_read(&self) -> bool {
        self.omit.read
    }

    pub fn omit_create(&self) -> bool {
        self.omit.create
    }

    pub fn omit_update(&self) -> bool {
        self.omit.update
    }

    pub fn omit_delete(&self) -> bool {
        self.omit.delete
    }

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

#[derive(Clone, Debug)]
pub struct Table {
    oid: u32,
    name: String,
    schema_name: String,
    relkind: Relkind,
    comment: String,
    columns: Vec<Arc<Column>>,
    omit: Omit,
}

impl Table {
    pub(crate) fn from_row(row: &tokio_postgres::Row) -> Self {
        let oid = row.try_get::<_, u32>(0).unwrap();
        let schema_name = row.try_get::<_, String>(1).unwrap();
        let table_name = row.try_get::<_, String>(2).unwrap();
        let relkind_str = row.try_get::<_, String>(3).unwrap();
        let comment = row.try_get::<_, String>(4).unwrap_or("".to_string());
        let omit = Omit::new(&comment);

        Self {
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
        }
    }

    pub(crate) fn push_column(&mut self, column: Column) {
        self.columns.push(Arc::new(column));
    }

    pub fn columns(&self) -> &[Arc<Column>] {
        &self.columns
    }

    pub fn oid(&self) -> &u32 {
        &self.oid
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn schema_name(&self) -> &str {
        &self.schema_name
    }

    pub fn type_name(&self) -> String {
        to_pascal_case(&singularize(self.name()))
    }

    pub fn omit_read(&self) -> bool {
        self.omit.read
    }

    pub fn omit_create(&self) -> bool {
        self.omit.create || self.relkind == Relkind::MaterializedView
    }

    pub fn omit_update(&self) -> bool {
        self.omit.update || self.relkind == Relkind::MaterializedView
    }

    pub fn omit_delete(&self) -> bool {
        self.omit.delete || self.relkind == Relkind::MaterializedView
    }

    pub fn condition_type_name(&self) -> String {
        format!("{}Condition", self.type_name())
    }

    pub fn order_by_enum_name(&self) -> String {
        format!("{}OrderBy", self.type_name())
    }

    pub fn connection_type_name(&self) -> String {
        format!("{}Connection", self.type_name())
    }

    pub fn edge_type_name(&self) -> String {
        format!("{}Edge", self.type_name())
    }

    pub fn create_type_name(&self) -> String {
        format!("Create{}Input", self.type_name())
    }

    pub fn update_type_name(&self) -> String {
        format!("Update{}Patch", self.type_name())
    }

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

    pub fn condition_type(&self) -> InputObject {
        self.columns().iter().filter(|c| !c.omit_read()).fold(
            InputObject::new(self.condition_type_name()),
            |obj, col| {
                if condition_type_ref(col).is_some() {
                    let filter_name = self.generate_condition_filter_type_name(col);
                    obj.field(InputValue::new(
                        col.name().as_str(),
                        TypeRef::named(filter_name),
                    ))
                } else {
                    obj
                }
            },
        )
    }

    pub fn create_type(&self) -> InputObject {
        self.columns()
            .iter()
            .filter(|c| !c.omit_create())
            .fold(InputObject::new(self.create_type_name()), |obj, col| {
                if let Some(tr) = condition_type_ref(col) {
                    let type_ref: TypeRef = if !col.nullable() && !col.has_default() {
                        TypeRef::named_nn(tr.to_string())
                    } else {
                        tr
                    };
                    obj.field(InputValue::new(col.name().as_str(), type_ref))
                } else {
                    obj
                }
            })
    }

    pub fn update_type(&self) -> InputObject {
        self.columns()
            .iter()
            .filter(|c| !c.omit_update())
            .fold(InputObject::new(self.update_type_name()), |obj, col| {
                if let Some(tr) = condition_type_ref(col) {
                    obj.field(InputValue::new(col.name().as_str(), tr))
                } else {
                    obj
                }
            })
    }

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

    pub fn condition_filter_types(&self) -> Vec<InputObject> {
        self.columns()
            .iter()
            .filter(|c| !c.omit_read())
            .filter_map(|col| self.condition_filter_type(col))
            .collect()
    }

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
