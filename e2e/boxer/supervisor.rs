use super::*;
use std::{io::BufRead, process::Stdio, thread, time::Instant};

#[test]
fn supervised_network_approval_can_grant_a_target_for_the_session() {
    let workspace = tempfile::tempdir().unwrap();
    let mut child = Command::new(binary("boxer"))
        .args(["--isolated", "--network", "proxy", "--supervised", "--cwd"])
        .arg(workspace.path())
        .arg("--")
        .arg(binary("sandbox-probe"))
        .args(["--proxy-connect", "supervisor-fixture.invalid:443", "2"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stderr = std::io::BufReader::new(child.stderr.take().unwrap());
    let mut first_line = String::new();
    stderr.read_line(&mut first_line).unwrap();
    let session = first_line
        .strip_prefix("Boxer supervisor session: ")
        .and_then(|value| value.split_whitespace().next())
        .expect("supervisor session ID")
        .to_owned();

    let deadline = Instant::now() + std::time::Duration::from_secs(15);
    let request_id = loop {
        let output = Command::new(binary("boxer"))
            .args(["supervisor", &session, "list", "--json"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        if let Some(request) = response["items"].as_array().unwrap().first() {
            assert_eq!(request["target"]["host"], "supervisor-fixture.invalid");
            assert_eq!(request["target"]["port"], 443);
            break request["id"].as_str().unwrap().to_owned();
        }
        assert!(
            Instant::now() < deadline,
            "network approval was not requested"
        );
        thread::sleep(std::time::Duration::from_millis(25));
    };

    let approved = Command::new(binary("boxer"))
        .args(["supervisor", &session, "approve", &request_id, "--session"])
        .output()
        .unwrap();
    assert!(
        approved.status.success(),
        "{}",
        String::from_utf8_lossy(&approved.stderr)
    );

    let deadline = Instant::now() + std::time::Duration::from_secs(15);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "supervised probe exited with {status}");
            break;
        }
        let output = Command::new(binary("boxer"))
            .args(["supervisor", &session, "list", "--json"])
            .output()
            .unwrap();
        if output.status.success() {
            let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            if let Some(request) = response["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|request| request["id"].as_str() != Some(&request_id))
            {
                let _ = Command::new(binary("boxer"))
                    .args([
                        "supervisor",
                        &session,
                        "deny",
                        request["id"].as_str().unwrap(),
                    ])
                    .output();
                let _ = child.kill();
                panic!("session approval prompted again for the same target");
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            panic!("supervised probe did not finish");
        }
        thread::sleep(std::time::Duration::from_millis(25));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .matches("HTTP/1.1 403")
            .count()
            >= 2,
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}
