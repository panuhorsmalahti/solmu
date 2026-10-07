use super::*;

async fn create_memory(backend: &Backend, content: &str) -> Value {
    backend
        .client
        .post(backend.endpoint("/api/v1/memories"))
        .json(&json!({"content":content}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[tokio::test]
async fn memories_are_paged_newest_first_mutable_and_added_to_relevant_prompts() {
    let backend = Backend::configured(Some("openai"), true, false).await;
    let old = create_memory(&backend, "My cat is named Miso").await;
    tokio::time::sleep(Duration::from_millis(5)).await;
    let latest = create_memory(&backend, "Miso the cat likes salmon").await;
    assert!(old["created_at"].as_str().is_some());
    let page: Value = backend
        .client
        .get(backend.endpoint("/api/v1/memories?limit=1&offset=0"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(page["items"][0]["id"], latest["id"]);
    assert_eq!(page["has_more"], true);

    let thread = backend.create_thread("Memory prompt").await;
    let id = thread["id"].as_str().unwrap();
    let message = backend.send_message(id, "Tell me about my cat").await;
    assert!(
        complete_reply(&backend, id, &message)
            .await
            .contains(&"done".to_string())
    );
    let captured = backend.requests.lock().unwrap();
    let body = &captured.last().unwrap().1;
    let systems = body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|message| message["role"] == "system")
        .map(|message| message["content"].as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(systems.contains("My cat is named Miso"));
    assert!(systems.contains("Miso the cat likes salmon"));
    assert!(body.to_string().contains("Memory"));
    drop(captured);

    let write_thread = backend.create_thread("Memory tool").await;
    let write_id = write_thread["id"].as_str().unwrap();
    let write_message = backend.send_message(write_id, "MEMORY_WRITE").await;
    assert!(
        complete_reply(&backend, write_id, &write_message)
            .await
            .contains(&"done".to_string())
    );
    let written: Value = backend
        .client
        .get(backend.endpoint("/api/v1/memories?limit=100"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        written["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|memory| memory["content"] == "The user likes cats")
    );

    let updated: Value = backend
        .client
        .put(backend.endpoint(&format!("/api/v1/memories/{}", old["id"].as_str().unwrap())))
        .json(&json!({"content":"My cat is named Miso and is six"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(updated["content"], "My cat is named Miso and is six");
    assert!(updated["updated_at"].as_str().is_some());
    assert_eq!(
        backend
            .client
            .delete(backend.endpoint(&format!(
                "/api/v1/memories/{}",
                latest["id"].as_str().unwrap()
            )))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
}
