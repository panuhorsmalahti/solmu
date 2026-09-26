use super::*;

async fn profile(backend: &Backend) -> Value {
    backend
        .client
        .get(backend.endpoint("/api/v1/profile"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

async fn save(backend: &Backend, prompt: &str, model: &str) -> Value {
    let response = backend
        .client
        .put(backend.endpoint("/api/v1/profile"))
        .json(&json!({"system_prompt": prompt, "model": model}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.json().await.unwrap()
}

async fn database(backend: &Backend) -> sqlx::SqliteConnection {
    sqlx::SqliteConnection::connect(&format!(
        "sqlite://{}",
        backend
            .directory
            .path()
            .join("solmu.db")
            .to_string_lossy()
            .replace('\\', "/")
    ))
    .await
    .unwrap()
}

#[tokio::test]
async fn prompt_versions_are_timestamped_atomic_and_persistent() {
    let mut backend = Backend::start().await;
    let initial = profile(&backend).await;
    assert_eq!(initial["backend_default_model"], "test-model");
    let mut db = database(&backend).await;
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT system_prompt, edited_at FROM system_prompt_versions ORDER BY id")
            .fetch_all(&mut db)
            .await
            .unwrap();
    assert_eq!(
        rows,
        vec![(
            initial["system_prompt"].as_str().unwrap().into(),
            initial["edited_at"].as_str().unwrap().into()
        )]
    );
    tokio::time::sleep(Duration::from_millis(20)).await;
    let first = save(&backend, "First revision 🧶", "gpt-6-luna").await;
    assert_ne!(first["edited_at"], initial["edited_at"]);
    assert_eq!(
        first["backend_default_model"], "test-model",
        "Profile override must not change its fallback placeholder"
    );
    let identical = save(&backend, "First revision 🧶", "gpt-6-sol").await;
    assert_eq!(identical["edited_at"], first["edited_at"]);
    let rejected = backend
        .client
        .put(backend.endpoint("/api/v1/profile"))
        .json(&json!({"system_prompt":" "}))
        .send()
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);
    let (second, third) = tokio::join!(
        save(&backend, "Second revision", "gpt-6-luna"),
        save(&backend, "Third revision", "gpt-6-sol")
    );
    for saved in [&initial, &first, &second, &third] {
        let timestamp = saved["edited_at"].as_str().unwrap();
        assert!(timestamp.ends_with('Z'));
        let valid: (Option<String>,) = sqlx::query_as("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', ?)")
            .bind(timestamp)
            .fetch_one(&mut db)
            .await
            .unwrap();
        assert_eq!(valid.0.as_deref(), Some(timestamp));
    }
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT system_prompt, edited_at FROM system_prompt_versions ORDER BY id")
            .fetch_all(&mut db)
            .await
            .unwrap();
    assert_eq!(
        rows.len(),
        4,
        "Only distinct, valid prompt saves create revisions"
    );
    let current = profile(&backend).await;
    assert_eq!(
        rows.last().unwrap(),
        &(
            current["system_prompt"].as_str().unwrap().into(),
            current["edited_at"].as_str().unwrap().into()
        )
    );
    db.close().await.unwrap();
    backend.restart().await;
    assert_eq!(profile(&backend).await, current);
    let mut db = database(&backend).await;
    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM system_prompt_versions")
        .fetch_one(&mut db)
        .await
        .unwrap();
    assert_eq!(count.0, 4);
    db.close().await.unwrap();
}

#[tokio::test]
async fn existing_custom_prompt_is_preserved_when_history_is_added() {
    let mut backend = Backend::start().await;
    save(&backend, "Existing custom prompt", "gpt-6-sol").await;
    backend.stop();
    let mut db = database(&backend).await;
    // Reconstruct the previous schema, including its existing profile row.
    sqlx::query("DROP TABLE system_prompt_versions")
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE profile DROP COLUMN edited_at")
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 4")
        .execute(&mut db)
        .await
        .unwrap();
    db.close().await.unwrap();
    backend.restart().await;
    let current = profile(&backend).await;
    assert_eq!(current["system_prompt"], "Existing custom prompt");
    assert_eq!(current["model"], "gpt-6-sol");
    let mut db = database(&backend).await;
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT system_prompt, edited_at FROM system_prompt_versions ORDER BY id")
            .fetch_all(&mut db)
            .await
            .unwrap();
    assert_eq!(
        rows,
        vec![(
            "Existing custom prompt".into(),
            current["edited_at"].as_str().unwrap().into()
        )]
    );
    db.close().await.unwrap();
}
