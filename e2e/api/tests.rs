use std::time::Duration;

use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use reqwest::StatusCode;
use serde_json::{Value, json};
use solmu_e2e::support::Backend;
use sqlx::Connection;

#[tokio::test]
async fn thread_crud_messages_pagination_and_persistence() {
    let mut backend = Backend::configured(None, false, false).await;
    let thread = backend.create_thread("First thread").await;
    let id = thread["id"].as_str().unwrap();
    let second = backend.create_thread("Second thread").await;
    let message = backend.send_message(id, "hello 🧶\nsecond line").await;
    assert_eq!(message["role"], "user");
    assert_eq!(message["thread_id"], id);
    let response = backend
        .client
        .patch(backend.endpoint(&format!("/api/v1/threads/{id}")))
        .json(&json!({"title":"Renamed"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.json::<Value>().await.unwrap()["title"], "Renamed");
    let page: Value = backend
        .client
        .get(backend.endpoint("/api/v1/threads?limit=1&offset=1"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    assert_eq!(page["offset"], 1);
    assert_eq!(
        backend.messages(second["id"].as_str().unwrap()).await["items"],
        json!([])
    );
    backend.restart().await;
    let stored: Value = backend
        .client
        .get(backend.endpoint(&format!("/api/v1/threads/{id}")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(stored["title"], "Renamed");
    assert_eq!(
        backend.messages(id).await["items"][0]["content"],
        "hello 🧶\nsecond line"
    );
    let response = backend
        .client
        .delete(backend.endpoint(&format!("/api/v1/threads/{id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        backend
            .client
            .get(backend.endpoint(&format!("/api/v1/threads/{id}")))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let url = format!(
        "sqlite://{}",
        backend
            .directory
            .path()
            .join("solmu.db")
            .to_string_lossy()
            .replace('\\', "/")
    );
    let mut database = sqlx::SqliteConnection::connect(&url).await.unwrap();
    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM messages WHERE thread_id = ?")
        .bind(id)
        .fetch_one(&mut database)
        .await
        .unwrap();
    assert_eq!(count.0, 0, "Deleting a thread must delete its messages");
    database.close().await.unwrap();
}

#[tokio::test]
async fn invalid_requests_and_thread_membership_are_enforced() {
    let backend = Backend::configured(None, false, false).await;
    let thread = backend.create_thread("Validation").await;
    let id = thread["id"].as_str().unwrap();
    for (path, body, status) in [
        (
            "/api/v1/threads".to_owned(),
            json!({"title":" "}),
            StatusCode::BAD_REQUEST,
        ),
        (
            format!("/api/v1/threads/{id}/messages"),
            json!({"content":" "}),
            StatusCode::BAD_REQUEST,
        ),
        (
            format!("/api/v1/threads/{id}/messages"),
            json!({"content":"hello", "role":"assistant"}),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            "/api/v1/threads/missing/messages".to_owned(),
            json!({"content":"hello"}),
            StatusCode::NOT_FOUND,
        ),
    ] {
        let response = backend
            .client
            .post(backend.endpoint(&path))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        assert!(response.json::<Value>().await.unwrap()["error"]["code"].is_string());
    }
    assert_eq!(
        backend
            .client
            .get(backend.endpoint("/api/v1/threads?limit=0"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        backend
            .client
            .get(backend.endpoint("/api/v1/threads?offset=invalid"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        backend
            .client
            .get(backend.endpoint("/api/v1/threads/missing/messages"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let message = backend.send_message(id, "user message").await;
    let other = backend.create_thread("Other").await;
    let response = backend
        .client
        .post(backend.endpoint(&format!(
            "/api/v1/threads/{}/responses",
            other["id"].as_str().unwrap()
        )))
        .json(&json!({"message_id":message["id"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

async fn complete_reply(backend: &Backend, id: &str, message: &Value) -> Vec<String> {
    let response = backend
        .client
        .post(backend.endpoint(&format!("/api/v1/threads/{id}/responses")))
        .json(&json!({"message_id": message["id"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut events = response.bytes_stream().eventsource();
    let mut names = Vec::new();
    while let Some(event) = tokio::time::timeout(Duration::from_secs(10), events.next())
        .await
        .unwrap()
    {
        names.push(event.unwrap().event);
    }
    names
}

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
    assert_eq!(
        captured[0].1["messages"][0]["content"],
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
async fn websocket_notifications_cover_thread_message_title_and_delete_changes() {
    let backend = Backend::start().await;
    let (mut socket, _) = tokio_tungstenite::connect_async(format!(
        "{}/api/v1/events",
        backend.url.replacen("http", "ws", 1)
    ))
    .await
    .unwrap();
    assert!(
        socket
            .next()
            .await
            .unwrap()
            .unwrap()
            .to_text()
            .unwrap()
            .contains("ready")
    );
    let thread = backend.create_thread("New conversation").await;
    let id = thread["id"].as_str().unwrap();
    let notification = tokio::time::timeout(Duration::from_secs(5), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(notification.to_text().unwrap().contains(id));
    backend.send_message(id, "Name this thread").await;
    for _ in 0..2 {
        let notification = tokio::time::timeout(Duration::from_secs(5), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(
            notification
                .to_text()
                .unwrap()
                .contains("conversation_changed")
        );
    }
    backend
        .client
        .delete(backend.endpoint(&format!("/api/v1/threads/{id}")))
        .send()
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
            .to_text()
            .unwrap()
            .contains(id)
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
            assert_eq!(requests[0].1["model"], cheap.unwrap_or("test-model"));
            assert_eq!(
                requests.last().unwrap().1["model"],
                if cheap == Some("unknown-title-model") {
                    "test-model"
                } else {
                    cheap.unwrap_or("test-model")
                }
            );
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
