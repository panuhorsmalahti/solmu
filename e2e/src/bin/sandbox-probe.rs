use std::{
    io::{Read, Write},
    net::TcpStream,
};

fn main() {
    let path = std::env::args().nth(1).expect("probe output path");
    #[cfg(unix)]
    if path == "--network-check" || path == "--network-check-descendant" {
        network_check();
        if path == "--network-check-descendant" {
            std::thread::spawn(network_check).join().unwrap();
            assert!(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .arg("--network-check")
                    .status()
                    .unwrap()
                    .success()
            );
        }
        return;
    }
    #[cfg(unix)]
    if path == "--check-inherited" {
        let descriptor: libc::c_int = std::env::var("SOLMU_TEST_DESCRIPTOR")
            .unwrap()
            .parse()
            .unwrap();
        let closed = unsafe { libc::fcntl(descriptor, libc::F_GETFD) } < 0;
        assert_eq!(
            closed,
            std::env::var("SOLMU_TEST_DESCRIPTOR_CLOSED").unwrap() == "yes"
        );
        return;
    }
    if path == "--access-check" || path == "--access-check-descendant" {
        access_check();
        if path == "--access-check-descendant" {
            assert!(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .arg("--access-check")
                    .status()
                    .unwrap()
                    .success()
            );
        }
        return;
    }
    #[cfg(target_os = "linux")]
    if path == "--limits" {
        linux_limits();
        return;
    }
    if path == "--sleep" {
        std::thread::sleep(std::time::Duration::from_secs(30));
        return;
    }
    if path == "--memory-limit" {
        let mut bytes = vec![0u8; 256 * 1024 * 1024];
        for byte in bytes.iter_mut().step_by(4096) {
            *byte = 1;
        }
        std::hint::black_box(bytes);
        panic!("memory limit was not enforced");
    }
    if path == "--delayed-write" {
        std::thread::sleep(std::time::Duration::from_secs(1));
        std::fs::write(std::env::args().nth(2).unwrap(), "descendant escaped").unwrap();
        return;
    }
    if path == "--spawn-descendant" {
        spawn_descendant();
        std::process::exit(7);
    }
    if std::env::var_os("SOLMU_TEST_ISOLATION").is_some() {
        let hidden = std::env::var("SOLMU_TEST_HIDDEN_FILE").unwrap();
        assert!(
            !std::path::Path::new(&hidden).exists(),
            "host files must not be visible"
        );
        let host_pid = std::env::var("SOLMU_TEST_HOST_PID").unwrap();
        assert!(
            !std::path::Path::new(&format!("/proc/{host_pid}")).exists(),
            "host process must not be visible"
        );
        let status = std::fs::read_to_string("/proc/self/status").unwrap();
        assert!(status.contains("CapEff:\t0000000000000000"));
        assert!(status.contains("NoNewPrivs:\t1"));
        #[cfg(target_os = "linux")]
        {
            assert!(status.contains("Seccomp:\t2"));
            for name in ["CapPrm", "CapInh", "CapAmb"] {
                assert!(status.contains(&format!("{name}:\t0000000000000000")));
            }
            assert_eq!(
                std::fs::read_to_string("/proc/self/cgroup").unwrap().trim(),
                "0::/"
            );
            // This harmless call normally succeeds; seccomp must reject it.
            assert_eq!(unsafe { libc::syscall(libc::SYS_unshare, 0) }, -1);
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::EPERM)
            );
            assert_eq!(
                unsafe { libc::syscall(libc::SYS_clone3, std::ptr::null::<u8>(), 0) },
                -1
            );
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::ENOSYS)
            );
            std::thread::spawn(|| assert_eq!(unsafe { libc::syscall(libc::SYS_unshare, 0) }, -1))
                .join()
                .unwrap();
        }
        assert!(std::env::var_os("SSH_AUTH_SOCK").is_none());
        assert_eq!(
            std::env::var("OPENAI_API_KEY").unwrap(),
            "sandbox-fixture-key"
        );
        assert_eq!(
            std::env::var("AWS_REGION").unwrap(),
            "sandbox-fixture-region"
        );
        println!("processes, filesystem and privileges isolated");
    }
    if path != "--no-write" {
        if std::fs::write(&path, b"Solmu sandbox write allowed").is_err() {
            eprintln!("write denied");
            std::process::exit(12);
        }
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"Solmu sandbox write allowed"
        );
    }
    if let Ok(address) = std::env::var("SOLMU_TEST_NETWORK") {
        let mut stream = TcpStream::connect(address).expect("network must be allowed");
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.contains("network allowed"));
        println!("network allowed");
    }
    println!("Solmu sandbox probe complete");
    std::process::exit(7);
}

