use std::{
    io::{Read, Write},
    net::TcpStream,
};

fn main() {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments
        .first()
        .is_some_and(|value| value == "--credential-fixture")
    {
        println!("cmd-secret-fixture");
        return;
    }
    #[cfg(target_os = "linux")]
    if arguments
        .first()
        .is_some_and(|value| value == "--abstract-connect")
    {
        use std::os::{linux::net::SocketAddrExt, unix::net::UnixDatagram};
        let name = arguments[1].to_string_lossy();
        let address = std::os::unix::net::SocketAddr::from_abstract_name(name.as_bytes()).unwrap();
        let socket = UnixDatagram::unbound().unwrap();
        let result = socket.connect_addr(&address);
        assert!(
            result.is_err(),
            "abstract Unix sockets outside the Landlock domain must be inaccessible"
        );
        println!("abstract Unix socket isolation verified");
        return;
    }
    #[cfg(target_os = "linux")]
    if arguments
        .first()
        .is_some_and(|value| value == "--signal-check")
    {
        let pid: libc::pid_t = arguments[1].to_string_lossy().parse().unwrap();
        let result = unsafe { libc::kill(pid, libc::SIGTERM) };
        assert_eq!(result, -1, "the host process must not receive the signal");
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::EPERM),
            "Landlock must reject signaling outside its domain"
        );
        println!("signal isolation verified");
        return;
    }
    if arguments
        .first()
        .is_some_and(|value| value == "--learn-fixture")
    {
        let input = std::path::PathBuf::from(&arguments[1]);
        let output = std::path::PathBuf::from(&arguments[2]);
        let _ = std::fs::read(input).unwrap();
        std::fs::write(output, b"learned write").unwrap();
        println!("learn fixture stdout");
        eprintln!("learn fixture stderr");
        return;
    }
    if arguments
        .first()
        .is_some_and(|value| value == "--learn-network-fixture")
    {
        let address = arguments[1].to_string_lossy();
        for _ in 0..2 {
            TcpStream::connect(address.as_ref()).unwrap();
        }
        return;
    }
    if arguments
        .first()
        .is_some_and(|value| value == "--learn-dns-fixture")
    {
        let resolver = arguments[1].to_string_lossy();
        let port: u16 = arguments[2].to_string_lossy().parse().unwrap();
        let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let mut question = vec![
            0x53, 0x4f, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        for label in ["agent", "solmu", "test"] {
            question.push(label.len() as u8);
            question.extend_from_slice(label.as_bytes());
        }
        question.extend_from_slice(&[0, 0, 1, 0, 1]);
        socket.send_to(&question, resolver.as_ref()).unwrap();
        let mut answer = [0u8; 512];
        socket.recv_from(&mut answer).unwrap();
        TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
        return;
    }
    if arguments.first().is_some_and(|value| {
        value == "--write-only-check" || value == "--write-only-directory-check"
    }) {
        let directory_grant = arguments[0] == "--write-only-directory-check";
        let path = std::path::PathBuf::from(&arguments[1]);
        let target = if directory_grant {
            path.join("created.txt")
        } else {
            path.clone()
        };
        std::fs::write(&target, b"write-only").expect("write-only grant permits writes");
        assert!(
            std::fs::read(&target).is_err(),
            "write-only grants must deny reads"
        );
        if directory_grant {
            assert!(std::fs::read(path.join("existing.txt")).is_err());
        }
        println!("write-only access verified");
        return;
    }
    if arguments
        .first()
        .is_some_and(|value| value == "--unlink-check")
    {
        let root = std::path::PathBuf::from(&arguments[1]);
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("keep.txt");
        std::fs::write(&file, b"can still write").unwrap();
        assert!(
            std::fs::remove_file(&file).is_err(),
            "file deletion must be denied"
        );
        let renamed = root.join("renamed.txt");
        assert!(
            std::fs::rename(&file, &renamed).is_err(),
            "renames must be denied"
        );
        let nested = root.join("nested");
        std::fs::create_dir(&nested).unwrap();
        assert!(
            std::fs::remove_dir(&nested).is_err(),
            "directory deletion must be denied"
        );
        println!("file deletion protection verified");
        return;
    }
    if arguments
        .first()
        .is_some_and(|value| value == "--env-check" || value == "--env-check-value")
    {
        let check_value = arguments[0] == "--env-check-value";
        for check in arguments.iter().skip(1) {
            let check = check.to_string_lossy();
            let (name, expectation) = check.split_once('=').expect("NAME=present|absent");
            if check_value {
                assert_eq!(
                    std::env::var(name).as_deref(),
                    Ok(expectation),
                    "unexpected environment variable value: {name}"
                );
            } else {
                let actual = std::env::var_os(name).is_some();
                assert_eq!(
                    actual,
                    expectation == "present",
                    "unexpected environment variable visibility: {name}"
                );
            }
        }
        println!("environment filtering verified");
        return;
    }
    if arguments
        .first()
        .is_some_and(|value| value == "--rollback-set")
    {
        let modified = std::path::PathBuf::from(&arguments[1]);
        let created = std::path::PathBuf::from(&arguments[2]);
        let deleted = std::path::PathBuf::from(&arguments[3]);
        std::fs::write(modified, b"Solmu sandbox write allowed").unwrap();
        std::fs::write(created, b"created during session").unwrap();
        std::fs::remove_file(deleted).unwrap();
        std::process::exit(7);
    }
    let path = arguments
        .first()
        .expect("probe output path")
        .to_string_lossy()
        .into_owned();
    #[cfg(target_os = "linux")]
    if path == "--proxy-check" || path == "--proxy-child-check" {
        proxy_boundaries();
        if path == "--proxy-check" {
            proxy_check();
            std::thread::spawn(proxy_boundaries).join().unwrap();
            assert!(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .arg("--proxy-child-check")
                    .status()
                    .unwrap()
                    .success()
            );
            println!("routed network enforced");
        }
        return;
    }
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

#[cfg(target_os = "linux")]
fn proxy_boundaries() {
    use std::{net::SocketAddr, os::unix::net::UnixStream, time::Duration};
    assert!(UnixStream::connect(std::env::var("SOLMU_TEST_UNIX").unwrap()).is_err());
    for address in [
        std::env::var("SOLMU_TEST_FORBIDDEN").unwrap(),
        "1.1.1.1:443".into(),
    ] {
        assert!(
            TcpStream::connect_timeout(
                &address.parse::<SocketAddr>().unwrap(),
                Duration::from_millis(200)
            )
            .is_err()
        );
    }
    assert_eq!(
        unsafe { libc::socket(libc::AF_NETLINK, libc::SOCK_RAW, libc::NETLINK_ROUTE) },
        -1
    );
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::EPERM)
    );
    let mut pair = [-1; 2];
    assert_eq!(
        unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_DGRAM, 0, pair.as_mut_ptr()) },
        -1
    );
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::EPERM)
    );
    assert_eq!(
        unsafe {
            libc::socketpair(
                libc::AF_UNIX,
                libc::SOCK_STREAM | libc::SOCK_CLOEXEC,
                0,
                pair.as_mut_ptr(),
            )
        },
        0
    );
    // These already-connected streams must not become host socket connections.
    let path = std::env::var("SOLMU_TEST_UNIX").unwrap();
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (target, byte) in address.sun_path.iter_mut().zip(path.bytes()) {
        *target = byte as libc::c_char;
    }
    assert_eq!(
        unsafe {
            libc::connect(
                pair[0],
                (&address as *const libc::sockaddr_un).cast(),
                std::mem::size_of_val(&address) as libc::socklen_t,
            )
        },
        -1
    );
    let mut unspecified: libc::sockaddr = unsafe { std::mem::zeroed() };
    unspecified.sa_family = libc::AF_UNSPEC as libc::sa_family_t;
    assert_eq!(
        unsafe {
            libc::connect(
                pair[0],
                &unspecified,
                std::mem::size_of_val(&unspecified) as libc::socklen_t,
            )
        },
        -1
    );
    unsafe {
        libc::close(pair[0]);
        libc::close(pair[1]);
    }
    // Check before creating any of the probe's own connections. Broker channels
    // and connected host sockets belong only to the worker, never this process.
    let descriptors = std::fs::read_dir("/proc/self/fd")
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    for descriptor in descriptors {
        let Ok(descriptor) = descriptor.to_string_lossy().parse::<i32>() else {
            continue;
        };
        if descriptor <= 2 {
            continue;
        }
        let mut kind = 0i32;
        let mut length = std::mem::size_of_val(&kind) as libc::socklen_t;
        assert_ne!(
            unsafe {
                libc::getsockopt(
                    descriptor,
                    libc::SOL_SOCKET,
                    libc::SO_TYPE,
                    (&mut kind as *mut i32).cast(),
                    &mut length,
                )
            },
            0,
            "inherited broker socket {descriptor}"
        );
    }
}

