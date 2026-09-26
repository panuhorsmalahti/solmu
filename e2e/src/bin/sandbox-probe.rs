use std::{
    io::{Read, Write},
    net::TcpStream,
};

fn main() {
    let path = std::env::args().nth(1).expect("probe output path");
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