#[cfg(unix)]
fn network_check() {
    use std::net::{TcpListener, UdpSocket};
    use std::os::unix::net::UnixStream;
    for key in ["SOLMU_TEST_TCP4", "SOLMU_TEST_TCP6"] {
        let address = std::env::var(key).unwrap().parse().unwrap();
        assert!(
            TcpStream::connect_timeout(&address, std::time::Duration::from_millis(200)).is_err(),
            "{key} must be denied"
        );
    }
    assert!(
        TcpListener::bind("127.0.0.1:0").is_err(),
        "TCP listening must be denied"
    );
    assert!(
        TcpListener::bind("[::1]:0").is_err(),
        "IPv6 TCP listening must be denied"
    );
    assert!(
        UdpSocket::bind("127.0.0.1:0").is_err(),
        "UDP must be denied"
    );
    assert!(
        UdpSocket::bind("[::1]:0").is_err(),
        "IPv6 UDP must be denied"
    );
    assert!(
        UnixStream::connect(std::env::var("SOLMU_TEST_UNIX").unwrap()).is_err(),
        "Unix socket connections must be denied"
    );
    #[cfg(target_os = "linux")]
    {
        // Netlink is normally available without root, so this checks kernel
        // enforcement independently of ordinary raw-IP privilege checks.
        assert_eq!(
            unsafe { libc::socket(libc::AF_NETLINK, libc::SOCK_RAW, libc::NETLINK_ROUTE) },
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::EPERM)
        );
        assert_eq!(
            unsafe { libc::syscall(libc::SYS_io_uring_setup, 0, std::ptr::null::<u8>()) },
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::EPERM)
        );
        assert_eq!(
            unsafe { libc::syscall(libc::SYS_pidfd_getfd, -1, 0, 0) },
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::EPERM)
        );
    }
    println!("socket networking denied");
}

fn access_check() {
    let cases: serde_json::Value =
        serde_json::from_str(&std::env::var("SOLMU_TEST_ACCESS").unwrap()).unwrap();
    for case in cases.as_array().unwrap() {
        let path = case["path"].as_str().unwrap();
        if let Some(readable) = case["read"].as_bool() {
            assert_eq!(std::fs::read(path).is_ok(), readable, "read policy: {path}");
        }
        if let Some(writable) = case["write"].as_bool() {
            assert_eq!(
                std::fs::write(path, "sandbox change").is_ok(),
                writable,
                "write policy: {path}"
            );
        }
    }
    if let Ok(expectations) = std::env::var("SOLMU_TEST_POLICY_ENV") {
        let expectations: serde_json::Value = serde_json::from_str(&expectations).unwrap();
        for (name, expected) in expectations.as_object().unwrap() {
            assert_eq!(
                std::env::var(name).ok().as_deref(),
                expected.as_str(),
                "environment policy: {name}"
            );
        }
    }
    println!("access policy enforced");
}

#[cfg(target_os = "linux")]
fn linux_limits() {
    let mut children = Vec::new();
    let exhausted = loop {
        match std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--sleep")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(child) => {
                children.push(child);
                if children.len() >= 32 {
                    break false;
                }
            }
            Err(error) => {
                assert_eq!(error.raw_os_error(), Some(libc::EAGAIN));
                break true;
            }
        }
    };
    for mut child in children {
        child.kill().unwrap();
        child.wait().unwrap();
    }
    assert!(
        exhausted,
        "cgroup process limit must stop additional children"
    );
    std::fs::write("limits-ready", "ready").unwrap();
    while !std::path::Path::new("limits-release").exists() {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    println!("process limit enforced");
}

#[allow(clippy::zombie_processes)] // The launcher must terminate this child after the probe exits.
fn spawn_descendant() {
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--delayed-write")
        .arg(std::env::args().nth(2).unwrap())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    println!("spawned descendant {}", child.id());
}
