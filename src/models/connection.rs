#[derive(Clone, Debug)]
pub struct EdgePayload {
    pub cursor: String,
    pub node: serde_json::Value,
}

#[derive(Clone, Debug)]
pub struct ConnectionPayload {
    pub total_count: i64,
    pub has_next_page: bool,
    pub has_previous_page: bool,
    pub edges: Vec<EdgePayload>,
}
