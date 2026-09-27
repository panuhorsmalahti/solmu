use super::{WORKER, Worker, ipc};
use crate::network::Target;
use std::{
    io::{self, Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    os::{
        fd::FromRawFd,
        unix::{net::UnixStream, process::ExitStatusExt},
    },
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

pub fn worker() -> io::Result<Option<i32>> {
    let mut arguments = std::env::args_os().skip(1);
    if arguments.next().as_deref() != Some(std::ffi::OsStr::new(WORKER)) {
        return Ok(None);
    }
    let source = arguments
        .next()
        .and_then(|argument| argument.into_string().ok())
        .filter(|source| source.len() <= 262_144)
        .ok_or_else(|| io::Error::other("Invalid network worker configuration"))?;
    if arguments.next().is_some() {
        return Err(io::Error::other("Invalid network worker arguments"));
    }
    let worker: Worker = serde_json::from_str(&source).map_err(io::Error::other)?;
    if worker.bridge < 3 || worker.inbound < 3 || worker.bridge == worker.inbound {
        return Err(io::Error::other("Invalid network worker descriptors"));
    }
    for fd in [worker.bridge, worker.inbound] {
        // SAFETY: validate the live descriptor and mark it before agent exec.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    // No host-connected descriptors or broker channels can be taken from the
    // worker through ptrace, pidfd_getfd, or procfs. Seccomp also rejects tracing.
    if unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) } < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: launcher passes two distinct, live, exclusively owned channels.
    let control = Arc::new(Mutex::new(unsafe {
        UnixStream::from_raw_fd(worker.bridge)
    }));
    let mut inbound = unsafe { UnixStream::from_raw_fd(worker.inbound) };
    let active = Arc::new(AtomicUsize::new(0));
    for target in worker.local {
        let listener = TcpListener::bind(target.authority())?;
        let control = control.clone();
        let active = active.clone();
        thread::Builder::new()
            .name("boxer-local-route".into())
            .spawn(move || {
                for client in listener.incoming().flatten() {
                    let target = target.clone();
                    let control = control.clone();
                    dispatch(client, &active, move |client| {
                        relay(client, route(&control, &target)?)
                    });
                }
            })?;
    }
    let listener = (0..64)
        .find_map(|_| match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) if !worker.publish.contains(&listener.local_addr().ok()?.port()) => {
                Some(Ok(listener))
            }
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .ok_or_else(|| {
            io::Error::other("No proxy port outside published service ports was available")
        })??;
    let proxy = format!("http://{}", listener.local_addr()?);
    let outbound_active = active.clone();
    thread::Builder::new()
        .name("boxer-forward-proxy".into())
        .spawn(move || {
            for client in listener.incoming().flatten() {
                let control = control.clone();
                dispatch(client, &outbound_active, move |client| {
                    proxy_connection(client, &control)
                });
            }
        })?;
    thread::Builder::new()
        .name("boxer-inbound-route".into())
        .spawn(move || {
            loop {
                let mut port = [0; 2];
                let Ok(Some(descriptor)) = ipc::receive(&mut inbound, &mut port) else {
                    break;
                };
                let client = TcpStream::from(descriptor);
                let port = u16::from_be_bytes(port);
                dispatch(client, &active, move |client| {
                    let destination = TcpStream::connect_timeout(
                        &format!("127.0.0.1:{port}").parse().unwrap(),
                        Duration::from_secs(2),
                    )?;
                    relay(client, destination)
                });
            }
        })?;
    let mut command = Command::new("/opt/solmu/agent");
    command.args(worker.arguments);
    for name in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ] {
        command.env(name, &proxy);
    }
    // Explicit local routes are guest listeners, so direct HTTP and WebSocket
    // clients can reach them. Other host loopback services have no guest listener.
    for name in ["NO_PROXY", "no_proxy"] {
        command.env(name, "localhost,127.0.0.1,::1");
    }
    let status = command.status()?;
    Ok(Some(
        status
            .code()
            .unwrap_or_else(|| 128 + status.signal().unwrap_or(1)),
    ))
}

struct Active(Arc<AtomicUsize>);
impl Drop for Active {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

fn dispatch(
    client: TcpStream,
    active: &Arc<AtomicUsize>,
    handler: impl FnOnce(TcpStream) -> io::Result<()> + Send + 'static,
) {
    if active.fetch_add(1, Ordering::Relaxed) >= 64 {
        active.fetch_sub(1, Ordering::Relaxed);
        return;
    }
    let active = Active(active.clone());
    let _ = thread::Builder::new()
        .name("boxer-network-connection".into())
        .spawn(move || {
            let _active = active;
            let _ = handler(client);
        });
}

fn route(control: &Mutex<UnixStream>, target: &Target) -> io::Result<TcpStream> {
    let mut channel = control
        .lock()
        .map_err(|_| io::Error::other("Network broker unavailable"))?;
    ipc::write(&mut channel, target)?;
    let mut status = [0];
    let descriptor = ipc::receive(&mut channel, &mut status)?;
    match (status[0], descriptor) {
        (0, Some(descriptor)) => Ok(TcpStream::from(descriptor)),
        _ => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Network route denied or unavailable",
        )),
    }
}

