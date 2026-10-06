#![cfg(unix)]

use super::*;
use portable_pty::{Child, CommandBuilder, PtySize, native_pty_system};
use std::{
    io::{Read, Write},
    sync::mpsc,
};

fn wait_for_output(
    output: &mpsc::Receiver<Vec<u8>>,
    collected: &mut Vec<u8>,
    needle: &[u8],
) -> Result<(), mpsc::RecvTimeoutError> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !collected.windows(needle.len()).any(|part| part == needle) {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        let bytes = output.recv_timeout(remaining)?;
        collected.extend(bytes);
    }
    Ok(())
}

fn assert_output(
    output: &mpsc::Receiver<Vec<u8>>,
    collected: &mut Vec<u8>,
    needle: &[u8],
    child: &mut (dyn Child + Send + Sync),
    session_dir: &std::path::Path,
    id: &str,
) {
    if let Err(error) = wait_for_output(output, collected, needle) {
        let status = child.try_wait().ok().flatten();
        let stderr =
            std::fs::read_to_string(session_dir.join(format!("{id}.err"))).unwrap_or_default();
        panic!(
            "timed out waiting for attached terminal output ({error}); attach status: {status:?}; session stderr: {stderr:?}; attach output: {}",
            String::from_utf8_lossy(collected),
        );
    }
}

