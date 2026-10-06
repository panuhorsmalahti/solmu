use super::*;

#[test]
fn trust_signatures_verify_and_block_tampered_files_before_launch() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let private = root.path().join("private.pk8");
    let public = root.path().join("trusted.pub");
    let instructions = workspace.join("AGENTS.md");
    std::fs::write(&instructions, "trusted agent instructions\n").unwrap();

    let generated = Command::new(binary("boxer"))
        .args(["trust", "keygen", "--private-key"])
        .arg(&private)
        .arg("--public-key")
        .arg(&public)
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let signed = Command::new(binary("boxer"))
        .args(["trust", "sign", "--key"])
        .arg(&private)
        .arg(&instructions)
        .output()
        .unwrap();
    assert!(
        signed.status.success(),
        "{}",
        String::from_utf8_lossy(&signed.stderr)
    );
    let verified = Command::new(binary("boxer"))
        .args(["trust", "verify", "--key"])
        .arg(&public)
        .arg(&instructions)
        .output()
        .unwrap();
    assert!(
        verified.status.success(),
        "{}",
        String::from_utf8_lossy(&verified.stderr)
    );

    let launched = Command::new(binary("boxer"))
        .args(["--trust-key"])
        .arg(&public)
        .args(["--verify"])
        .arg(&instructions)
        .arg("--")
        .arg(binary("sandbox-probe"))
        .arg("valid-run.txt")
        .current_dir(&workspace)
        .output()
        .unwrap();
    assert_eq!(launched.status.code(), Some(7));
    assert!(workspace.join("valid-run.txt").exists());

    std::fs::write(&instructions, "tampered instructions\n").unwrap();
    let rejected = Command::new(binary("boxer"))
        .args(["--trust-key"])
        .arg(&public)
        .args(["--verify"])
        .arg(&instructions)
        .arg("--")
        .arg(binary("sandbox-probe"))
        .arg("must-not-run.txt")
        .current_dir(&workspace)
        .output()
        .unwrap();
    assert_eq!(rejected.status.code(), Some(125));
    assert!(!workspace.join("must-not-run.txt").exists());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("changed after signing"));
}

#[test]
fn signed_trust_policy_requires_every_listed_file_before_startup() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let private = root.path().join("private.pk8");
    let public = root.path().join("trusted.pub");
    let instructions = workspace.join("AGENTS.md");
    let policy = workspace.join("boxer-trust.json");
    std::fs::write(&instructions, "trusted instructions").unwrap();
    std::fs::write(&policy, r#"{"version":1,"files":["AGENTS.md"]}"#).unwrap();
    assert!(
        Command::new(binary("boxer"))
            .args(["trust", "keygen", "--private-key"])
            .arg(&private)
            .arg("--public-key")
            .arg(&public)
            .output()
            .unwrap()
            .status
            .success()
    );
    for file in [&instructions, &policy] {
        let signed = Command::new(binary("boxer"))
            .args(["trust", "sign", "--key"])
            .arg(&private)
            .arg(file)
            .output()
            .unwrap();
        assert!(
            signed.status.success(),
            "{}",
            String::from_utf8_lossy(&signed.stderr)
        );
    }
    let run = |marker: &str| {
        Command::new(binary("boxer"))
            .arg("--trust-key")
            .arg(&public)
            .args(["--trust-policy", "boxer-trust.json", "--cwd"])
            .arg(&workspace)
            .arg("--")
            .arg(binary("sandbox-probe"))
            .arg(marker)
            .output()
            .unwrap()
    };
    let accepted = run("accepted.txt");
    assert_eq!(
        accepted.status.code(),
        Some(7),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    assert!(workspace.join("accepted.txt").exists());

    std::fs::write(&policy, r#"{"version":1,"files":[]}"#).unwrap();
    let rejected = run("must-not-start.txt");
    assert_eq!(rejected.status.code(), Some(125));
    assert!(!workspace.join("must-not-start.txt").exists());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("changed after signing"));
}

#[test]
fn signed_workspace_trust_policy_is_discovered_and_verified_automatically() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    let trust_dir = root.path().join("trust");
    std::fs::create_dir(&workspace).unwrap();
    let instructions = workspace.join("AGENTS.md");
    let policy = workspace.join("boxer-trust.json");
    std::fs::write(&instructions, "trusted instructions").unwrap();
    std::fs::write(&policy, r#"{"version":1,"files":["AGENTS.md"]}"#).unwrap();

    let generated = Command::new(binary("boxer"))
        .args(["trust", "keygen"])
        .env("BOXER_TRUST_DIR", &trust_dir)
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    for file in [&instructions, &policy] {
        let signed = Command::new(binary("boxer"))
            .args(["trust", "sign"])
            .arg(file)
            .env("BOXER_TRUST_DIR", &trust_dir)
            .output()
            .unwrap();
        assert!(
            signed.status.success(),
            "{}",
            String::from_utf8_lossy(&signed.stderr)
        );
    }
    let run = |marker: &str| {
        Command::new(binary("boxer"))
            .args(["--workspace", "--cwd"])
            .arg(&workspace)
            .arg("--")
            .arg(binary("sandbox-probe"))
            .arg(marker)
            .env("BOXER_TRUST_DIR", &trust_dir)
            .output()
            .unwrap()
    };
    let accepted = run("auto-verified.txt");
    assert_eq!(
        accepted.status.code(),
        Some(7),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    assert!(workspace.join("auto-verified.txt").exists());

    std::fs::write(&instructions, "modified instructions").unwrap();
    let rejected = run("must-not-run.txt");
    assert_eq!(rejected.status.code(), Some(125));
    assert!(!workspace.join("must-not-run.txt").exists());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("changed after signing"));
}
