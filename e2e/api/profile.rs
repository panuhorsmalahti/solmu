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
    assert_eq!(
        backend.requests.lock().unwrap()[0].1["messages"][0]["content"],
        prompt
    );
}
