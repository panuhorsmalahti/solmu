use super::*;
use serde_json::Value;

fn why(args: &[&str]) -> (std::process::Output, Value) {
    let output = Command::new(binary("boxer")).args(args).output().unwrap();
    let parsed = serde_json::from_slice(&output.stdout).unwrap_or(Value::Null);
    (output, parsed)
}

#[test]
fn why_explains_workspace_grants_denials_and_unrestricted_access() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    let extra = root.path().join("extra");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::create_dir(&extra).unwrap();
    std::fs::write(extra.join("notes.txt"), "local").unwrap();
    let relative = workspace.join("new.txt");

    let (out, result) = why(&[
        "why",
        "--path",
        relative.to_str().unwrap(),
        "--op",
        "write",
        "--workspace",
        "--cwd",
        workspace.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    if cfg!(windows) {
        assert_eq!(result["result"], "unsupported");
    } else {
        assert_eq!(result["result"], "allowed");
    }

    let (out, result) = why(&[
        "why",
        "--path",
        extra.join("notes.txt").to_str().unwrap(),
        "--op",
        "read",
        "--workspace",
        "--read",
        extra.to_str().unwrap(),
        "--cwd",
        workspace.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    if cfg!(windows) {
        assert_eq!(result["result"], "unsupported");
    } else {
        assert_eq!(result["result"], "allowed");
    }

    let (out, result) = why(&[
        "why",
        "--path",
        extra.join("notes.txt").to_str().unwrap(),
        "--op",
        "write",
        "--workspace",
        "--cwd",
        workspace.to_str().unwrap(),
    ]);
    assert!(out.status.success());
    assert_eq!(
        result["result"],
        if cfg!(windows) {
            "unsupported"
        } else {
            "denied"
        }
    );

    let (out, result) = why(&["why", "--path", extra.to_str().unwrap(), "--op", "write"]);
    assert!(out.status.success());
    assert_eq!(result["result"], "allowed");
}

#[test]
fn why_explains_network_access_without_connecting() {
    let (out, result) = why(&["why", "--host", "api.example.com", "--op", "connect"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(result["operation"], "connect");
    assert_eq!(result["host"], "api.example.com");
    assert_eq!(result["port"], 443);
    assert_eq!(result["result"], "allowed");

    let (out, result) = why(&[
        "why",
        "--host",
        "api.example.com:8443",
        "--op",
        "connect",
        "--network",
        "deny",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        result["result"],
        if cfg!(windows) {
            "unsupported"
        } else {
            "denied"
        }
    );

    let root = tempfile::tempdir().unwrap();
    let policy = root.path().join("network-policy.json");
    std::fs::write(&policy, r#"{"version":1,"mode":"isolated","network":"proxy","hosts":["*.example.com"],"deny_hosts":["blocked.example.com"]}"#).unwrap();
    for (host, expected) in [
        ("api.example.com", "allowed"),
        ("example.com", "denied"),
        ("blocked.example.com", "denied"),
        ("outside.test", "denied"),
    ] {
        let (out, result) = why(&[
            "why",
            "--host",
            host,
            "--op",
            "connect",
            "--policy",
            policy.to_str().unwrap(),
        ]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            result["result"],
            if cfg!(target_os = "linux") {
                expected
            } else {
                "unsupported"
            },
            "{host}"
        );
    }

    let (out, result) = why(&[
        "why",
        "--host",
        "127.0.0.1:3000",
        "--op",
        "connect",
        "--network",
        "deny",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(result["port"], 3000);
    assert_eq!(
        result["result"],
        if cfg!(windows) {
            "unsupported"
        } else {
            "denied"
        }
    );
}
