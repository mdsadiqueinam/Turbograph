use std::sync::Arc;

use async_graphql::Value as GqlValue;
use async_graphql::dynamic::{Field, FieldFuture, InputObject, InputValue, TypeRef};
use deadpool_postgres::Pool;

use crate::models::table::{Column, Table};
use crate::models::transaction::TransactionConfig;

mod executor;

pub struct GeneratedMutation {
    pub fields: Vec<Field>,
    pub input_objects: Vec<InputObject>,
}

// ── CREATE ────────────────────────────────────────────────────────────────────

fn build_create_field(
    table: &Table,
    type_name: String, // owned, moved in
    tbl_schema: String,
    tbl_name: String,
    all_columns: Arc<Vec<Arc<Column>>>,
    pool: Arc<Pool>,
) -> (Field, InputObject) {
    // input_name moved directly — no clone needed
    let field = Field::new(
        format!("create{}", type_name),
        TypeRef::named(type_name),
        move |ctx| {
            let input_pairs: Vec<(String, GqlValue)> = ctx
                .args
                .get("input")
                .and_then(|v| v.object().ok())
                .map(|obj| {
                    obj.iter()
                        .map(|(k, v)| (k.to_string(), v.as_value().clone()))
                        .collect()
                })
                .unwrap_or_default();

            let pool = pool.clone();
            let schema = tbl_schema.clone();
            let name = tbl_name.clone();
            let columns = all_columns.clone();
            let tx_config = ctx.data_opt::<TransactionConfig>().cloned();

            FieldFuture::new(async move {
                executor::execute_create(&pool, &schema, &name, input_pairs, &columns, tx_config)
                    .await
            })
        },
    )
    .argument(InputValue::new(
        "input",
        TypeRef::named_nn(table.create_type_name()),
    )); // moved, no clone

    (field, table.create_type())
}

// ── UPDATE ────────────────────────────────────────────────────────────────────

fn build_update_field(
    table: &Table,
    type_name: String,
    tbl_schema: String,
    tbl_name: String,
    all_columns: Arc<Vec<Arc<Column>>>,
    pool: Arc<Pool>,
) -> (Field, InputObject) {
    let field = Field::new(
        format!("update{}", type_name),
        TypeRef::named_nn_list_nn(type_name),
        move |ctx| {
            let patch_pairs: Vec<(String, GqlValue)> = ctx
                .args
                .get("patch")
                .and_then(|v| v.object().ok())
                .map(|obj| {
                    obj.iter()
                        .map(|(k, v)| (k.to_string(), v.as_value().clone()))
                        .collect()
                })
                .unwrap_or_default();

            let condition_pairs: Option<Vec<(String, GqlValue)>> = ctx
                .args
                .get("condition")
                .and_then(|v| v.object().ok())
                .map(|obj| {
                    obj.iter()
                        .map(|(k, v)| (k.to_string(), v.as_value().clone()))
                        .collect()
                });

            let pool = pool.clone();
            let schema = tbl_schema.clone();
            let name = tbl_name.clone();
            let columns = all_columns.clone();
            let tx_config = ctx.data_opt::<TransactionConfig>().cloned();

            FieldFuture::new(async move {
                executor::execute_update(
                    &pool,
                    &schema,
                    &name,
                    patch_pairs,
                    condition_pairs,
                    &columns,
                    tx_config,
                )
                .await
            })
        },
    )
    .argument(InputValue::new(
        "patch",
        TypeRef::named_nn(table.update_type_name()),
    )) // moved, no clone
    .argument(InputValue::new(
        "condition",
        TypeRef::named(table.condition_type_name()),
    )); // moved, no clone

    (field, table.update_type())
}

// ── DELETE ────────────────────────────────────────────────────────────────────

fn build_delete_field(
    type_name: String,
    tbl_schema: String,
    tbl_name: String,
    all_columns: Arc<Vec<Arc<Column>>>,
    pool: Arc<Pool>,
) -> Field {
    let cond_ref = format!("{}Condition", type_name);

    Field::new(
        format!("delete{}", type_name),
        TypeRef::named_nn_list_nn(type_name),
        move |ctx| {
            let condition_pairs: Option<Vec<(String, GqlValue)>> = ctx
                .args
                .get("condition")
                .and_then(|v| v.object().ok())
                .map(|obj| {
                    obj.iter()
                        .map(|(k, v)| (k.to_string(), v.as_value().clone()))
                        .collect()
                });

            let pool = pool.clone();
            let schema = tbl_schema.clone();
            let name = tbl_name.clone();
            let columns = all_columns.clone();
            let tx_config = ctx.data_opt::<TransactionConfig>().cloned();

            FieldFuture::new(async move {
                executor::execute_delete(
                    &pool,
                    &schema,
                    &name,
                    condition_pairs,
                    &columns,
                    tx_config,
                )
                .await
            })
        },
    )
    .argument(InputValue::new("condition", TypeRef::named(cond_ref)))
}

// ── Public entry point ────────────────────────────────────────────────────────

pub fn generate_mutation(table: Arc<Table>, pool: Arc<Pool>) -> GeneratedMutation {
    let mut fields = Vec::new();
    let mut input_objects = Vec::new();

    let type_name = table.type_name(); // presumably returns String
    let tbl_schema = table.schema_name().to_string();
    let tbl_name = table.name().to_string();
    let all_columns: Arc<Vec<Arc<Column>>> = Arc::new(table.columns().to_vec());

    if !table.omit_create() {
        let (field, input) = build_create_field(
            &table,
            type_name.clone(),
            tbl_schema.clone(),
            tbl_name.clone(),
            all_columns.clone(),
            pool.clone(),
        );
        fields.push(field);
        input_objects.push(input);
    }

    if !table.omit_update() {
        let (field, input) = build_update_field(
            &table,
            type_name.clone(),
            tbl_schema.clone(),
            tbl_name.clone(),
            all_columns.clone(),
            pool.clone(),
        );
        fields.push(field);
        input_objects.push(input);
    }

    if !table.omit_delete() {
        let field = build_delete_field(
            type_name,   // last use — moved, no clone
            tbl_schema,  // last use — moved, no clone
            tbl_name,    // last use — moved, no clone
            all_columns, // last use — moved, no clone
            pool,        // last use — moved, no clone
        );
        fields.push(field);
    }

    GeneratedMutation {
        fields,
        input_objects,
    }
}
