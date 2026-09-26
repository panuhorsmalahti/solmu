use super::*;

#[tokio::test]
async fn provider_model_lists_and_thread_overrides_preserve_other_thread_settings() {
    for (provider, names, id) in [
        (
            "openai",
            vec!["GPT 6 Astra", "GPT 6 Sol", "GPT 6 Luna"],
            "gpt-6-luna",
        ),
        (
            "anthropic",
            vec!["Opus 5.5", "Fable 5.1", "Sonnet 5", "Haiku 4.5"],
            "claude-opus-5-5",
        ),
    ] {
        let mut backend = Backend::configured(Some(provider), true, false).await;
        let catalog: Value = backend
            .client
            .get(backend.endpoint("/api/v1/models"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(catalog["provider"], provider);
        for name in names {
            assert!(
                catalog["models"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|model| model["name"] == name)
            );
        }
        let first = backend.create_thread("First").await;
        let second = backend.create_thread("Second").await;
        let first_id = first["id"].as_str().unwrap();
        let response = backend
            .client
            .patch(backend.endpoint(&format!("/api/v1/threads/{first_id}")))
            .json(&json!({"model":id}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.json::<Value>().await.unwrap()["title"], "First");
        let message = backend.send_message(first_id, "Hello").await;
        complete_reply(&backend, first_id, &message).await;
        assert_eq!(backend.requests.lock().unwrap()[0].1["model"], id);
        let second_id = second["id"].as_str().unwrap();
        let message = backend.send_message(second_id, "Hello").await;
        complete_reply(&backend, second_id, &message).await;
        assert_eq!(backend.requests.lock().unwrap()[1].1["model"], "test-model");
        backend.restart().await;
        let stored: Value = backend
            .client
            .get(backend.endpoint(&format!("/api/v1/threads/{first_id}")))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(stored["model"], id);
        for bad in ["", "openai::gpt-6-sol", "invalid model"] {
            assert_eq!(
                backend
                    .client
                    .patch(backend.endpoint(&format!("/api/v1/threads/{first_id}")))
                    .json(&json!({"model":bad}))
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::BAD_REQUEST
            );
        }
        let cleared: Value = backend
            .client
            .patch(backend.endpoint(&format!("/api/v1/threads/{first_id}")))
            .json(&json!({"model":null}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(cleared["model"], Value::Null);
    }
}
