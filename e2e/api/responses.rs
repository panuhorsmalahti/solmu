use super::*;

#[tokio::test]
async fn replies_stream_before_completion_and_are_saved_with_history() {
    let backend = Backend::start().await;
    let thread = backend.create_thread("Streaming").await;
    let id = thread["id"].as_str().unwrap();
    let message = backend.send_message(id, "First message").await;
    let response = backend
        .client
        .post(backend.endpoint(&format!("/api/v1/threads/{id}/responses")))
        .json(&json!({"message_id": message["id"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut events = response.bytes_stream().eventsource();
    assert_eq!(events.next().await.unwrap().unwrap().event, "start");
    let first = events.next().await.unwrap().unwrap();
    assert_eq!(first.event, "delta");
    assert_eq!(
        serde_json::from_str::<Value>(&first.data).unwrap()["text"],
        "Hello"
    );
    assert_eq!(
        backend.messages(id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let busy = backend
        .client
        .post(backend.endpoint(&format!("/api/v1/threads/{id}/messages")))
        .json(&json!({"content":"Too early"}))
        .send()
        .await
        .unwrap();
    assert_eq!(busy.status(), StatusCode::CONFLICT);
    let mut completed = false;
    while let Some(event) = events.next().await {
        let event = event.unwrap();
        assert_ne!(event.event, "error", "{}", event.data);
        if event.event == "done" {
            completed = true;
        }
    }
    assert!(completed);
    assert_eq!(
        backend.messages(id).await["items"][1]["content"],
        "Hello from Solmu"
    );
    let duplicate = backend
        .client
        .post(backend.endpoint(&format!("/api/v1/threads/{id}/responses")))
        .json(&json!({"message_id": message["id"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(duplicate.status(), StatusCode::CONFLICT);
    let next = backend.send_message(id, "Follow up").await;
    assert!(
        complete_reply(&backend, id, &next)
            .await
            .contains(&"done".to_owned())
    );
    let captured = backend.requests.lock().unwrap();
    assert_eq!(captured.len(), 2);
    assert_eq!(
        captured[0].0, "openai",
        "Environment credentials should select a provider automatically"
    );
    assert!(captured[1].1.to_string().contains("Hello from Solmu"));
    assert!(captured[1].1.to_string().contains("Follow up"));
    assert_eq!(captured[0].1["messages"][0]["role"], "system");
    assert!(
        captured[0].1["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains(".agents/skills/<name>/SKILL.md")
    );
    assert_eq!(captured[0].1["messages"][1]["role"], "system");
    assert_eq!(
        captured[0].1["messages"][1]["content"],
        "You are Solmu, an autonomous agent."
    );
}

#[tokio::test]
async fn truncated_provider_stream_discards_partial_reply_and_releases_thread() {
    let backend = Backend::start().await;
    let thread = backend.create_thread("Interrupted reply").await;
    let id = thread["id"].as_str().unwrap();
    let message = backend.send_message(id, "TRUNCATE").await;
    let events = complete_reply(&backend, id, &message).await;
    assert!(events.contains(&"delta".to_owned()));
    assert!(events.contains(&"error".to_owned()));
    assert!(!events.contains(&"done".to_owned()));
    assert_eq!(
        backend.messages(id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let next = backend
        .send_message(id, "Continue after interruption")
        .await;
    assert!(
        complete_reply(&backend, id, &next)
            .await
            .contains(&"done".to_owned())
    );
}

#[tokio::test]
async fn stop_discards_partial_assistant_and_releases_the_thread() {
    let backend = Backend::start().await;
    let thread = backend.create_thread("Stopping").await;
    let id = thread["id"].as_str().unwrap();
    let message = backend.send_message(id, "Stop this reply").await;
    let response = backend
        .client
        .post(backend.endpoint(&format!("/api/v1/threads/{id}/responses")))
        .json(&json!({"message_id":message["id"]}))
        .send()
        .await
        .unwrap();
    let mut events = response.bytes_stream().eventsource();
    assert_eq!(events.next().await.unwrap().unwrap().event, "start");
    assert_eq!(events.next().await.unwrap().unwrap().event, "delta");
    assert_eq!(
        backend
            .client
            .post(backend.endpoint(&format!("/api/v1/threads/{id}/stop")))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(events.next().await.unwrap().unwrap().event, "stopped");
    assert!(events.next().await.is_none());
    assert_eq!(
        backend.messages(id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    backend.send_message(id, "Continue").await;
}
