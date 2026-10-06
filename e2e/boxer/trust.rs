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
