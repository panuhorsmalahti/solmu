use super::*;
use solmu_e2e::support::plugins::install;

async fn catalog(backend: &Backend, id: &str, kind: &str) -> Value {
    backend
        .client
        .get(backend.endpoint(&format!("/api/v1/threads/{id}/{kind}")))
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
async fn plugin_skills_and_mcp_tools_load_into_the_agent_and_report_status() {
    let backend = Backend::start().await;
    let thread = backend.create_thread("Plugins").await;
    let id = thread["id"].as_str().unwrap();
    let workspace = std::path::Path::new(thread["workspace"].as_str().unwrap());
    let root = install(workspace, true);
    let plugins = catalog(&backend, id, "plugins").await;
    assert_eq!(plugins["items"][0]["name"], "project-tools", "{plugins}");
    assert_eq!(plugins["items"][0]["skills"][0], "project-notes");
    assert_eq!(plugins["items"][0]["mcp_servers"][0], "notes");
    let skills = catalog(&backend, id, "skills").await;
    assert!(
        skills["items"].to_string().contains("project-notes"),
        "{skills}"
    );
    let mcp = catalog(&backend, id, "mcp").await;
    assert_eq!(mcp["servers"][0]["status"], "connected", "{mcp}");
    let tool = mcp["servers"][0]["tools"][0]["agent_name"]
        .as_str()
        .unwrap();
    let message = backend
        .send_message(
            id,
            &format!(
                "TOOLS {}",
                json!([{"name":tool,"arguments":{"query":"plugin"}}])
            ),
        )
        .await;
    let events = complete_reply(&backend, id, &message).await;
    assert!(events.contains(&"tool_result".into()));
    let runs = catalog(&backend, id, "tools").await;
    assert_eq!(
        runs["items"][0]["result"]["output"]["structuredContent"]["env"], "plugin-configured",
        "{runs}"
    );
    let output = &runs["items"][0]["result"]["output"]["structuredContent"];
    assert_eq!(
        output["pluginRoot"],
        root.canonicalize().unwrap().to_string_lossy().as_ref()
    );
    let data = workspace
        .join(".solmu/plugin-data/project-tools")
        .canonicalize()
        .unwrap();
    assert_eq!(output["pluginData"], data.to_string_lossy().as_ref());
    assert_eq!(
        output["pluginExtra"],
        format!("{}/note", data.to_string_lossy())
    );
    assert!(root.join("mcp-requests.jsonl").exists());
    assert!(workspace.join(".solmu/plugin-data/project-tools").exists());
    let prompt = backend
        .requests
        .lock()
        .unwrap()
        .last()
        .unwrap()
        .1
        .to_string();
    assert!(prompt.contains("project-notes"));
}

#[tokio::test]
async fn invalid_plugin_is_reported_and_replaced_live_without_affecting_other_plugins() {
    let backend = Backend::start().await;
    let thread = backend.create_thread("Plugins").await;
    let id = thread["id"].as_str().unwrap();
    let workspace = std::path::Path::new(thread["workspace"].as_str().unwrap());
    let root = install(workspace, false);
    let invalid = workspace.join(".agents/plugins/broken");
    std::fs::create_dir_all(&invalid).unwrap();
    std::fs::write(invalid.join("plugin.json"), r#"{"$schema":"https://agent-plugins.org/schemas/1.0.0/plugin.schema.json","name":"Broken"}"#).unwrap();
    let plugins = catalog(&backend, id, "plugins").await;
    assert_eq!(plugins["items"].as_array().unwrap().len(), 1);
    assert!(
        plugins["issues"]
            .to_string()
            .contains("Invalid plugin name"),
        "{plugins}"
    );
    std::fs::write(invalid.join("plugin.json"), r#"{"$schema":"https://agent-plugins.org/schemas/1.0.0/plugin.schema.json","name":"broken"}"#).unwrap();
    let plugins = catalog(&backend, id, "plugins").await;
    assert_eq!(plugins["items"].as_array().unwrap().len(), 2, "{plugins}");
    std::fs::write(root.join("mcp.json"), "invalid JSON").unwrap();
    let plugins = catalog(&backend, id, "plugins").await;
    assert!(
        plugins["items"].to_string().contains("expected value"),
        "{plugins}"
    );
    assert_eq!(
        catalog(&backend, id, "skills").await["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
