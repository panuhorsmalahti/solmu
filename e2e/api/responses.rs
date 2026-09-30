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

#[tokio::test]
async fn manual_compaction_replaces_active_history_and_keeps_future_turns_working() {
    let backend = Backend::start().await;
    let thread = backend.create_thread("Manual compaction").await;
    let id = thread["id"].as_str().unwrap();
    let first = backend.send_message(id, "First archived user turn").await;
    assert!(
        complete_reply(&backend, id, &first)
            .await
            .contains(&"done".into())
    );
    let second = backend.send_message(id, "Second archived user turn").await;
    assert!(
        complete_reply(&backend, id, &second)
            .await
            .contains(&"done".into())
    );

    let compact = backend
        .client
        .post(backend.endpoint(&format!("/api/v1/threads/{id}/compact")))
        .send()
        .await
        .unwrap();
    assert_eq!(compact.status(), StatusCode::OK);
    let summary: Value = compact.json().await.unwrap();
    assert!(
        summary["content"]
            .as_str()
            .unwrap()
            .contains("Conversation summary (compacted)")
    );
    let visible = backend.messages(id).await["items"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0]["id"], summary["id"]);

    let next = backend.send_message(id, "Continue after compaction").await;
    assert!(
        complete_reply(&backend, id, &next)
            .await
            .contains(&"done".into())
    );
    let requests = backend.requests.lock().unwrap();
    let last_request = requests
        .iter()
        .rev()
        .find(|(_, body)| body["stream"] == true)
        .unwrap()
        .1
        .to_string();
    assert!(last_request.contains("Conversation summary (compacted)"));
    assert!(last_request.contains("Continue after compaction"));
    assert!(!last_request.contains("First archived user turn"));
    assert!(!last_request.contains("Second archived user turn"));
}

#[tokio::test]
async fn responses_automatically_compact_at_the_configured_context_threshold() {
    let mut backend = Backend::start().await;
    backend.set_context_window(1).await;
    let thread = backend.create_thread("Automatic compaction").await;
    let id = thread["id"].as_str().unwrap();
    let first = backend
        .send_message(id, "Earlier conversation to summarize")
        .await;
    assert!(
        complete_reply(&backend, id, &first)
            .await
            .contains(&"done".into())
    );
    let latest = backend
        .send_message(id, "Latest request must remain intact")
        .await;
    assert!(
        complete_reply(&backend, id, &latest)
            .await
            .contains(&"done".into())
    );

    let visible = backend.messages(id).await["items"]
        .as_array()
        .unwrap()
        .clone();
    assert!(visible.iter().any(|message| {
        message["content"]
            .as_str()
            .unwrap()
            .contains("Conversation summary (compacted)")
    }));
    assert!(
        visible
            .iter()
            .any(|message| message["content"] == "Latest request must remain intact")
    );
    assert!(
        !visible
            .iter()
            .any(|message| message["content"] == "Earlier conversation to summarize")
    );
    let requests = backend.requests.lock().unwrap();
    let last_request = requests
        .iter()
        .rev()
        .find(|(_, body)| body["stream"] == true)
        .unwrap()
        .1
        .to_string();
    assert!(last_request.contains("Conversation summary (compacted)"));
    assert!(last_request.contains("Latest request must remain intact"));
}
