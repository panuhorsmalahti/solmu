use super::*;

async fn page(backend: &Backend, suffix: &str) -> Value {
    backend
        .client
        .get(backend.endpoint(&format!("/api/v1/audit{suffix}")))
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
async fn audit_lists_all_threads_in_stable_cursor_order_with_details_and_deletions() {
    let backend = Backend::start().await;
    let first = backend.create_thread("First project").await;
    let second = backend.create_thread("Second project").await;
    for thread in [&first, &second] {
        let id = thread["id"].as_str().unwrap();
        let message = backend.send_message(id, "TOOLS").await;
        assert!(
            complete_reply(&backend, id, &message)
                .await
                .contains(&"done".into())
        );
    }
    let mut before = None;
    let mut seen = Vec::new();
    loop {
        let suffix = before.map_or("?limit=5".to_owned(), |cursor| {
            format!("?limit=5&before={cursor}")
        });
        let response = page(&backend, &suffix).await;
        let items = response["items"].as_array().unwrap();
        for item in items {
            seen.push(item.clone());
            assert!(item["arguments"].is_object());
            assert!(item["result"].is_object());
            assert!(item["created_at"].is_string());
            assert!(item["thread_title"].is_string());
        }
        before = response["next_cursor"].as_i64();
        if before.is_none() {
            break;
        }
    }
    assert_eq!(seen.len(), 12);
    assert!(
        seen.windows(2)
            .all(|pair| pair[0]["sequence"].as_i64().unwrap()
                > pair[1]["sequence"].as_i64().unwrap())
    );
    assert!(
        seen.iter()
            .take(6)
            .all(|run| run["thread_id"] == second["id"])
    );
    assert!(
        seen.iter()
            .skip(6)
            .all(|run| run["thread_id"] == first["id"])
    );
    backend
        .client
        .delete(backend.endpoint(&format!(
            "/api/v1/threads/{}",
            first["id"].as_str().unwrap()
        )))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    assert_eq!(
        page(&backend, "?limit=100").await["items"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    for suffix in ["?limit=0", "?limit=101", "?before=0", "?before=oops"] {
        assert!(
            backend
                .client
                .get(backend.endpoint(&format!("/api/v1/audit{suffix}")))
                .send()
                .await
                .unwrap()
                .status()
                .is_client_error()
        );
    }
}
