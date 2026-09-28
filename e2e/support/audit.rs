use super::Backend;
use serde_json::json;

pub async fn seed(backend: &Backend, turns: usize) -> String {
    let thread = backend.create_thread("Audit project").await;
    let id = thread["id"].as_str().unwrap().to_owned();
    for _ in 0..turns {
        add_turn(backend, &id).await;
    }
    id
}

pub async fn add_turn(backend: &Backend, id: &str) {
    let message = backend.send_message(id, "TOOLS").await;
    let response = backend
        .client
        .post(backend.endpoint(&format!("/api/v1/threads/{id}/responses")))
        .json(&json!({"message_id":message["id"]}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let body = response.text().await.unwrap();
    assert!(body.contains("event: done"), "{body}");
}
