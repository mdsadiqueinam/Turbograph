use reqwest::Client;
use serde_json::{Value as Json, json};

const DEFAULT_GRAPHQL_URL: &str = "http://localhost:4000/graphql";

fn graphql_url() -> String {
    std::env::var("GRAPHQL_URL").unwrap_or_else(|_| DEFAULT_GRAPHQL_URL.to_string())
}

async fn wait_for_server(client: &Client, url: &str) {
    let probe = json!({
        "query": "query { __typename }"
    });

    let mut last_error = String::new();
    for _ in 0..40 {
        match client.post(url).json(&probe).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    return;
                }
                last_error = format!("HTTP {}", resp.status());
            }
            Err(err) => {
                last_error = err.to_string();
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }

    panic!(
        "E2E server is not reachable at {url}. Start it with: cargo run --manifest-path examples/server/Cargo.toml. Last error: {last_error}"
    );
}

async fn gql(client: &Client, query: &str) -> Json {
    let url = graphql_url();
    wait_for_server(client, &url).await;

    let resp = client
        .post(url)
        .json(&json!({ "query": query }))
        .send()
        .await
        .expect("failed to send GraphQL request");

    let status = resp.status();
    let body: Json = resp.json().await.expect("failed to parse JSON response");

    assert!(
        status.is_success(),
        "GraphQL endpoint returned non-success status: {status}; body: {body}"
    );
    assert!(
        body["errors"].is_null(),
        "GraphQL response contains errors for query:\n{query}\nerrors={errors:#?}",
        errors = body["errors"]
    );

    body["data"].clone()
}

#[tokio::test]
async fn e2e_can_query_over_http() {
    let client = Client::new();
    let data = gql(
        &client,
        r#"{
            allUsers(orderBy: [ID_ASC]) { totalCount }
        }"#,
    )
    .await;

    let total = data["allUsers"]["totalCount"]
        .as_u64()
        .expect("totalCount should be an integer");
    assert!(total >= 5, "expected at least seeded users, got {total}");
}

#[tokio::test]
async fn e2e_rls_prevents_updating_other_user() {
    let client = Client::new();

    // main.rs injects role=app_user and app.current_user_id=1,
    // so updating bob (id=2) should affect zero rows.
    let data = gql(
        &client,
        r#"mutation {
            updateUser(
                patch: { bio: "should not apply" }
                condition: { username: { equal: "bob" } }
            ) { id username bio }
        }"#,
    )
    .await;

    let rows = data["updateUser"]
        .as_array()
        .expect("updateUser should be a list");
    assert!(
        rows.is_empty(),
        "RLS should block updates to users not owned by app.current_user_id=1"
    );
}

#[tokio::test]
async fn e2e_rls_allows_updating_current_user() {
    let client = Client::new();
    let original_bio = "Full-stack developer and coffee enthusiast.";
    let temp_bio = "e2e-temp-bio";

    let updated = gql(
        &client,
        &format!(
            r#"mutation {{
                updateUser(
                    patch: {{ bio: "{}" }}
                    condition: {{ username: {{ equal: "alice" }} }}
                ) {{ id username bio }}
            }}"#,
            temp_bio
        ),
    )
    .await;

    let rows = updated["updateUser"]
        .as_array()
        .expect("updateUser should be a list");
    assert_eq!(rows.len(), 1, "alice should be writable by current user");
    assert_eq!(rows[0]["username"], json!("alice"));
    assert_eq!(rows[0]["bio"], json!(temp_bio));

    // Revert seed value to keep the environment stable for other tests.
    let reverted = gql(
        &client,
        &format!(
            r#"mutation {{
                updateUser(
                    patch: {{ bio: "{}" }}
                    condition: {{ username: {{ equal: "alice" }} }}
                ) {{ id bio }}
            }}"#,
            original_bio
        ),
    )
    .await;

    let revert_rows = reverted["updateUser"]
        .as_array()
        .expect("updateUser should be a list");
    assert_eq!(revert_rows.len(), 1);
    assert_eq!(revert_rows[0]["bio"], json!(original_bio));
}

#[tokio::test]
async fn e2e_rls_prevents_deleting_other_users_posts() {
    let client = Client::new();

    // post id=3 belongs to author_id=2 (bob), so delete should affect zero rows
    // when main.rs uses app.current_user_id=1.
    let data = gql(
        &client,
        r#"mutation {
            deletePost(condition: { id: { equal: 3 } }) { id }
        }"#,
    )
    .await;

    let rows = data["deletePost"]
        .as_array()
        .expect("deletePost should be a list");
    assert!(
        rows.is_empty(),
        "RLS should block deleting posts not owned by app.current_user_id=1"
    );
}

