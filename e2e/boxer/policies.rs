use super::*;
use serde_json::{Value, json};
use std::path::Path;

fn plan(workspace: &Path, args: &[&str]) -> Value {
    let output = Command::new(binary("boxer"))
        .arg("--cwd")
        .arg(workspace)
        .args(args)
        .args([
            "--print-policy",
            "--",
            "program with spaces",
            "quoted\" argument",
        ])
        .env("BOXER_TEST_SECRET", "never-print-this-value")
        .env("OPENAI_API_KEY", "never-print-this-key")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(!text.contains("never-print-this"));
    serde_json::from_str(&text).unwrap()
}

#[test]
fn resolved_policies_show_permissions_and_never_launch_or_disclose_environment_values() {
    let workspace = tempfile::tempdir().unwrap();
    let value = plan(workspace.path(), &[]);
    assert_eq!(value["policy"]["mode"], "unrestricted");
    assert_eq!(value["network"], "allowed");
    assert_eq!(value["environment"]["inherit"], true);
    assert_eq!(value["arguments"][0], "quoted\" argument");
    assert_eq!(value["enforcement"], "not-applied");
    let value = plan(workspace.path(), &["--profile", "solmu"]);
    assert_eq!(value["policy"]["mode"], "workspace");
    assert_eq!(value["environment"]["inherit"], false);
    assert!(
        value["environment"]["forwarded_names"]
            .as_array()
            .unwrap()
            .contains(&json!("OPENAI_API_KEY"))
    );
    assert!(
        !value["environment"]["forwarded_names"]
            .as_array()
            .unwrap()
            .contains(&json!("BOXER_TEST_SECRET"))
    );
    assert_eq!(value["platform_supported"], !cfg!(windows));
    assert_eq!(
        value["runtime_list"],
        if cfg!(target_os = "macos") {
            json!(["/"])
        } else {
            json!([])
        }
    );
    let value = plan(workspace.path(), &["--isolated", "--cpus", "1"]);
    assert_eq!(value["policy"]["cpus"], 1);
    assert_eq!(value["policy"]["memory_mib"], 2048);
    assert_eq!(value["policy"]["pids"], 256);
    assert_eq!(value["private_mounts"], json!(["/proc", "/dev", "/tmp"]));
}

#[test]
fn explicit_policy_files_resolve_relative_paths_variables_and_cli_overrides() {
    let directory = tempfile::tempdir().unwrap();
    let workspace = directory.path().join("project");
    let shared = directory.path().join("shared");
    let output = directory.path().join("output");
    for path in [&workspace, &shared, &output] {
        std::fs::create_dir_all(path).unwrap();
    }
    let file = directory.path().join("boxer.json");
    std::fs::write(&file, json!({"version":1,"mode":"workspace","read":["shared", "$WORKSPACE"], "write":["output"], "clean_env":true,"pass_env":["BOXER_CUSTOM"]}).to_string()).unwrap();
    let value = plan(
        &workspace,
        &[
            "--policy",
            file.to_str().unwrap(),
            "--pass-env",
            "ANOTHER_CUSTOM",
        ],
    );
    assert!(
        value["policy"]["read"]
            .as_array()
            .unwrap()
            .contains(&json!(shared.canonicalize().unwrap()))
    );
    assert!(
        value["policy"]["read"]
            .as_array()
            .unwrap()
            .contains(&json!(workspace.canonicalize().unwrap()))
    );
    assert_eq!(
        value["policy"]["write"][0],
        json!(output.canonicalize().unwrap())
    );
    assert_eq!(
        value["policy"]["pass_env"],
        json!(["BOXER_CUSTOM", "ANOTHER_CUSTOM"])
    );
    // Project files never activate a sandbox policy implicitly.
    let value = plan(&workspace, &[]);
    assert_eq!(value["policy"]["mode"], "unrestricted");
    let value = plan(&workspace, &["--read-only", "--workspace"]);
    assert_eq!(value["policy"]["read_only"], true);
}

#[test]
fn invalid_policy_grants_and_environment_names_fail_before_starting_the_program() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("boxer.json");
    for source in [
        "{",
        r#"{"version":2,"mode":"workspace"}"#,
        r#"{"version":1}"#,
        r#"{"version":1,"mode":"workspace","typo":true}"#,
        r#"{"version":1,"mode":"workspace","read":["missing"]}"#,
        r#"{"version":1,"mode":"workspace","read":["$UNKNOWN/path"]}"#,
        r#"{"version":1,"mode":"isolated","cpus":0}"#,
        r#"{"version":1,"mode":"workspace","mode":"unrestricted"}"#,
        r#"{"version":1,"version":1,"mode":"workspace"}"#,
        r#"{"version":1,"mode":"workspace"} {}"#,
    ] {
        std::fs::write(&file, source).unwrap();
        let output = Command::new(binary("boxer"))
            .arg("--policy")
            .arg(&file)
            .args(["--print-policy", "--", "program-not-present"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(125), "{source}");
    }
    for args in [
        vec!["--read", directory.path().to_str().unwrap()],
        vec![
            "--workspace",
            "--read-only",
            "--write",
            directory.path().to_str().unwrap(),
        ],
        vec!["--pass-env", "BAD=NAME"],
        vec!["--profile", "unknown"],
    ] {
        let output = Command::new(binary("boxer"))
            .args(args)
            .arg("--print-policy")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(125));
    }
}