#[cfg(target_os = "linux")]
fn proxy_check() {
    fn request(mut stream: TcpStream, request: &str) -> String {
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }
    let allowed = std::env::var("SOLMU_TEST_ALLOWED").unwrap();
    let forbidden = std::env::var("SOLMU_TEST_FORBIDDEN").unwrap();
    let proxy = std::env::var("HTTP_PROXY").unwrap();
    let proxy = proxy.strip_prefix("http://").unwrap();
    assert!(
        request(
            TcpStream::connect(&allowed).unwrap(),
            "GET / HTTP/1.1\r\nHost: fixture\r\nConnection: close\r\n\r\n"
        )
        .contains("allowed route")
    );
    assert!(
        request(
            TcpStream::connect(proxy).unwrap(),
            &format!("GET http://{allowed}/ HTTP/1.1\r\nHost: fixture\r\n\r\n")
        )
        .contains("allowed route")
    );
    let mut tunnel = TcpStream::connect(proxy).unwrap();
    tunnel
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    tunnel
        .write_all(format!("CONNECT {allowed} HTTP/1.1\r\nHost: fixture\r\n\r\n").as_bytes())
        .unwrap();
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        tunnel.read_exact(&mut byte).unwrap();
        header.push(byte[0]);
    }
    assert!(header.starts_with(b"HTTP/1.1 200"));
    assert!(
        request(
            tunnel,
            "GET / HTTP/1.1\r\nHost: fixture\r\nConnection: close\r\n\r\n"
        )
        .contains("allowed route")
    );
    for authority in [
        forbidden,
        "example.com:443".into(),
        format!("localhost:{}", allowed.rsplit_once(':').unwrap().1),
    ] {
        assert!(
            request(
                TcpStream::connect(proxy).unwrap(),
                &format!("CONNECT {authority} HTTP/1.1\r\nHost: fixture\r\n\r\n")
            )
            .starts_with("HTTP/1.1 403"),
            "{authority}"
        );
    }
    let output = std::process::Command::new("/usr/bin/curl")
        .args([
            "--disable",
            "--noproxy",
            "",
            "--fail",
            "--silent",
            "--show-error",
            "--max-time",
            "5",
            "--proxy",
            &format!("http://{proxy}"),
            &format!("http://{allowed}/"),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("allowed route"));
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
