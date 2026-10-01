use super::*;

#[tokio::test]
async fn dotenv_loading_and_environment_provider_override_work() {
    let backend = Backend::configured(Some("anthropic"), true, true).await;
    let thread = backend.create_thread("dotenv").await;
    let id = thread["id"].as_str().unwrap();
    let message = backend.send_message(id, "Hello").await;
    let events = complete_reply(&backend, id, &message).await;
    assert!(events.contains(&"done".to_owned()));
    assert_eq!(
        backend.requests.lock().unwrap()[0].0,
        "anthropic",
        "An environment value must override .env"
    );
    assert_eq!(backend.messages(id).await["items"][1]["role"], "assistant");
}

#[tokio::test]
async fn missing_credentials_and_provider_failures_preserve_user_messages() {
    let backend = Backend::configured(None, false, false).await;
    let thread = backend.create_thread("No provider").await;
    let id = thread["id"].as_str().unwrap();
    let message = backend.send_message(id, "Hello").await;
    let response = backend
        .client
        .post(backend.endpoint(&format!("/api/v1/threads/{id}/responses")))
        .json(&json!({"message_id":message["id"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        backend.messages(id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let backend = Backend::start().await;
    let thread = backend.create_thread("Provider failure").await;
    let id = thread["id"].as_str().unwrap();
    let message = backend.send_message(id, "FAIL").await;
    let response = backend
        .client
        .post(backend.endpoint(&format!("/api/v1/threads/{id}/responses")))
        .json(&json!({"message_id":message["id"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(
        backend.messages(id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let next = backend.send_message(id, "This is still allowed").await;
    assert_eq!(next["role"], "user", "Failure must release the thread lock");
}

#[tokio::test]
async fn automatic_names_use_the_cheap_model_fall_back_and_preserve_manual_titles() {
    for cheap in [
        None,
        Some("title-fixture-model"),
        Some("unknown-title-model"),
    ] {
        let mut backend = Backend::start().await;
        if let Some(model) = cheap {
            backend.set_title_model(model).await;
        }
        let thread = backend.create_thread("New conversation").await;
        let id = thread["id"].as_str().unwrap();
        assert_eq!(thread["title"], "New conversation");
        backend.send_message(id, "Plan my next idea").await;
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if backend.threads().await["items"][0]["title"] == "A new idea" {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        })
        .await
        .unwrap();
        {
            let requests = backend.requests.lock().unwrap();
            let models = requests
                .iter()
                .map(|(_, request)| request["model"].as_str().unwrap())
                .collect::<Vec<_>>();
            match cheap {
                None => assert!(
                    models
                        .iter()
                        .any(|model| ["gpt-6-luna", "test-model"].contains(model)),
                    "Default title generation should try Luna and may fall back to the main model: {models:?}"
                ),
                Some("unknown-title-model") => {
                    assert!(models.contains(&"unknown-title-model"), "{models:?}");
                    assert!(models.contains(&"test-model"), "{models:?}");
                }
                Some(model) => assert!(models.contains(&model), "{models:?}"),
            }
        }
        let before = backend.requests.lock().unwrap().len();
        backend.send_message(id, "Follow up").await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            backend.requests.lock().unwrap().len(),
            before,
            "Only the first user message names the thread"
        );
        let manual = backend.create_thread("My chosen name").await;
        backend
            .send_message(manual["id"].as_str().unwrap(), "Hello")
            .await;
        assert_eq!(
            backend.threads().await["items"][0]["title"],
            "My chosen name"
        );
    }
}