fn wait_for_status(directory: &std::path::Path, id: &str, status: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        let inspect = Command::new(binary("boxer"))
            .args(["inspect", id, "--json"])
            .env("BOXER_SESSIONS_DIR", directory)
            .output()
            .unwrap();
        if inspect.status.success()
            && serde_json::from_slice::<serde_json::Value>(&inspect.stdout)
                .is_ok_and(|session| session["status"] == status)
        {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Boxer session {id} did not reach status {status}"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn detached_sessions_can_reattach_interactively_detach_stop_and_prune() {
    let directory = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let start = Command::new(binary("boxer"))
        .args(["--detached", "--cwd"])
        .arg(workspace.path())
        .args([
            "--",
            "/bin/sh",
            "-c",
            "printf 'session-ready\\n'; IFS= read -r input; printf 'received:%s\\n' \"$input\"; exec sleep 60",
        ])
        .env("BOXER_SESSIONS_DIR", directory.path())
        .output()
        .unwrap();
    assert!(
        start.status.success(),
        "{}",
        String::from_utf8_lossy(&start.stderr)
    );
    let text = String::from_utf8_lossy(&start.stdout);
    let id = text
        .lines()
        .find_map(|line| line.strip_prefix("Detached Boxer session: "))
        .unwrap();

    let inspect = Command::new(binary("boxer"))
        .args(["inspect", id])
        .env("BOXER_SESSIONS_DIR", directory.path())
        .output()
        .unwrap();
    assert!(inspect.status.success());
    assert!(String::from_utf8_lossy(&inspect.stdout).contains("Status: running"));
    assert!(String::from_utf8_lossy(&inspect.stdout).contains("Attachment: detached"));
    assert!(String::from_utf8_lossy(&inspect.stdout).contains(workspace.path().to_str().unwrap()));

    let ps = Command::new(binary("boxer"))
        .arg("ps")
        .env("BOXER_SESSIONS_DIR", directory.path())
        .output()
        .unwrap();
    assert!(ps.status.success());
    assert!(String::from_utf8_lossy(&ps.stdout).contains(id));
    let ps_json = Command::new(binary("boxer"))
        .args(["ps", "--json"])
        .env("BOXER_SESSIONS_DIR", directory.path())
        .output()
        .unwrap();
    assert!(ps_json.status.success());
    let listed: serde_json::Value = serde_json::from_slice(&ps_json.stdout).unwrap();
    assert_eq!(listed[0]["id"], id);
    assert_eq!(listed[0]["attached"], false);
    let inspect_json = Command::new(binary("boxer"))
        .args(["inspect", id, "--json"])
        .env("BOXER_SESSIONS_DIR", directory.path())
        .output()
        .unwrap();
    assert!(inspect_json.status.success());
    let details: serde_json::Value = serde_json::from_slice(&inspect_json.stdout).unwrap();
    let workspace_path = workspace.path().canonicalize().unwrap();
    assert_eq!(details["workspace"], workspace_path.to_str().unwrap());
    assert_eq!(details["attached"], false);

    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut attach_command = CommandBuilder::new(binary("boxer"));
    attach_command.args(["attach", id]);
    attach_command.env("BOXER_SESSIONS_DIR", directory.path());
    let mut attach_child = pair.slave.spawn_command(attach_command).unwrap();
    drop(pair.slave);
    let mut writer = pair.master.take_writer().unwrap();
    let mut reader = pair.master.try_clone_reader().unwrap();
    let (output_tx, output_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buffer = [0; 2048];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    if output_tx.send(buffer[..count].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });
    let mut attached_output = Vec::new();
    assert_output(
        &output_rx,
        &mut attached_output,
        b"session-ready",
        attach_child.as_mut(),
        directory.path(),
        id,
    );
    let attached_inspect = Command::new(binary("boxer"))
        .args(["inspect", id])
        .env("BOXER_SESSIONS_DIR", directory.path())
        .output()
        .unwrap();
    assert!(attached_inspect.status.success());
    assert!(String::from_utf8_lossy(&attached_inspect.stdout).contains("Attachment: attached"));
    writer.write_all(b"hello\r").unwrap();
    assert_output(
        &output_rx,
        &mut attached_output,
        b"received:hello",
        attach_child.as_mut(),
        directory.path(),
        id,
    );
    writer.write_all(&[0x1d]).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(30));
    writer.write_all(b"d").unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if attach_child.try_wait().unwrap().is_some() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "attach did not detach"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let detached_inspect = Command::new(binary("boxer"))
        .args(["inspect", id])
        .env("BOXER_SESSIONS_DIR", directory.path())
        .output()
        .unwrap();
    assert!(detached_inspect.status.success());
    assert!(String::from_utf8_lossy(&detached_inspect.stdout).contains("Attachment: detached"));

    let mut log_found = false;
    for _ in 0..40 {
        let output = Command::new(binary("boxer"))
            .args(["sessions", "logs", id])
            .env("BOXER_SESSIONS_DIR", directory.path())
            .output()
            .unwrap();
        assert!(output.status.success());
        if String::from_utf8_lossy(&output.stdout).contains("received:hello") {
            log_found = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert!(
        log_found,
        "detached session did not preserve terminal output"
    );

    let stop = Command::new(binary("boxer"))
        .args(["stop", id, "--timeout", "2"])
        .env("BOXER_SESSIONS_DIR", directory.path())
        .output()
        .unwrap();
    assert!(
        stop.status.success(),
        "{}",
        String::from_utf8_lossy(&stop.stderr)
    );
    assert!(String::from_utf8_lossy(&stop.stdout).contains("stopped"));
    let running_only = Command::new(binary("boxer"))
        .args(["ps", "--json"])
        .env("BOXER_SESSIONS_DIR", directory.path())
        .output()
        .unwrap();
    let running: serde_json::Value = serde_json::from_slice(&running_only.stdout).unwrap();
    assert!(running.as_array().unwrap().is_empty());
    let all_sessions = Command::new(binary("boxer"))
        .args(["ps", "--all", "--json"])
        .env("BOXER_SESSIONS_DIR", directory.path())
        .output()
        .unwrap();
    let all: serde_json::Value = serde_json::from_slice(&all_sessions.stdout).unwrap();
    assert_eq!(all[0]["status"], "stopped");

    let recent_prune = Command::new(binary("boxer"))
        .args(["prune", "--older-than", "1", "--dry-run"])
        .env("BOXER_SESSIONS_DIR", directory.path())
        .output()
        .unwrap();
    assert!(recent_prune.status.success());
    assert!(String::from_utf8_lossy(&recent_prune.stdout).contains("Would remove 0"));
    let preview = Command::new(binary("boxer"))
        .args(["prune", "--keep", "0", "--dry-run"])
        .env("BOXER_SESSIONS_DIR", directory.path())
        .output()
        .unwrap();
    assert!(preview.status.success());
    assert!(String::from_utf8_lossy(&preview.stdout).contains("Would remove 1"));
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 3);
    let prune = Command::new(binary("boxer"))
        .args(["prune", "--keep", "0"])
        .env("BOXER_SESSIONS_DIR", directory.path())
        .output()
        .unwrap();
    assert!(prune.status.success());
    assert!(String::from_utf8_lossy(&prune.stdout).contains("Removed 1 finished session"));
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);

    let force_start = Command::new(binary("boxer"))
        .args(["--detached", "--cwd"])
        .arg(workspace.path())
        .args(["--", "/bin/sh", "-c", "exec sleep 60"])
        .env("BOXER_SESSIONS_DIR", directory.path())
        .output()
        .unwrap();
    assert!(force_start.status.success());
    let force_start_text = String::from_utf8_lossy(&force_start.stdout);
    let force_id = force_start_text
        .lines()
        .find_map(|line| line.strip_prefix("Detached Boxer session: "))
        .unwrap();
    let pause = Command::new(binary("boxer"))
        .args(["pause", force_id])
        .env("BOXER_SESSIONS_DIR", directory.path())
        .output()
        .unwrap();
    assert!(pause.status.success());
    wait_for_status(directory.path(), force_id, "paused");
    let resume = Command::new(binary("boxer"))
        .args(["resume", force_id])
        .env("BOXER_SESSIONS_DIR", directory.path())
        .output()
        .unwrap();
    assert!(resume.status.success());
    wait_for_status(directory.path(), force_id, "running");
    let force_stop = Command::new(binary("boxer"))
        .args(["stop", force_id, "--force"])
        .env("BOXER_SESSIONS_DIR", directory.path())
        .output()
        .unwrap();
    assert!(
        force_stop.status.success(),
        "{}",
        String::from_utf8_lossy(&force_stop.stderr)
    );
    assert!(String::from_utf8_lossy(&force_stop.stdout).contains("stopped"));
}