#[tokio::test]
async fn e2e_products_create() {
    let client = Client::new();
    let data = gql(
        &client,
        r#"mutation {
            createProduct(
                input: {
                    name: "Monitor"
                    description: "27-inch 4K display"
                    price: 399.99
                    stock: 5
                    isActive: true
                }
            ) {
                id
                name
                description
                price
                stock
                isActive
                createdAt
            }
        }"#,
    )
    .await;

    let product = &data["createProduct"];
    assert!(!product.is_null(), "createProduct should return a product");

    // Verify UUID format (8-4-4-4-12 hex digits)
    let id = product["id"].as_str().expect("id should be a string");
    assert!(id.len() == 36, "UUID should be 36 characters");
    assert!(
        id.chars().filter(|c| *c == '-').count() == 4,
        "UUID should have 4 dashes"
    );

    assert_eq!(product["name"], json!("Monitor"));
    assert_eq!(product["description"], json!("27-inch 4K display"));
    assert_eq!(product["price"], json!(399.99));
    assert_eq!(product["stock"], json!(5));
    assert_eq!(product["isActive"], json!(true));
}

#[tokio::test]
async fn e2e_products_read() {
    let client = Client::new();
    let data = gql(
        &client,
        r#"{
            allProducts(orderBy: [NAME_ASC]) {
                totalCount
                nodes {
                    id
                    name
                    price
                }
            }
        }"#,
    )
    .await;

    let products = &data["allProducts"];
    let total = products["totalCount"]
        .as_u64()
        .expect("totalCount should be an integer");
    assert!(
        total >= 3,
        "expected at least 3 seeded products, got {total}"
    );

    let nodes = products["nodes"]
        .as_array()
        .expect("nodes should be an array");

    // Verify first product has valid UUID
    if let Some(first) = nodes.first() {
        let id = first["id"].as_str().expect("id should be a string");
        assert!(id.len() == 36, "UUID should be 36 characters");
        // Verify UUID format
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.len(), 5, "UUID should have 5 parts");
        assert_eq!(parts[0].len(), 8, "first part should be 8 chars");
        assert_eq!(parts[1].len(), 4, "second part should be 4 chars");
        assert_eq!(parts[2].len(), 4, "third part should be 4 chars");
        assert_eq!(parts[3].len(), 4, "fourth part should be 4 chars");
        assert_eq!(parts[4].len(), 12, "fifth part should be 12 chars");
    }
}

#[tokio::test]
async fn e2e_products_update() {
    let client = Client::new();

    // Update a seeded product (Laptop - first in alphabetical order)
    let data = gql(
        &client,
        r#"mutation {
            updateProduct(
                patch: { stock: 42, price: 1099.99 }
                condition: { name: { equal: "Laptop" } }
            ) {
                id
                name
                price
                stock
            }
        }"#,
    )
    .await;

    let rows = data["updateProduct"]
        .as_array()
        .expect("updateProduct should be a list");
    assert_eq!(rows.len(), 1, "should update exactly one product");
    assert_eq!(rows[0]["name"], json!("Laptop"));
    assert_eq!(rows[0]["price"], json!(1099.99));
    assert_eq!(rows[0]["stock"], json!(42));

    // Revert to original values
    gql(
        &client,
        r#"mutation {
            updateProduct(
                patch: { stock: 10, price: 999.99 }
                condition: { name: { equal: "Laptop" } }
            ) { id }
        }"#,
    )
    .await;
}

#[tokio::test]
async fn e2e_products_delete() {
    let client = Client::new();

    // Create a product to delete
    let create_data = gql(
        &client,
        r#"mutation {
            createProduct(
                input: {
                    name: "Temporary Product"
                    price: 9.99
                    stock: 1
                }
            ) {
                id
            }
        }"#,
    )
    .await;

    let product_id = create_data["createProduct"]["id"]
        .as_str()
        .expect("id should be a string");

    // Delete the product
    let delete_data = gql(
        &client,
        &format!(
            r#"mutation {{
                deleteProduct(condition: {{ id: {{ equal: "{}" }} }}) {{
                    id
                    name
                }}
            }}"#,
            product_id
        ),
    )
    .await;

    let deleted = &delete_data["deleteProduct"];
    assert!(
        !deleted.as_array().unwrap().is_empty(),
        "should delete the product"
    );

    // Verify it's gone by querying allProducts
    let verify = gql(
        &client,
        &format!(
            r#"{{
                allProducts {{
                    nodes {{
                        id
                    }}
                }}
            }}"#
        ),
    )
    .await;

    let product_ids: Vec<&str> = verify["allProducts"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|n| n["id"].as_str())
        .collect();

    assert!(
        !product_ids.contains(&product_id),
        "deleted product should not be in allProducts"
    );
}
