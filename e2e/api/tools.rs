use super::*;

async fn runs(backend: &Backend, id: &str) -> Value {
    backend
        .client
        .get(backend.endpoint(&format!("/api/v1/threads/{id}/tools")))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap()
}
async fn execute(backend: &Backend, id: &str, calls: Value) -> Vec<String> {
    let message = backend.send_message(id, &format!("TOOLS {calls}")).await;
    let events = complete_reply(backend, id, &message).await;
    assert!(events.contains(&"done".into()), "{events:?}");
    events
}
#[tokio::test]
async fn all_tools_execute_stream_save_and_replay_across_restarts_for_both_providers() {
    for provider in ["openai", "anthropic"] {
        let mut backend = Backend::configured(Some(provider), true, false).await;
        let thread = backend.create_thread("Tools").await;
        let id = thread["id"].as_str().unwrap().to_owned();
        let message = backend.send_message(&id, "TOOLS").await;
        let events = complete_reply(&backend, &id, &message).await;
        assert!(events.contains(&"done".into()), "{provider}: {events:?}");
        assert_eq!(
            events.iter().filter(|event| *event == "tool_start").count(),
            6
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| *event == "tool_result")
                .count(),
            6
        );
        let saved = runs(&backend, &id).await;
        let items = saved["items"].as_array().unwrap();
        assert_eq!(items.len(), 6);
        for run in items {
            assert_eq!(run["status"], "completed", "{run}");
            assert_eq!(run["result"]["success"], true);
            assert!(run["started_at"].is_string());
            assert!(run["finished_at"].is_string());
        }
        assert_eq!(items[1]["result"]["output"]["content"], "Hello tools");
        assert!(
            items[3]["result"]["output"]["matches"]
                .to_string()
                .contains("hello.txt")
        );
        assert_eq!(items[4]["result"]["output"]["matches"][0]["line"], 1);
        assert_eq!(items[5]["result"]["output"]["stdout"], "Workspace ready");
        assert_eq!(
            std::fs::read_to_string(backend.directory.path().join("workspace/hello.txt")).unwrap(),
            "Welcome tools\n"
        );
        let captured = backend.requests.lock().unwrap()[0].1.clone();
        let definitions = captured["tools"].as_array().unwrap();
        assert_eq!(definitions.len(), 6);
        assert!(captured.to_string().contains("Bash"));
        backend.restart().await;
        assert_eq!(runs(&backend, &id).await, saved);
        let next = backend.send_message(&id, "Continue after restart").await;
        assert!(
            complete_reply(&backend, &id, &next)
                .await
                .contains(&"done".into())
        );
        {
            let captured = backend.requests.lock().unwrap();
            let last = &captured.last().unwrap().1;
            assert!(last.to_string().contains("Workspace ready"));
            assert!(last.to_string().contains("call-5"));
        }
        assert_eq!(
            runs(&backend, &id).await["items"].as_array().unwrap().len(),
            6,
            "Saved calls must not execute again"
        );
        backend
            .client
            .delete(backend.endpoint(&format!("/api/v1/threads/{id}")))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        let mut database = sqlx::SqliteConnection::connect(&format!(
            "sqlite://{}",
            backend.directory.path().join("solmu.db").display()
        ))
        .await
        .unwrap();
        for query in [
            "SELECT count(*) FROM tool_runs",
            "SELECT count(*) FROM tool_turns",
        ] {
            let count: i64 = sqlx::query_scalar(query)
                .fetch_one(&mut database)
                .await
                .unwrap();
            assert_eq!(count, 0);
        }
    }
}
#[tokio::test]
async fn tool_errors_are_model_visible_and_workspace_paths_are_validated() {
    let backend = Backend::start().await;
    let thread = backend.create_thread("Validation").await;
    let id = thread["id"].as_str().unwrap();
    let outside = backend.directory.path().join("outside.txt");
    std::fs::write(&outside, "private").unwrap();
    let calls = json!([
        {"name":"Unknown","arguments":{}},
        {"name":"Read","arguments":{"path":"../outside.txt"}},
        {"name":"Write","arguments":{"path":outside,"content":"changed"}},
        {"name":"Read","arguments":{"path":"."}},
        {"name":"Grep","arguments":{"pattern":"["}},
        {"name":"Write","arguments":{"path":"a.txt","content":"two two"}},
        {"name":"Edit","arguments":{"path":"a.txt","old_string":"two","new_string":"one"}},
        {"name":"Edit","arguments":{"path":"a.txt","old_string":"two","new_string":"one","replace_all":true}},
        {"name":"Read","arguments":{"path":"a.txt","offset":0}},
        {"name":"Bash","arguments":{"command":"printf failure >&2; exit 3"}}
    ]);
    execute(&backend, id, calls).await;
    let saved = runs(&backend, id).await;
    let items = saved["items"].as_array().unwrap();
    for index in [0, 1, 2, 3, 4, 6, 8, 9] {
        assert_eq!(items[index]["status"], "failed", "{}", items[index]);
    }
    assert_eq!(items[7]["status"], "completed");
    assert_eq!(items[9]["result"]["output"]["exit_code"], 3);
    assert_eq!(std::fs::read_to_string(outside).unwrap(), "private");
    assert_eq!(
        std::fs::read_to_string(backend.directory.path().join("workspace/a.txt")).unwrap(),
        "one one"
    );
    assert!(
        backend
            .requests
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .1
            .to_string()
            .contains("Unknown tool")
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            backend.directory.path().join("outside.txt"),
            backend.directory.path().join("workspace/link.txt"),
        )
        .unwrap();
        execute(
            &backend,
            id,
            json!([{"name":"Read","arguments":{"path":"link.txt"}}]),
        )
        .await;
        assert_eq!(runs(&backend, id).await["items"][10]["status"], "failed");
    }
}
#[tokio::test]
async fn stopping_bash_kills_descendants_and_records_cancellation() {
    let backend = Backend::start().await;
    let thread = backend.create_thread("Stop tools").await;
    let id = thread["id"].as_str().unwrap();
    let message=backend.send_message(id,&format!("TOOLS {}",json!([
        {"name":"Bash","arguments":{"command":"(sleep 2; printf escaped > escaped.txt) & printf started > started.txt; sleep 20"}},
        {"name":"Write","arguments":{"path":"never.txt","content":"should not run"}}
    ]))).await;
    let response = backend
        .client
        .post(backend.endpoint(&format!("/api/v1/threads/{id}/responses")))
        .json(&json!({"message_id":message["id"]}))
        .send()
        .await
        .unwrap();
    let mut events = response.bytes_stream().eventsource();
    loop {
        if events.next().await.unwrap().unwrap().event == "tool_start" {
            break;
        }
    }
    let workspace = backend.directory.path().join("workspace");
    let started = tokio::time::timeout(Duration::from_secs(20), async {
        while !workspace.join("started.txt").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(
        started.is_ok(),
        "Missing startup marker: {}",
        runs(&backend, id).await
    );
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
    let mut stopped = false;
    while let Some(event) = events.next().await {
        if event.unwrap().event == "stopped" {
            stopped = true;
        }
    }
    assert!(stopped);
    for run in runs(&backend, id).await["items"].as_array().unwrap() {
        assert_eq!(run["status"], "cancelled");
    }
    tokio::time::sleep(Duration::from_millis(2200)).await;
    assert!(
        !workspace.join("escaped.txt").exists(),
        "Bash descendants must die with the command"
    );
    assert!(!workspace.join("never.txt").exists());
    let next = backend.send_message(id, "Continue after Stop").await;
    assert!(
        complete_reply(&backend, id, &next)
            .await
            .contains(&"done".into())
    );
}
#[tokio::test]
async fn disconnected_tools_are_recovered_without_repeating_side_effects() {
    let mut backend = Backend::start().await;
    let thread = backend.create_thread("Disconnect tools").await;
    let id = thread["id"].as_str().unwrap().to_owned();
    let message=backend.send_message(&id,&format!("TOOLS {}",json!([{"name":"Bash","arguments":{"command":"printf started > started.txt; sleep 20"}}]))).await;
    let response = backend
        .client
        .post(backend.endpoint(&format!("/api/v1/threads/{id}/responses")))
        .json(&json!({"message_id":message["id"]}))
        .send()
        .await
        .unwrap();
    let mut events = response.bytes_stream().eventsource();
    loop {
        if events.next().await.unwrap().unwrap().event == "tool_start" {
            break;
        }
    }
    tokio::time::timeout(Duration::from_secs(20), async {
        while !backend
            .directory
            .path()
            .join("workspace/started.txt")
            .exists()
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    drop(events);
    backend.restart().await;
    let recovered = runs(&backend, &id).await;
    assert_eq!(recovered["items"][0]["status"], "interrupted");
    let next = backend.send_message(&id, "Continue").await;
    assert!(
        complete_reply(&backend, &id, &next)
            .await
            .contains(&"done".into())
    );
    assert!(
        backend
            .requests
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .1
            .to_string()
            .contains("interrupted")
    );
    assert_eq!(
        runs(&backend, &id).await["items"].as_array().unwrap().len(),
        1
    );
}

#[tokio::test]
async fn tool_ranges_unicode_nested_paths_and_output_limits_are_reported() {
    let backend = Backend::start().await;
    let thread = backend.create_thread("Tool limits").await;
    let id = thread["id"].as_str().unwrap();
    let workspace = backend.directory.path().join("workspace");
    std::fs::write(workspace.join("large.txt"), vec![b'x'; 1_000_001]).unwrap();
    execute(&backend,id,json!([
        {"name":"Write","arguments":{"path":"nested/hello.txt","content":"Ensimmäinen\nSecond\nThird\n"}},
        {"name":"Read","arguments":{"path":"nested/hello.txt","offset":2,"limit":1}},
        {"name":"Read","arguments":{"path":"large.txt"}},
        {"name":"Glob","arguments":{"pattern":"**/*.txt","path":"nested"}},
        {"name":"Grep","arguments":{"pattern":"second","path":"nested","case_sensitive":false}},
        {"name":"Bash","arguments":{"command":"printf '%050000d' 0"}}
    ])).await;
    let saved = runs(&backend, id).await;
    let items = saved["items"].as_array().unwrap();
    assert_eq!(items[1]["result"]["output"]["content"], "Second");
    assert_eq!(items[1]["result"]["output"]["total_lines"], 3);
    assert_eq!(items[1]["result"]["output"]["truncated"], true);
    assert_eq!(items[2]["status"], "failed");
    assert_eq!(
        items[3]["result"]["output"]["matches"][0],
        "nested/hello.txt"
    );
    assert_eq!(items[4]["result"]["output"]["matches"][0]["line"], 2);
    assert_eq!(
        items[5]["result"]["output"]["stdout"]
            .as_str()
            .unwrap()
            .len(),
        32_000
    );
    assert_eq!(items[5]["result"]["output"]["truncated"], true);
    let message=backend.send_message(id,&format!("TOOLS {}",json!([{"name":"Write","arguments":{"path":"too-big.txt","content":"x".repeat(32_001)}}]))).await;
    let events = complete_reply(&backend, id, &message).await;
    assert!(events.contains(&"error".into()));
    assert!(!workspace.join("too-big.txt").exists());
}
