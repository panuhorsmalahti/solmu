use super::*;
use solmu_e2e::support::mcp::install;

pub(super) async fn catalog(backend: &Backend, id: &str) -> Value {
    backend
        .client
        .get(backend.endpoint(&format!("/api/v1/threads/{id}/mcp")))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap()
}
pub(super) fn tool(catalog: &Value, name: &str) -> String {
    catalog["servers"][0]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == name)
        .unwrap()["agent_name"]
        .as_str()
        .unwrap()
        .into()
}
#[tokio::test]
async fn stdio_tools_load_automatically_execute_save_errors_and_replay_for_both_protocol_eras() {
    for legacy in [false, true] {
        let backend = Backend::start().await;
        let thread = backend.create_thread("MCP tools").await;
        let id = thread["id"].as_str().unwrap();
        let workspace = std::path::PathBuf::from(thread["workspace"].as_str().unwrap());
        install(&workspace, legacy);
        let message = backend.send_message(id, "Discover configured tools").await;
        complete_reply(&backend, id, &message).await;
        assert!(
            backend
                .requests
                .lock()
                .unwrap()
                .iter()
                .any(|(_, request)| request["tools"].to_string().contains("mcp_notes_search"))
        );
        let status = catalog(&backend, id).await;
        assert_eq!(status["servers"][0]["status"], "connected", "{status}");
        assert_eq!(
            status["servers"][0]["protocol_version"],
            if legacy { "2025-11-25" } else { "2026-07-28" }
        );
        assert_eq!(status["servers"][0]["tools"].as_array().unwrap().len(), 3);
        let message = backend
            .send_message(
                id,
                &format!(
                    "TOOLS {}",
                    json!([
                        {"name":tool(&status,"search"),"arguments":{"query":"Architecture"}},
                        {"name":tool(&status,"fail"),"arguments":{}}
                    ])
                ),
            )
            .await;
        let events = complete_reply(&backend, id, &message).await;
        assert!(events.contains(&"tool_result".into()));
        let runs: Value = backend
            .client
            .get(backend.endpoint(&format!("/api/v1/threads/{id}/tools")))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(runs["items"][0]["status"], "completed", "{runs}");
        assert_eq!(
            runs["items"][0]["result"]["output"]["structuredContent"]["env"],
            "configured"
        );
        assert_eq!(runs["items"][1]["status"], "failed");
        assert!(runs["items"][0]["result"]["output"].get("_meta").is_none());
        let followup = backend.send_message(id, "Use the previous result").await;
        complete_reply(&backend, id, &followup).await;
        assert!(
            backend
                .requests
                .lock()
                .unwrap()
                .iter()
                .last()
                .unwrap()
                .1
                .to_string()
                .contains("Found MCP project notes")
        );
        let requests = std::fs::read_to_string(workspace.join("mcp-requests.jsonl")).unwrap();
        let requests: Vec<Value> = requests
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(requests[0]["method"], "server/discover");
        if !legacy {
            for request in requests
                .iter()
                .filter(|request| request.get("id").is_some())
            {
                assert_eq!(
                    request["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"],
                    "2026-07-28"
                );
                assert_eq!(
                    request["params"]["_meta"]["io.modelcontextprotocol/clientInfo"]["name"],
                    "solmu"
                );
                assert!(
                    request["params"]["_meta"]["io.modelcontextprotocol/clientCapabilities"]
                        .is_object()
                );
            }
            assert!(
                !requests
                    .iter()
                    .any(|request| request["method"] == "initialize")
            );
        } else {
            assert!(
                requests
                    .iter()
                    .any(|request| request["method"] == "initialize")
            );
        }
    }
}

#[tokio::test]
async fn mcp_config_names_live_changes_disabled_servers_and_invalid_configs_are_workspace_scoped() {
    let backend = Backend::start().await;
    let thread = backend.create_thread("Live MCP").await;
    let id = thread["id"].as_str().unwrap();
    let workspace = std::path::PathBuf::from(thread["workspace"].as_str().unwrap());
    assert!(
        catalog(&backend, id).await["servers"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let (mut socket, _) = tokio_tungstenite::connect_async(
        backend.endpoint("/api/v1/events").replacen("http", "ws", 1),
    )
    .await
    .unwrap();
    socket.next().await.unwrap().unwrap();
    let path = install(&workspace, false);
    tokio::time::timeout(Duration::from_secs(12), async {
        while let Some(Ok(message)) = socket.next().await {
            let data: Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
            if data["type"] == "mcp_changed" && data["thread_id"] == id {
                break;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(
        catalog(&backend, id).await["servers"][0]["status"],
        "connected"
    );
    std::fs::write(workspace.join("mcp-description.txt"), "Updated MCP notes.").unwrap();
    tokio::time::sleep(Duration::from_secs(6)).await;
    assert_eq!(
        catalog(&backend, id).await["servers"][0]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == "search")
            .unwrap()["description"],
        "Updated MCP notes."
    );
    for name in ["mcp.json", ".cursor/mcp.json", ".vscode/mcp.json"] {
        let destination = workspace.join(name);
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::rename(&path, &destination).unwrap();
        assert_eq!(catalog(&backend, id).await["servers"][0]["source"], name);
        std::fs::rename(destination, &path).unwrap();
    }
    std::fs::write(&path, r#"{"mcpServers":{"notes":{"disabled":true},"bad":{"command":"definitely-not-a-solmu-command"},"missing":{"url":"https://example.com/${env:SOLMU_MCP_TEST_MISSING}"}}}"#).unwrap();
    let status = catalog(&backend, id).await;
    assert_eq!(status["servers"][0]["status"], "error");
    assert_eq!(status["servers"][1]["status"], "disabled");
    assert_eq!(status["issues"].as_array().unwrap().len(), 1);
    std::fs::write(&path, "invalid JSON").unwrap();
    let status = catalog(&backend, id).await;
    assert!(status["servers"].as_array().unwrap().is_empty());
    assert_eq!(
        status["issues"][0]["message"],
        "MCP config must be valid JSON"
    );
    let other = tempfile::tempdir().unwrap();
    let thread: Value = backend
        .client
        .post(backend.endpoint("/api/v1/threads"))
        .json(&json!({"workspace":other.path()}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        catalog(&backend, thread["id"].as_str().unwrap()).await["issues"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        backend
            .client
            .get(backend.endpoint("/api/v1/threads/missing/mcp"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn stopping_a_response_cancels_the_mcp_stdio_request() {
    let backend = Backend::start().await;
    let thread = backend.create_thread("Cancel MCP").await;
    let id = thread["id"].as_str().unwrap();
    let workspace = std::path::PathBuf::from(thread["workspace"].as_str().unwrap());
    install(&workspace, false);
    let status = catalog(&backend, id).await;
    let message = backend
        .send_message(
            id,
            &format!(
                "TOOLS {}",
                json!([{"name":tool(&status,"slow"),"arguments":{}}])
            ),
        )
        .await;
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
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        backend
            .client
            .post(backend.endpoint(&format!("/api/v1/threads/{id}/stop")))
            .send()
            .await
            .unwrap()
            .status()
            .is_success()
    );
    let mut stopped = false;
    while let Some(event) = events.next().await {
        if event.unwrap().event == "stopped" {
            stopped = true;
        }
    }
    assert!(stopped);
    tokio::time::timeout(Duration::from_secs(5), async {
        while !workspace.join("mcp-cancelled").exists() {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
}
