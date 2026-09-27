use super::*;
use solmu_e2e::support::skills::install;
use std::{fs, path::PathBuf};

async fn catalog(backend: &Backend, id: &str) -> Value {
    let response = backend
        .client
        .get(backend.endpoint(&format!("/api/v1/threads/{id}/skills")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.json().await.unwrap()
}

#[tokio::test]
async fn skills_discovery_validates_yaml_metadata_and_stays_scoped_to_the_thread_workspace() {
    let backend = Backend::start().await;
    let thread = backend.create_thread("Skills").await;
    let id = thread["id"].as_str().unwrap();
    let workspace = PathBuf::from(thread["workspace"].as_str().unwrap());
    assert!(
        catalog(&backend, id).await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(!workspace.join(".agents").exists());
    install(&workspace, "writing", "Write clear explanations.");
    let yaml = install(&workspace, "analysis", "Analyze data.");
    fs::write(&yaml, "---\r\nname: analysis\r\ndescription: >-\r\n  Analyze data\r\n  with examples.\r\nlicense: MIT\r\ncompatibility: Python\r\nallowed-tools: Read Bash\r\nmetadata:\r\n  author: solmu\r\n  version: '1.0'\r\n---\r\n\r\nUse the data guide.\r\n").unwrap();
    let invalid = [
        ("mismatch", "name: other\ndescription: Wrong name"),
        ("bad--name", "name: bad--name\ndescription: Wrong hyphens"),
        ("UPPER", "name: UPPER\ndescription: Wrong case"),
        ("missing", "name: missing"),
        ("empty", "name: empty\ndescription: '  '"),
        (
            "mapping",
            "name: mapping\ndescription: Invalid metadata\nmetadata: {version: 1}",
        ),
        (
            "compat",
            "name: compat\ndescription: Invalid compatibility\ncompatibility: ''",
        ),
        ("broken", "name: [broken\ndescription: Invalid YAML"),
    ];
    for (name, frontmatter) in invalid {
        let file = install(&workspace, name, "invalid");
        fs::write(file, format!("---\n{frontmatter}\n---\nInstructions.")).unwrap();
    }
    let result = catalog(&backend, id).await;
    assert_eq!(result["items"].as_array().unwrap().len(), 2);
    assert_eq!(result["items"][0]["name"], "analysis");
    assert_eq!(
        result["items"][0]["description"],
        "Analyze data with examples."
    );
    assert_eq!(result["items"][0]["metadata"]["version"], "1.0");
    assert_eq!(result["items"][0]["allowed_tools"], "Read Bash");
    assert_eq!(result["issues"].as_array().unwrap().len(), invalid.len());
    let other = backend.directory.path().join("another-project");
    fs::create_dir_all(&other).unwrap();
    let second: Value = backend
        .client
        .post(backend.endpoint("/api/v1/threads"))
        .json(&json!({"workspace":other}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        catalog(&backend, second["id"].as_str().unwrap()).await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        backend
            .client
            .get(backend.endpoint("/api/v1/threads/missing/skills"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn skills_enter_every_reply_context_and_instructions_and_resources_load_through_read() {
    let backend = Backend::start().await;
    let thread = backend.create_thread("Use skills").await;
    let id = thread["id"].as_str().unwrap();
    let workspace = PathBuf::from(thread["workspace"].as_str().unwrap());
    let file = install(&workspace, "writing", "Use for clear writing.");
    let text = format!(
        "TOOLS {}",
        json!([
            {"name":"Read","arguments":{"path":".agents/skills/writing/SKILL.md"}},
            {"name":"Read","arguments":{"path":".agents/skills/writing/references/style.md"}}
        ])
    );
    let message = backend.send_message(id, &text).await;
    assert!(
        complete_reply(&backend, id, &message)
            .await
            .contains(&"done".into())
    );
    {
        let requests = backend.requests.lock().unwrap();
        let first = requests
            .iter()
            .find(|(_, body)| body["stream"] == true)
            .unwrap()
            .1
            .to_string();
        assert!(first.contains("Use for clear writing."));
        assert!(first.contains(".agents/skills/writing/SKILL.md"));
        assert!(
            !first.contains("Keep explanations clear and practical."),
            "Instructions are read on demand"
        );
        let next = requests
            .iter()
            .filter(|(_, body)| body["stream"] == true)
            .nth(1)
            .unwrap()
            .1
            .to_string();
        assert!(next.contains("Keep explanations clear and practical."));
        assert!(next.contains("Use short sentences and concrete examples."));
    }
    fs::write(
        file,
        "---\nname: writing\ndescription: Updated writing guidance.\n---\nUse examples.\n",
    )
    .unwrap();
    let next = backend.send_message(id, "Follow up").await;
    complete_reply(&backend, id, &next).await;
    let request = backend
        .requests
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find(|(_, body)| body["stream"] == true)
        .unwrap()
        .1
        .to_string();
    assert!(request.contains("Updated writing guidance."));
    assert_eq!(
        backend.messages(id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        4,
        "Runtime skill context must not become saved chat messages"
    );
}

#[tokio::test]
async fn installed_added_edited_and_removed_skills_emit_live_events_without_backend_restart() {
    let backend = Backend::start().await;
    let thread = backend.create_thread("Live skills").await;
    let id = thread["id"].as_str().unwrap();
    let workspace = PathBuf::from(thread["workspace"].as_str().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(
        backend.endpoint("/api/v1/events").replacen("http", "ws", 1),
    )
    .await
    .unwrap();
    socket.next().await.unwrap().unwrap();
    catalog(&backend, id).await;
    let file = install(&workspace, "writing", "Live writing guidance.");
    for step in 0..3 {
        let event = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let event = socket.next().await.unwrap().unwrap();
                let data: Value = serde_json::from_str(event.to_text().unwrap()).unwrap();
                if data["type"] == "skills_changed" {
                    break data;
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(event["thread_id"], id);
        let result = catalog(&backend, id).await;
        if step == 2 {
            assert!(result["items"].as_array().unwrap().is_empty());
        } else {
            assert_eq!(result["items"][0]["name"], "writing");
        }
        if step == 0 {
            fs::write(&file, "---\nname: writing\ndescription: Live writing guidance.\n---\nChanged body only.\n").unwrap();
        }
        if step == 1 {
            fs::remove_file(&file).unwrap();
        }
    }
}

#[tokio::test]
async fn installed_symlinked_skills_allow_read_only_resources_and_never_expand_write_access() {
    let backend = Backend::start().await;
    let thread = backend.create_thread("Installed skill").await;
    let id = thread["id"].as_str().unwrap();
    let workspace = PathBuf::from(thread["workspace"].as_str().unwrap());
    let external = backend.directory.path().join("installed");
    let skill = install(&external, "writing", "Installed writing skill.");
    fs::create_dir_all(workspace.join(".agents/skills")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        skill.parent().unwrap(),
        workspace.join(".agents/skills/writing"),
    )
    .unwrap();
    #[cfg(windows)]
    {
        let destination = backend
            .directory
            .path()
            .join("workspace/.agents/skills/writing");
        let output = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(destination.to_string_lossy().replace('/', "\\"))
            .arg(skill.parent().unwrap().to_string_lossy().replace('/', "\\"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let text = format!(
        "TOOLS {}",
        json!([
            {"name":"Read","arguments":{"path":".agents/skills/writing/references/style.md"}},
            {"name":"Write","arguments":{"path":".agents/skills/writing/references/style.md","content":"Changed"}}
        ])
    );
    let message = backend.send_message(id, &text).await;
    complete_reply(&backend, id, &message).await;
    let tools: Value = backend
        .client
        .get(backend.endpoint(&format!("/api/v1/threads/{id}/tools")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(tools["items"][0]["status"], "completed");
    assert_eq!(tools["items"][1]["status"], "failed");
    assert_eq!(
        fs::read_to_string(skill.parent().unwrap().join("references/style.md")).unwrap(),
        "Use short sentences and concrete examples."
    );
}
