use super::*;
use serde_json::{Value, json};

#[test]
fn learn_traces_filesystem_access_and_emits_json_discovery() {
    let workspace = tempfile::tempdir().unwrap();
    let input = workspace.path().join("input.txt");
    let output = workspace.path().join("output.txt");
    std::fs::write(&input, "fixture input").unwrap();

    let result = Command::new(binary("boxer"))
        .args(["learn", "--json", "--"])
        .arg(binary("sandbox-probe"))
        .arg("--learn-fixture")
        .arg(&input)
        .arg(&output)
        .current_dir(workspace.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(report.get("policy_gaps").is_none());
    assert!(
        report["filesystem"]["read"]
            .as_array()
            .unwrap()
            .contains(&json!(input))
    );
    assert!(
        report["filesystem"]["write"]
            .as_array()
            .unwrap()
            .contains(&json!(output))
    );
    assert_eq!(report["command_exit_code"], 0);
    assert_eq!(std::fs::read(&output).unwrap(), b"learned write");
    assert!(String::from_utf8_lossy(&result.stderr).contains("learn fixture stdout"));
    assert!(String::from_utf8_lossy(&result.stderr).contains("learn fixture stderr"));
}

#[test]
fn learn_compares_discovered_access_with_a_resolved_boxer_policy() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let input = root.path().join("external-input.txt");
    let output = root.path().join("external-output.txt");
    std::fs::write(&input, "outside the granted workspace").unwrap();
    let policy = root.path().join("policy.json");
    std::fs::write(
        &policy,
        r#"{"version":1,"mode":"workspace","network":"deny"}"#,
    )
    .unwrap();

    let result = Command::new(binary("boxer"))
        .args(["learn", "--json", "--policy"])
        .arg(&policy)
        .args(["--"])
        .arg(binary("sandbox-probe"))
        .arg("--learn-fixture")
        .arg(&input)
        .arg(&output)
        .current_dir(&workspace)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(
        report["policy_gaps"]["filesystem"]["read"]
            .as_array()
            .unwrap()
            .contains(&json!(input))
    );
    assert!(
        report["policy_gaps"]["filesystem"]["write"]
            .as_array()
            .unwrap()
            .contains(&json!(output))
    );
    assert!(
        report["policy_gaps"]["network"]["outbound_denied"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        report["policy_gaps"]["network"]["listening_denied"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let by_profile = Command::new(binary("boxer"))
        .args(["learn", "--json", "--profile", "policy", "--"])
        .arg(binary("sandbox-probe"))
        .arg("--learn-fixture")
        .arg(&input)
        .arg(&output)
        .current_dir(&workspace)
        .env("BOXER_PROFILE_DIR", root.path())
        .output()
        .unwrap();
    assert!(
        by_profile.status.success(),
        "{}",
        String::from_utf8_lossy(&by_profile.stderr)
    );
    let by_profile: Value = serde_json::from_slice(&by_profile.stdout).unwrap();
    assert_eq!(
        by_profile["policy_gaps"]["filesystem"]["read"],
        report["policy_gaps"]["filesystem"]["read"]
    );
}

#[test]
fn learn_counts_repeated_network_access_to_each_endpoint() {
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let result = Command::new(binary("boxer"))
        .args(["learn", "--json", "--"])
        .arg(binary("sandbox-probe"))
        .arg("--learn-network-fixture")
        .arg(address.to_string())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    for _ in 0..2 {
        let (stream, _) = listener.accept().unwrap();
        drop(stream);
    }
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    let outbound = report["network"]["outbound"].as_array().unwrap();
    assert_eq!(outbound.len(), 1);
    assert_eq!(outbound[0]["address"], "127.0.0.1");
    assert_eq!(outbound[0]["port"], address.port());
    assert_eq!(outbound[0]["count"], 2);
}

#[test]
fn learn_correlates_dns_answers_with_outbound_connections() {
    use std::net::{TcpListener, UdpSocket};

    let root = tempfile::tempdir().unwrap();
    let resolver = UdpSocket::bind("127.0.0.1:0").unwrap();
    resolver
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    let resolver_address = resolver.local_addr().unwrap();
    let destination = TcpListener::bind("127.0.0.1:0").unwrap();
    let destination_port = destination.local_addr().unwrap().port();
    let policy = root.path().join("policy.json");
    std::fs::write(
        &policy,
        serde_json::json!({
            "version": 1,
            "mode": "isolated",
            "network": "proxy",
            "hosts": [format!("agent.solmu.test:{destination_port}")]
        })
        .to_string(),
    )
    .unwrap();
    let responder = std::thread::spawn(move || {
        let mut request = [0u8; 512];
        let (length, peer) = resolver.recv_from(&mut request).unwrap();
        let mut question_end = 12;
        loop {
            let label_length = request[question_end] as usize;
            question_end += 1;
            if label_length == 0 {
                break;
            }
            question_end += label_length;
        }
        question_end += 4;
        let mut response = Vec::from(&request[..2]);
        response.extend_from_slice(&[0x81, 0x80, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00]);
        response.extend_from_slice(&request[12..question_end]);
        response.extend_from_slice(&[
            0xc0, 0x0c, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x3c, 0x00, 0x04, 127, 0, 0, 1,
        ]);
        resolver.send_to(&response, peer).unwrap();
        assert_eq!(length, question_end);
    });
    let result = Command::new(binary("boxer"))
        .args(["learn", "--json", "--timeout", "10", "--policy"])
        .arg(&policy)
        .arg("--")
        .arg(binary("sandbox-probe"))
        .arg("--learn-dns-fixture")
        .arg(resolver_address.to_string())
        .arg(destination_port.to_string())
        .output()
        .unwrap();
    responder.join().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    let outbound = report["network"]["outbound"].as_array().unwrap();
    assert!(outbound.iter().any(|endpoint| {
        endpoint["address"] == "127.0.0.1"
            && endpoint["port"] == destination_port
            && endpoint["hostname"] == "agent.solmu.test"
    }));
    assert!(
        report["policy_gaps"]["network"]["outbound_denied"]
            .as_array()
            .unwrap()
            .iter()
            .all(|endpoint| endpoint["port"] != destination_port)
    );
    let _ = destination.accept();
}

#[test]
fn learn_requires_a_command_and_rejects_invalid_timeouts() {
    for arguments in [
        vec!["learn"],
        vec!["learn", "--", "sandbox-probe"],
        vec!["learn", "--timeout", "0", "--", "sandbox-probe"],
    ] {
        let result = Command::new(binary("boxer"))
            .args(arguments)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(125));
    }
}

#[test]
fn learn_timeout_stops_the_traced_process_group() {
    let result = Command::new(binary("boxer"))
        .args(["learn", "--json", "--timeout", "1", "--"])
        .arg(binary("sandbox-probe"))
        .arg("--sleep")
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(124));
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["timed_out"], true);
}
