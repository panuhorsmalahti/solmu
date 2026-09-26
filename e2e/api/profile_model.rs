use super::*;

#[tokio::test]
async fn profile_model_overrides_environment_and_thread_model_has_priority() {
    let backend = Backend::start().await;
    let profile = json!({"system_prompt":"You are Solmu.", "model":"gpt-6-luna"});
    assert_eq!(
        backend
            .client
            .put(backend.endpoint("/api/v1/profile"))
            .json(&profile)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let first = backend.create_thread("Profile model").await;
    let id = first["id"].as_str().unwrap();
    complete_reply(&backend, id, &backend.send_message(id, "Hello").await).await;
    assert_eq!(backend.requests.lock().unwrap()[0].1["model"], "gpt-6-luna");
    backend
        .client
        .patch(backend.endpoint(&format!("/api/v1/threads/{id}")))
        .json(&json!({"model":"gpt-6-sol"}))
        .send()
        .await
        .unwrap();
    complete_reply(&backend, id, &backend.send_message(id, "Continue").await).await;
    assert_eq!(backend.requests.lock().unwrap()[1].1["model"], "gpt-6-sol");
    backend
        .client
        .put(backend.endpoint("/api/v1/profile"))
        .json(&json!({"system_prompt":"Keep the selected model"}))
        .send()
        .await
        .unwrap();
    let stored: Value = backend
        .client
        .get(backend.endpoint("/api/v1/profile"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        stored["model"], "gpt-6-luna",
        "Omitted model preserves the override"
    );
    backend
        .client
        .put(backend.endpoint("/api/v1/profile"))
        .json(&json!({"system_prompt":"You are Solmu.","model":null}))
        .send()
        .await
        .unwrap();
    let second = backend.create_thread("Backend default").await;
    let id = second["id"].as_str().unwrap();
    complete_reply(&backend, id, &backend.send_message(id, "Hello").await).await;
    assert_eq!(backend.requests.lock().unwrap()[2].1["model"], "test-model");
}
