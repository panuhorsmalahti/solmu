use super::*;

async fn memory_tool(backend: &Backend, action: Value) {
    let thread = backend.create_thread("Memory tool").await;
    let id = thread["id"].as_str().unwrap();
    let content = format!("TOOLS {}", json!([{"name":"Memory","arguments":action}]));
    let message = backend.send_message(id, &content).await;
    assert!(
        complete_reply(backend, id, &message)
            .await
            .contains(&"done".to_string())
    );
}

async fn memory_page(backend: &Backend, limit: u32, offset: u32) -> Value {
    backend
        .client
        .get(backend.endpoint(&format!("/api/v1/memories?limit={limit}&offset={offset}")))
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
async fn memories_are_read_only_paged_and_added_to_relevant_prompts_by_the_agent_tool() {
    let backend = Backend::configured(Some("openai"), true, false).await;
    memory_tool(
        &backend,
        json!({"action":"write","content":"My cat is named Miso"}),
    )
    .await;
    let old = memory_page(&backend, 1, 0).await["items"][0].clone();
    tokio::time::sleep(Duration::from_millis(5)).await;
    memory_tool(
        &backend,
        json!({"action":"write","content":"Miso the cat likes salmon"}),
    )
    .await;
    let latest_page = memory_page(&backend, 1, 0).await;
    let latest = latest_page["items"][0].clone();
    assert_ne!(old["id"], latest["id"]);
    assert!(old["created_at"].as_str().is_some());
    assert_eq!(latest_page["has_more"], true);
    assert_eq!(
        memory_page(&backend, 1, 1).await["items"][0]["id"],
        old["id"]
    );

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

    memory_tool(
        &backend,
        json!({"action":"update","id":old["id"],"content":"My cat is named Miso and is six"}),
    )
    .await;
    let updated: Value = backend
        .client
        .get(backend.endpoint(&format!("/api/v1/memories/{}", old["id"].as_str().unwrap())))
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

    memory_tool(&backend, json!({"action":"delete","id":latest["id"]})).await;
    assert_eq!(
        backend
            .client
            .get(backend.endpoint(&format!(
                "/api/v1/memories/{}",
                latest["id"].as_str().unwrap()
            )))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );

    assert_eq!(
        backend
            .client
            .post(backend.endpoint("/api/v1/memories"))
            .json(&json!({"content":"Direct user write"}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
}
