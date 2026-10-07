use super::binary;
use std::process::Command;

#[test]
fn completion_generates_shell_scripts_with_muxer_subcommands() {
    for (shell, marker) in [
        ("bash", "complete -F _muxer_completions muxer"),
        ("zsh", "#compdef muxer"),
        ("fish", "complete -c muxer"),
        ("powershell", "Register-ArgumentCompleter"),
    ] {
        let output = Command::new(binary("muxer"))
            .args(["completion", shell])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "completion for {shell} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let script = String::from_utf8(output.stdout).unwrap();
        assert!(script.contains(marker), "missing {marker:?} for {shell}");
        assert!(script.contains("space tab pane"));
        assert!(script.contains("completion"));
    }

    let unsupported = Command::new(binary("muxer"))
        .args(["completion", "tcsh"])
        .output()
        .unwrap();
    assert!(!unsupported.status.success());
    assert!(
        String::from_utf8_lossy(&unsupported.stderr)
            .contains("Choose bash, zsh, fish, or powershell")
    );
}