fn relay(mut client: TcpStream, mut destination: TcpStream) -> io::Result<()> {
    client.set_read_timeout(Some(Duration::from_secs(300)))?;
    destination.set_read_timeout(Some(Duration::from_secs(300)))?;
    let mut outgoing = client.try_clone()?;
    let mut upstream = destination.try_clone()?;
    let writer = thread::Builder::new()
        .name("boxer-network-upload".into())
        .spawn(move || {
            let result = io::copy(&mut outgoing, &mut upstream);
            let _ = upstream.shutdown(Shutdown::Write);
            result
        })?;
    let result = io::copy(&mut destination, &mut client);
    let _ = client.shutdown(Shutdown::Both);
    let _ = destination.shutdown(Shutdown::Both);
    let _ = writer.join();
    result.map(|_| ())
}

fn proxy_connection(mut client: TcpStream, control: &Mutex<UnixStream>) -> io::Result<()> {
    client.set_read_timeout(Some(Duration::from_secs(3)))?;
    let mut header = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while !header.ends_with(b"\r\n\r\n") && header.len() < 16_384 {
        let remaining = deadline
            .checked_duration_since(std::time::Instant::now())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "Proxy header timed out"))?;
        client.set_read_timeout(Some(remaining))?;
        let mut byte = [0];
        client.read_exact(&mut byte)?;
        header.push(byte[0]);
    }
    let response = match proxy_request(&header) {
        Ok((target, request)) => match route(control, &target) {
            Ok(mut upstream) => {
                if let Some(request) = request {
                    upstream.write_all(request.as_bytes())?;
                } else {
                    client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")?;
                }
                return relay(client, upstream);
            }
            Err(_) => b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .as_slice(),
        },
        Err(_) => {
            b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".as_slice()
        }
    };
    client.write_all(response)
}

fn proxy_request(header: &[u8]) -> io::Result<(Target, Option<String>)> {
    if !header.ends_with(b"\r\n\r\n") {
        return Err(io::Error::other("Proxy header too large"));
    }
    let text = std::str::from_utf8(header).map_err(io::Error::other)?;
    let mut lines = text.split("\r\n");
    let parts = lines
        .next()
        .unwrap_or_default()
        .split(' ')
        .collect::<Vec<_>>();
    if parts.len() != 3
        || !matches!(parts[2], "HTTP/1.1" | "HTTP/1.0")
        || parts[0].is_empty()
        || !parts[0].bytes().all(|byte| byte.is_ascii_uppercase())
    {
        return Err(io::Error::other("Invalid proxy request"));
    }
    if parts[0] == "CONNECT" {
        let target = authority(parts[1])?;
        return Ok((target, None));
    }
    let url = url::Url::parse(parts[1]).map_err(io::Error::other)?;
    if url.scheme() != "http"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(io::Error::other(
            "Proxy accepts HTTP URLs and HTTPS CONNECT tunnels",
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| io::Error::other("Missing proxy host"))?
        .trim_matches(['[', ']'])
        .to_owned();
    let target = authority(&if host.contains(':') {
        format!("[{host}]:{}", url.port_or_known_default().unwrap())
    } else {
        format!("{host}:{}", url.port_or_known_default().unwrap())
    })?;
    let mut request = format!(
        "{} {}{}{} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n",
        parts[0],
        url.path(),
        if url.query().is_some() { "?" } else { "" },
        url.query().unwrap_or_default(),
        target.authority()
    );
    for line in lines.filter(|line| !line.is_empty()) {
        let (name, _) = line
            .split_once(':')
            .ok_or_else(|| io::Error::other("Invalid proxy header"))?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
            || line
                .bytes()
                .any(|byte| (byte.is_ascii_control() && byte != b'\t') || byte == 127)
        {
            return Err(io::Error::other("Invalid proxy header"));
        }
        if ![
            "host",
            "connection",
            "proxy-connection",
            "proxy-authorization",
        ]
        .iter()
        .any(|skip| name.eq_ignore_ascii_case(skip))
        {
            request.push_str(line);
            request.push_str("\r\n");
        }
    }
    request.push_str("\r\n");
    Ok((target, Some(request)))
}

fn authority(value: &str) -> io::Result<Target> {
    Target::parse(value, false).or_else(|_| Target::parse(value, true))
}
