use super::*;
use serde_json::{Value, json};

#[test]
fn network_policy_files_previews_and_overrides_are_explicit_and_validated() {
    let directory = tempfile::tempdir().unwrap();
    let policy = directory.path().join("offline.json");
    std::fs::write(
        &policy,
        json!({"version":1,"mode":"unrestricted","network":"deny"}).to_string(),
    )
    .unwrap();
    for allow in [false, true] {
        let mut command = Command::new(binary("boxer"));
        command.arg("--policy").arg(&policy);
        if allow {
            command.args(["--network", "allow"]);
        }
        let output = command
            .args(["--print-policy", "--", "missing-program"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["network"], if allow { "allowed" } else { "denied" });
        assert_eq!(
            value["policy"]["network"],
            if allow { "allow" } else { "deny" }
        );
        assert_eq!(value["platform_supported"], allow || !cfg!(windows));
    }
    for args in [
        vec!["--network"],
        vec!["--network", "offline"],
        vec!["--network", "local"],
    ] {
        assert_eq!(
            Command::new(binary("boxer"))
                .args(args)
                .output()
                .unwrap()
                .status
                .code(),
            Some(125)
        );
    }
    std::fs::write(
        &policy,
        r#"{"version":1,"mode":"workspace","network":"unknown"}"#,
    )
    .unwrap();
    assert_eq!(
        Command::new(binary("boxer"))
            .arg("--policy")
            .arg(&policy)
            .arg("--print-policy")
            .output()
            .unwrap()
            .status
            .code(),
        Some(125)
    );
}

#[cfg(unix)]
#[test]
fn offline_policy_blocks_tcp_udp_and_unix_sockets_in_threads_and_descendants() {
    use std::net::{TcpListener, UdpSocket};
    use std::os::unix::net::UnixListener;
    let directory = tempfile::tempdir().unwrap();
    let ipv4 = TcpListener::bind("127.0.0.1:0").unwrap();
    let ipv6 = TcpListener::bind("[::1]:0").unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
    let unix = UnixListener::bind(directory.path().join("host.sock")).unwrap();
    ipv4.set_nonblocking(true).unwrap();
    ipv6.set_nonblocking(true).unwrap();
    udp.set_nonblocking(true).unwrap();
    unix.set_nonblocking(true).unwrap();
    let mut modes = vec![
        vec![],
        vec!["--workspace"],
        vec!["--workspace", "--read-only"],
    ];
    if cfg!(target_os = "linux") {
        modes.push(vec!["--isolated"]);
    }
    for mode in modes {
        let output = Command::new(binary("boxer"))
            .args(&mode)
            .args(["--network", "deny", "--cwd"])
            .arg(directory.path())
            .arg("--")
            .arg(binary("sandbox-probe"))
            .arg("--network-check-descendant")
            .env("SOLMU_TEST_TCP4", ipv4.local_addr().unwrap().to_string())
            .env("SOLMU_TEST_TCP6", ipv6.local_addr().unwrap().to_string())
            .env("SOLMU_TEST_UNIX", directory.path().join("host.sock"))
            .env(
                "HTTP_PROXY",
                format!("http://{}", ipv4.local_addr().unwrap()),
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{mode:?}: {:?}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("socket networking denied"));
        assert_eq!(
            ipv4.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        assert_eq!(
            ipv6.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        assert_eq!(
            unix.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        assert_eq!(
            udp.recv_from(&mut [0u8; 32]).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}

#[cfg(unix)]
#[test]
fn offline_shell_writes_the_workspace_and_curl_cannot_reach_a_live_server() {
    let directory = tempfile::tempdir().unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    std::fs::write(directory.path().join("script.sh"), format!(
        "set -eu\nprintf 'workspace allowed' > offline.txt\nif /usr/bin/curl --noproxy '*' --silent --show-error --max-time 2 http://{}/; then exit 4; fi\n",
        listener.local_addr().unwrap()
    )).unwrap();
    let output = Command::new(binary("boxer"))
        .args(["--workspace", "--network", "deny", "--cwd"])
        .arg(directory.path())
        .args(["--", "/bin/sh", "./script.sh"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{:?}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(directory.path().join("offline.txt")).unwrap(),
        "workspace allowed"
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[cfg(unix)]
#[test]
fn socket_standard_io_is_rejected_before_the_program_runs() {
    use std::{
        net::{TcpListener, TcpStream},
        os::fd::OwnedFd,
        process::Stdio,
    };
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("must-not-run");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let descriptor: OwnedFd = stream.into();
    let output = Command::new(binary("boxer"))
        .args(["--network", "deny", "--"])
        .arg(binary("sandbox-probe"))
        .arg(&marker)
        .stdin(Stdio::from(descriptor))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&output.stderr).contains("socket-based standard I/O"));
    assert!(!marker.exists());
}

#[cfg(unix)]
#[test]
fn offline_exec_closes_inherited_socket_descriptors() {
    use std::{
        net::{TcpListener, TcpStream},
        os::{fd::AsRawFd, unix::process::CommandExt},
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let descriptor = stream.as_raw_fd();
    // Linux directly execs the program, so also prove that its existing default
    // descriptor-passing behavior is retained. macOS delegates exec to its
    // native launcher; only the offline guarantee is required there.
    let modes = if cfg!(target_os = "linux") {
        vec![false, true]
    } else {
        vec![true]
    };
    for deny in modes {
        let mut command = Command::new(binary("boxer"));
        if deny {
            command.args(["--network", "deny"]);
        }
        command
            .arg("--")
            .arg(binary("sandbox-probe"))
            .arg("--check-inherited")
            .env("SOLMU_TEST_DESCRIPTOR", descriptor.to_string())
            .env(
                "SOLMU_TEST_DESCRIPTOR_CLOSED",
                if deny { "yes" } else { "no" },
            );
        // SAFETY: this post-fork callback only updates flags of the known live
        // descriptor; the parent retains its original ownership and flags.
        unsafe {
            command.pre_exec(move || {
                if libc::fcntl(descriptor, libc::F_SETFD, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[cfg(windows)]
#[test]
fn unsupported_network_controls_fail_before_launching_on_windows() {
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("must-not-run");
    let output = Command::new(binary("boxer"))
        .args(["--network", "deny", "--"])
        .arg(binary("sandbox-probe"))
        .arg(&marker)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(125));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Network restrictions are not supported")
    );
    assert!(!marker.exists());
}