#[cfg(unix)]
#[test]
fn workspace_allowlists_and_explicit_grants_are_inherited_and_block_symlink_escapes() {
    let directory = tempfile::tempdir().unwrap();
    let projects = directory.path().join("projects");
    let workspace = projects.join("project with spaces");
    let readonly = directory.path().join("reference \"quoted\"");
    let writable = directory.path().join("results");
    // Use an unshared parent. Isolated mode has a writable private /tmp; a
    // write at the same basename there would create a private copy, not touch
    // the host's file, and must not be mistaken for a permission escape.
    let private = directory.path().join("host-private");
    std::fs::create_dir(&private).unwrap();
    let hidden = private.join("private.txt");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&writable).unwrap();
    for file in [&readonly, &hidden, &workspace.join("source.txt")] {
        std::fs::write(file, "original").unwrap();
    }
    std::os::unix::fs::symlink(&hidden, workspace.join("escape.txt")).unwrap();
    let cases = json!([
        {"path":workspace.join("source.txt"),"read":true,"write":true},
        {"path":readonly,"read":true,"write":false},
        {"path":writable.join("result.txt"),"write":true},
        {"path":hidden,"read":false,"write":false},
        {"path":workspace.join("escape.txt"),"read":false,"write":false}
    ]);
    #[cfg(target_os = "linux")]
    let modes = ["--workspace", "--isolated"];
    #[cfg(target_os = "macos")]
    let modes = ["--workspace"];
    for mode in modes {
        let output = Command::new(binary("boxer"))
            .args([mode, "--cwd"])
            .arg(&workspace)
            .arg("--read")
            .arg(&readonly)
            .arg("--read")
            .arg(&projects)
            .arg("--read")
            .arg(&workspace)
            .arg("--write")
            .arg(&writable)
            .arg("--")
            .arg(binary("sandbox-probe"))
            .arg("--access-check-descendant")
            .env("SOLMU_TEST_ACCESS", cases.to_string())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{mode}: {:?}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(std::fs::read_to_string(&readonly).unwrap(), "original");
    assert_eq!(std::fs::read_to_string(&hidden).unwrap(), "original");
    let cases = json!([{"path":workspace.join("source.txt"),"read":true,"write":false}]);
    let output = Command::new(binary("boxer"))
        .args(["--workspace", "--read-only", "--cwd"])
        .arg(&workspace)
        .arg("--")
        .arg(binary("sandbox-probe"))
        .arg("--access-check")
        .env("SOLMU_TEST_ACCESS", cases.to_string())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn clean_environment_removes_unrelated_credentials_and_explicit_names_are_forwarded() {
    let directory = tempfile::tempdir().unwrap();
    let output = Command::new(binary("boxer")).args(["--clean-env", "--pass-env", "BOXER_ALLOWED", "--cwd"])
        .arg(directory.path()).arg("--").arg(binary("sandbox-probe")).arg("--access-check")
        .env("SOLMU_TEST_ACCESS", "[]")
        .env("SOLMU_TEST_POLICY_ENV", json!({"BOXER_TEST_SECRET":null,"SSH_AUTH_SOCK":null,"BOXER_ALLOWED":"forwarded","OPENAI_API_KEY":"fixture-key"}).to_string())
        .env("BOXER_TEST_SECRET", "private").env("SSH_AUTH_SOCK", "/private/agent.sock")
        .env("BOXER_ALLOWED", "forwarded").env("OPENAI_API_KEY", "fixture-key")
        .output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(windows)]
#[test]
fn windows_rejects_workspace_policies_without_launching_the_program() {
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("should-not-exist");
    let output = Command::new(binary("boxer"))
        .arg("--workspace")
        .arg("--")
        .arg(binary("sandbox-probe"))
        .arg(&marker)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(125));
    assert!(!marker.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn solmu_backend_runs_with_workspace_permissions_and_bash_cannot_read_host_files() {
    use eventsource_stream::Eventsource;
    use futures_util::StreamExt;
    let backend = solmu_e2e::support::Backend::boxed().await;
    let secret = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(secret.path(), "private host data").unwrap();
    let thread = backend.create_thread("Boxed tools").await;
    let id = thread["id"].as_str().unwrap();
    assert_eq!(
        std::path::PathBuf::from(thread["workspace"].as_str().unwrap()),
        backend.directory.path().canonicalize().unwrap()
    );
    let command = format!(
        "cat '{}'",
        secret
            .path()
            .to_str()
            .unwrap()
            .replace('\\', "\\\\")
            .replace('\'', "'\\''")
    );
    let message = backend.send_message(id, &format!("TOOLS {}", json!([
        {"name":"Write", "arguments":{"path":"project.txt", "content":"workspace allowed"}},
        {"name":"Bash", "arguments":{"command":command}}
    ]))).await;
    let response = backend
        .client
        .post(backend.endpoint(&format!("/api/v1/threads/{id}/responses")))
        .json(&json!({"message_id": message["id"]}))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    let mut events = response.bytes_stream().eventsource();
    let mut completed = false;
    while let Some(event) = events.next().await {
        if event.unwrap().event == "done" {
            completed = true;
        }
    }
    assert!(completed);
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
    assert!(!tools.to_string().contains("private host data"));
    assert_eq!(
        std::fs::read_to_string(backend.directory.path().join("project.txt")).unwrap(),
        "workspace allowed"
    );
}
