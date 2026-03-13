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
                    patch: {{ bio: \"{}\" }}
                    condition: {{ username: {{ equal: \"alice\" }} }}
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
                    patch: {{ bio: \"{}\" }}
                    condition: {{ username: {{ equal: \"alice\" }} }}
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
