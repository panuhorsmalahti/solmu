use super::*;
use solmu_e2e::support::plugins::install;

async fn plugins(backend: &Backend, id: &str) -> Value {
    backend
        .client
        .get(backend.endpoint(&format!("/api/v1/threads/{id}/plugins")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[tokio::test]
async fn plugin_manifest_and_mcp_entry_errors_are_scoped_to_their_components() {
    let backend = Backend::start().await;
    let thread = backend.create_thread("Plugin validation").await;
    let id = thread["id"].as_str().unwrap();
    let workspace = std::path::Path::new(thread["workspace"].as_str().unwrap());
    let root = install(workspace, false);
    let manifest = root.join("plugin.json");
    std::fs::write(&manifest, json!({"$schema":"https://agent-plugins.org/schemas/1.0.0/plugin.schema.json","name":"project-tools","extra":true,"extensions":"wrong"}).to_string()).unwrap();
    let found = plugins(&backend, id).await;
    assert_eq!(found["items"].as_array().unwrap().len(), 1, "{found}");
    assert!(
        found["items"][0]["issues"]
            .to_string()
            .contains("Unknown manifest field")
    );
    assert!(
        found["items"][0]["issues"]
            .to_string()
            .contains("Non-object extensions")
    );
    std::fs::write(&manifest, json!({"$schema":"https://agent-plugins.org/schemas/1.0.0/plugin.schema.json","name":"project-tools","author":{"bad":true}}).to_string()).unwrap();
    let found = plugins(&backend, id).await;
    assert!(found["items"].as_array().unwrap().is_empty(), "{found}");
    assert!(
        found["issues"]
            .to_string()
            .contains("author may contain only")
    );
    install(workspace, false);
    std::fs::write(root.parent().unwrap().join("outside"), "outside").unwrap();
    std::fs::write(root.join("mcp.json"), json!({
        "$schema":"https://agent-plugins.org/schemas/1.0.0/mcp.schema.json",
        "mcpServers":{
            "escape":{"type":"stdio","command":"./../outside"},
            "insecure":{"type":"streamable-http","url":"http://example.com/mcp"},
            "reserved":{"type":"stdio","command":"tool","env":{"PLUGIN_DATA":"bad"}},
            "duplicate-headers":{"type":"streamable-http","url":"https://example.com/mcp","headers":{"Authorization":"a","authorization":"b"}}
        }
    }).to_string()).unwrap();
    let found = plugins(&backend, id).await;
    assert_eq!(found["items"][0]["skills"][0], "project-notes", "{found}");
    assert!(
        found["items"][0]["mcp_servers"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let errors = found["items"][0]["issues"].to_string();
    for expected in ["escapes", "HTTPS", "reserved", "duplicate"] {
        assert!(errors.contains(expected), "{expected}: {errors}");
    }
}
