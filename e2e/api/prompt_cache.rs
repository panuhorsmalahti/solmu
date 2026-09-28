use super::*;

async fn audit(backend: &Backend) -> Value {
    backend
        .client
        .get(backend.endpoint("/api/v1/audit"))
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
async fn openai_cache_usage_is_saved_and_scoped_to_last_24_hours() {
    let backend = Backend::start().await;
    let thread = backend.create_thread("Cache test").await;
    let id = thread["id"].as_str().unwrap();
    let message = backend.send_message(id, "Hello").await;
    assert!(
        complete_reply(&backend, id, &message)
            .await
            .contains(&"done".into())
    );

    {
        let requests = backend.requests.lock().unwrap();
        let (_, request) = requests.last().unwrap();
        assert_eq!(request["prompt_cache_key"], format!("solmu:{id}"));
        assert_eq!(request["stream_options"]["include_usage"], true);
    }

    let summary = &audit(&backend).await["cache_24h"];
    assert_eq!(summary["requests"], 1);
    assert_eq!(summary["input_tokens"], 100);
    assert_eq!(summary["output_tokens"], 20);
    assert_eq!(summary["cached_input_tokens"], 40);
    assert_eq!(summary["hit_rate_percent"], 40.0);

    let mut db = sqlx::SqliteConnection::connect(&format!(
        "sqlite://{}",
        backend.directory.path().join("solmu.db").display()
    ))
    .await
    .unwrap();
    let row: (String, String, String, i64, i64, i64, i64, String) = sqlx::query_as(
        "SELECT kind,provider,model,prompt_tokens,completion_tokens,total_tokens,cached_input_tokens,usage_json FROM llm_usage"
    ).fetch_one(&mut db).await.unwrap();
    assert_eq!(row.0, "response");
    assert_eq!(row.1, "openai");
    assert_eq!(row.2, "test-model");
    assert_eq!((row.3, row.4, row.5, row.6), (100, 20, 120, 40));
    assert_eq!(
        serde_json::from_str::<Value>(&row.7).unwrap()["prompt_tokens"],
        100
    );

    sqlx::query("UPDATE llm_usage SET created_at=strftime('%Y-%m-%dT%H:%M:%fZ','now','-25 hours')")
        .execute(&mut db)
        .await
        .unwrap();
    let summary = &audit(&backend).await["cache_24h"];
    assert_eq!(summary["requests"], 0);
    assert!(summary["hit_rate_percent"].is_null());
}

#[tokio::test]
async fn anthropic_uses_cache_breakpoints_and_records_cache_reads_and_writes() {
    let backend = Backend::configured(Some("anthropic"), true, false).await;
    let thread = backend.create_thread("Cache test").await;
    let id = thread["id"].as_str().unwrap();
    let message = backend.send_message(id, "Hello").await;
    assert!(
        complete_reply(&backend, id, &message)
            .await
            .contains(&"done".into())
    );

    {
        let requests = backend.requests.lock().unwrap();
        let (_, request) = requests.last().unwrap();
        assert!(
            request["system"]
                .as_array()
                .unwrap()
                .iter()
                .any(|block| block["cache_control"]["type"] == "ephemeral")
        );
        assert!(
            request["messages"]
                .as_array()
                .unwrap()
                .iter()
                .any(
                    |message| message["content"].as_array().is_some_and(|blocks| blocks
                        .iter()
                        .any(|block| block["cache_control"]["type"] == "ephemeral"))
                )
        );
    }

    let summary = &audit(&backend).await["cache_24h"];
    assert_eq!(summary["requests"], 1);
    assert_eq!(summary["input_tokens"], 100);
    assert_eq!(summary["output_tokens"], 20);
    assert_eq!(summary["cached_input_tokens"], 40);
    assert_eq!(summary["cache_creation_input_tokens"], 10);
    assert_eq!(summary["hit_rate_percent"], 40.0);
}

#[tokio::test]
async fn tool_rounds_record_each_provider_request() {
    let backend = Backend::start().await;
    let thread = backend.create_thread("Tool cache").await;
    let id = thread["id"].as_str().unwrap();
    let message = backend.send_message(id, "TOOLS").await;
    assert!(
        complete_reply(&backend, id, &message)
            .await
            .contains(&"done".into())
    );

    {
        let requests = backend.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(
            requests
                .iter()
                .all(|(_, request)| request["prompt_cache_key"] == format!("solmu:{id}"))
        );
    }
    let summary = &audit(&backend).await["cache_24h"];
    assert_eq!(summary["requests"], 2);
    assert_eq!(summary["input_tokens"], 200);
    assert_eq!(summary["output_tokens"], 40);
    assert_eq!(summary["cached_input_tokens"], 80);
    assert_eq!(summary["hit_rate_percent"], 40.0);
}

#[tokio::test]
async fn automatic_title_usage_is_saved() {
    let backend = Backend::start().await;
    let thread = backend.create_thread("New conversation").await;
    let id = thread["id"].as_str().unwrap();
    backend.send_message(id, "A new idea").await;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let threads = backend.threads().await;
            if threads["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|thread| thread["id"] == id && thread["title"] == "A new idea")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    let summary = &audit(&backend).await["cache_24h"];
    assert_eq!(summary["requests"], 1);
    assert_eq!(summary["input_tokens"], 1);
    assert_eq!(summary["output_tokens"], 3);
    assert_eq!(summary["cached_input_tokens"], 0);
    assert_eq!(summary["hit_rate_percent"], 0.0);
    let mut db = sqlx::SqliteConnection::connect(&format!(
        "sqlite://{}",
        backend.directory.path().join("solmu.db").display()
    ))
    .await
    .unwrap();
    let kind: String = sqlx::query_scalar("SELECT kind FROM llm_usage")
        .fetch_one(&mut db)
        .await
        .unwrap();
    assert_eq!(kind, "title");
}
