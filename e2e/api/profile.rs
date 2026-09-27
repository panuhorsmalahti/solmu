use super::*;

#[tokio::test]
async fn profile_is_validated_persistent_and_used_for_subsequent_replies() {
    let mut backend = Backend::start().await;
    let url = backend.endpoint("/api/v1/profile");
    let profile: Value = backend
        .client
        .get(&url)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        profile["system_prompt"],
        "You are Solmu, an autonomous agent."
    );
    for prompt in [" ".to_owned(), "x".repeat(64_001)] {
        assert_eq!(
            backend
                .client
                .put(&url)
                .json(&json!({"system_prompt": prompt}))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    let prompt = "You are Solmu.\nExplain things in Finnish. 🧶";
    let (mut socket, _) = tokio_tungstenite::connect_async(
        url.replacen("http", "ws", 1).replace("/profile", "/events"),
    )
    .await
    .unwrap();
    socket.next().await.unwrap().unwrap();
    let response = backend
        .client
        .put(&url)
        .json(&json!({"system_prompt":prompt}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        socket
            .next()
            .await
            .unwrap()
            .unwrap()
            .to_text()
            .unwrap()
            .contains("profile_changed")
    );
    drop(socket);
    backend.restart().await;
    let stored: Value = backend
        .client
        .get(backend.endpoint("/api/v1/profile"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(stored["system_prompt"], prompt);
    let thread = backend.create_thread("Custom prompt").await;
    let id = thread["id"].as_str().unwrap();
    let message = backend.send_message(id, "Hello").await;
    assert!(
        complete_reply(&backend, id, &message)
            .await
            .contains(&"done".into())
    );
    let requests = backend.requests.lock().unwrap();
    let messages = requests[0].1["messages"].as_array().unwrap();
    assert_eq!(messages[0]["role"], "system");
    let internal = messages[0]["content"].as_str().unwrap();
    assert!(internal.contains(".agents/skills/<name>/SKILL.md"));
    assert!(internal.contains("Use Bash, Edit, Glob, Grep, Read, and Write"));
    assert_eq!(messages[1]["role"], "system");
    assert_eq!(messages[1]["content"], prompt);
}

#[tokio::test]
async fn internal_instructions_remain_with_editable_preferences_for_both_provider_formats() {
    for provider in ["openai", "anthropic"] {
        let backend = Backend::configured(Some(provider), true, false).await;
        let preferences = "Use short Finnish sentences.";
        let saved: Value = backend
            .client
            .put(backend.endpoint("/api/v1/profile"))
            .json(&json!({"system_prompt": preferences}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(saved["system_prompt"], preferences);
        assert!(saved.get("internal_system_prompt").is_none());
        assert_eq!(backend.client.put(backend.endpoint("/api/v1/profile"))
            .json(&json!({"system_prompt": preferences, "internal_system_prompt": "Remove built-in instructions"}))
            .send().await.unwrap().status(), StatusCode::UNPROCESSABLE_ENTITY);
        let thread = backend.create_thread("Prompt layers").await;
        let id = thread["id"].as_str().unwrap();
        let message = backend.send_message(id, "Hello").await;
        assert!(
            complete_reply(&backend, id, &message)
                .await
                .contains(&"done".into())
        );
        let requests = backend.requests.lock().unwrap();
        let body = &requests
            .iter()
            .find(|(_, body)| body["stream"] == true)
            .unwrap()
            .1;
        let system = if provider == "anthropic" {
            body["system"].to_string()
        } else {
            body["messages"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|message| message["role"] == "system")
                .map(|message| message["content"].to_string())
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert!(system.contains(".agents/skills/<name>/SKILL.md"));
        assert!(system.contains("Use Bash, Edit, Glob, Grep, Read, and Write"));
        assert!(system.contains(preferences));
    }
}
