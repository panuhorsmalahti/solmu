use super::*;

#[test]
fn rust_runtime_group_grants_detected_toolchain_directories_read_only() {
    let temp = tempfile::tempdir().unwrap();
    let cargo = temp.path().join("cargo-home");
    let rustup = temp.path().join("rustup-home");
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    for path in ["bin", "registry", "git"] {
        std::fs::create_dir_all(cargo.join(path)).unwrap();
    }
    std::fs::create_dir_all(&rustup).unwrap();
    let output = Command::new(binary("boxer"))
        .args([
            "--workspace",
            "--runtime-group",
            "rust",
            "--cwd",
            workspace.to_str().unwrap(),
            "--print-policy",
            "--",
            "unused-program",
        ])
        .env("CARGO_HOME", &cargo)
        .env("RUSTUP_HOME", &rustup)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["policy"]["runtime_groups"][0], "rust");
    let read = result["policy"]["read"].as_array().unwrap();
    for path in [
        cargo.join("bin"),
        cargo.join("registry"),
        cargo.join("git"),
        rustup,
    ] {
        let expected = path.canonicalize().unwrap().to_string_lossy().to_string();
        assert!(read.iter().any(|item| item.as_str() == Some(&expected)));
    }
    let cargo_home = cargo.canonicalize().unwrap().to_string_lossy().to_string();
    assert!(!read.iter().any(|item| item.as_str() == Some(&cargo_home)));
}

#[test]
fn runtime_groups_require_a_restricted_mode_and_known_group_name() {
    let unrestricted = Command::new(binary("boxer"))
        .args(["--runtime-group", "node", "--print-policy", "--", "unused"])
        .output()
        .unwrap();
    assert_eq!(unrestricted.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&unrestricted.stderr).contains("Runtime groups require"));

    let unknown = Command::new(binary("boxer"))
        .args([
            "--workspace",
            "--runtime-group",
            "ruby",
            "--print-policy",
            "--",
            "unused",
        ])
        .output()
        .unwrap();
    assert_eq!(unknown.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("Unknown runtime group"));
}
