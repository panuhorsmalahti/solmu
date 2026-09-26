use super::*;

#[test]
fn muxer_help_version_and_invalid_startup() {
    let directory = tempfile::tempdir().unwrap();
    let mut too_many = std::process::Command::new(binary("muxer"));
    for _ in 0..9 {
        too_many.arg("--cwd").arg(directory.path());
    }
    let output = too_many.output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("eight spaces"));
    for option in ["--help", "--version"] {
        let output = std::process::Command::new(binary("muxer"))
            .arg(option)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("Solmu muxer"));
    }
    for args in [
        vec!["--unknown"],
        vec!["--cwd"],
        vec!["--cwd", "solmu-missing-workspace"],
    ] {
        assert!(
            !std::process::Command::new(binary("muxer"))
                .args(args)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
}
