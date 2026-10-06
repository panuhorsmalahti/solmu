use super::ipc;
use crate::{
    Policy,
    network::{Target, UpstreamProxy},
};
use std::{
    io,
    net::{IpAddr, Shutdown, TcpListener, TcpStream},
    os::{fd::AsRawFd, unix::net::UnixStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use zeroize::Zeroizing;

pub struct Host {
    outbound: UnixStream,
    inbound: UnixStream,
    pub children: Vec<UnixStream>,
    alive: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl Host {
    pub fn new(policy: &Policy) -> io::Result<Self> {
        let allowed = policy
            .hosts
            .iter()
            .map(|host| Target::parse(host, false))
            .chain(policy.local.iter().map(|host| Target::parse(host, true)))
            .collect::<io::Result<Vec<_>>>()?;
        let upstream_proxy = policy
            .upstream_proxy
            .as_deref()
            .map(|value| UpstreamProxy::parse(value))
            .transpose()?;
        let upstream_bypass = policy.upstream_bypass.clone();
        let listeners = policy
            .publish
            .iter()
            .map(|port| {
                let listener = TcpListener::bind(("127.0.0.1", *port))?;
                listener.set_nonblocking(true)?;
                Ok((*port, listener))
            })
            .collect::<io::Result<Vec<_>>>()?;
        let (outbound, child_outbound) = UnixStream::pair()?;
        let (inbound, child_inbound) = UnixStream::pair()?;
        let mut connection = outbound.try_clone()?;
        let publications = inbound.try_clone()?;
        publications.set_write_timeout(Some(Duration::from_secs(3)))?;
        let alive = Arc::new(AtomicBool::new(true));
        let mut host = Self {
            outbound,
            inbound,
            children: vec![child_outbound, child_inbound],
            alive,
            threads: Vec::new(),
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        host.threads.push(
            thread::Builder::new()
                .name("boxer-routes".into())
                .spawn(move || {
                    while let Ok(target) = ipc::read::<Target>(&mut connection) {
                        let socket = if allowed.contains(&target) {
                            runtime
                                .block_on(connect_route(
                                    &target,
                                    target.host.parse::<IpAddr>().is_ok(),
                                    upstream_proxy.as_ref(),
                                    &upstream_bypass,
                                ))
                                .ok()
                        } else {
                            None
                        };
                        let result = ipc::send(
                            &connection,
                            &[if socket.is_some() { 0 } else { 1 }],
                            socket.as_ref().map(AsRawFd::as_raw_fd),
                        );
                        if result.is_err() {
                            break;
                        }
                    }
                    // A DNS timeout must not keep the launcher waiting for resolver threads.
                    runtime.shutdown_background();
                })?,
        );
        let running = host.alive.clone();
        host.threads.push(
            thread::Builder::new()
                .name("boxer-published-ports".into())
                .spawn(move || {
                    while running.load(Ordering::Relaxed) {
                        for (port, listener) in &listeners {
                            if let Ok((socket, _)) = listener.accept()
                                && ipc::send(
                                    &publications,
                                    &port.to_be_bytes(),
                                    Some(socket.as_raw_fd()),
                                )
                                .is_err()
                            {
                                return;
                            }
                        }
                        thread::sleep(Duration::from_millis(10));
                    }
                })?,
        );
        Ok(host)
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
        self.children.clear();
        let _ = self.outbound.shutdown(Shutdown::Both);
        let _ = self.inbound.shutdown(Shutdown::Both);
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

pub(super) async fn connect(target: &Target, local: bool) -> io::Result<TcpStream> {
    connect_direct(target, local, false).await
}

async fn connect_direct(
    target: &Target,
    local: bool,
    allow_private: bool,
) -> io::Result<TcpStream> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let addresses = tokio::net::lookup_host((target.host.as_str(), target.port))
            .await?
            .take(32)
            .collect::<Vec<_>>();
        if addresses.is_empty()
            || (!local
                && addresses.iter().any(|address| {
                    !crate::network::is_globally_routable(address.ip())
                        && !(allow_private && crate::network::is_private_network(address.ip()))
                }))
        {
            return Err(io::Error::other(
                "Remote routes cannot resolve to private or special-use addresses",
            ));
        }
        let mut error = io::Error::other("No permitted route was reachable");
        for address in addresses {
            match tokio::time::timeout(
                Duration::from_millis(750),
                tokio::net::TcpStream::connect(address),
            )
            .await
            .unwrap_or_else(|_| Err(io::Error::new(io::ErrorKind::TimedOut, "Address timed out")))
            {
                Ok(stream) => {
                    let stream = stream.into_std()?;
                    stream.set_nonblocking(false)?;
                    return Ok(stream);
                }
                Err(failure) => error = failure,
            }
        }
        Err(error)
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Network route timed out"))?
}

pub(super) async fn connect_route(
    target: &Target,
    local: bool,
    upstream_proxy: Option<&UpstreamProxy>,
    upstream_bypass: &[String],
) -> io::Result<TcpStream> {
    if local {
        connect(target, local).await
    } else if let Some(upstream_proxy) = upstream_proxy {
        if crate::network::matches_bypass(&target.host, upstream_bypass) {
            connect_direct(target, false, true).await
        } else {
            connect_via_upstream(target, upstream_proxy).await
        }
    } else {
        connect(target, false).await
    }
}

async fn connect_via_upstream(target: &Target, proxy: &UpstreamProxy) -> io::Result<TcpStream> {
    tokio::time::timeout(Duration::from_secs(10), async {
        let destinations = tokio::net::lookup_host((target.host.as_str(), target.port))
            .await?
            .take(32)
            .collect::<Vec<_>>();
        if destinations.is_empty()
            || destinations
                .iter()
                .any(|address| !crate::network::is_globally_routable(address.ip()))
        {
            return Err(io::Error::other(
                "Remote routes cannot resolve to private or special-use addresses",
            ));
        }
        let proxy_addresses = tokio::net::lookup_host((proxy.host.as_str(), proxy.port))
            .await?
            .take(32)
            .collect::<Vec<_>>();
        if proxy_addresses.is_empty() {
            return Err(io::Error::other("Upstream proxy could not be resolved"));
        }
        let mut error = io::Error::other("Upstream proxy could not open the requested route");
        for destination in destinations {
            for proxy_address in &proxy_addresses {
                match tokio::time::timeout(
                    Duration::from_millis(750),
                    tokio::net::TcpStream::connect(proxy_address),
                )
                .await
                .unwrap_or_else(|_| {
                    Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "Upstream proxy connection timed out",
                    ))
                }) {
                    Ok(mut stream) => match open_tunnel(&mut stream, destination, proxy).await {
                        Ok(()) => {
                            let stream = stream.into_std()?;
                            stream.set_nonblocking(false)?;
                            return Ok(stream);
                        }
                        Err(failure) => error = failure,
                    },
                    Err(failure) => error = failure,
                }
            }
        }
        Err(error)
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Upstream proxy route timed out"))?
}

async fn open_tunnel(
    stream: &mut tokio::net::TcpStream,
    destination: std::net::SocketAddr,
    proxy: &UpstreamProxy,
) -> io::Result<()> {
    let authority = match destination {
        std::net::SocketAddr::V4(address) => format!("{}:{}", address.ip(), address.port()),
        std::net::SocketAddr::V6(address) => format!("[{}]:{}", address.ip(), address.port()),
    };
    let mut request = Zeroizing::new(format!(
        "CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\nProxy-Connection: Keep-Alive\r\n"
    ));
    if let Some(authorization) = &proxy.authorization {
        request.push_str("Proxy-Authorization: ");
        request.push_str(authorization);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).await?;
    let mut response = Vec::new();
    while !response.ends_with(b"\r\n\r\n") {
        if response.len() >= 16_384 {
            return Err(io::Error::other(
                "Upstream proxy response header is too large",
            ));
        }
        let mut byte = [0];
        stream.read_exact(&mut byte).await?;
        response.push(byte[0]);
    }
    let text = std::str::from_utf8(&response)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let status = text
        .split_once("\r\n")
        .map(|(line, _)| line)
        .unwrap_or_default()
        .split_ascii_whitespace()
        .nth(1)
        .and_then(|status| status.parse::<u16>().ok());
    if status != Some(200) {
        return Err(io::Error::other(format!(
            "Upstream proxy rejected CONNECT with status {}",
            status.map_or("invalid".to_owned(), |status| status.to_string())
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn upstream_proxy_opens_an_authenticated_http_connect_tunnel() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let proxy = UpstreamProxy::parse(&format!("http://agent:secret@{address}")).unwrap();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut header = Vec::new();
                while !header.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    socket.read_exact(&mut byte).await.unwrap();
                    header.push(byte[0]);
                }
                let header = String::from_utf8(header).unwrap();
                assert!(header.starts_with("CONNECT 93.184.216.34:443 HTTP/1.1\r\n"));
                assert!(header.contains("Proxy-Authorization: Basic YWdlbnQ6c2VjcmV0\r\n"));
                socket
                    .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                    .await
                    .unwrap();
                let mut request = [0; 5];
                socket.read_exact(&mut request).await.unwrap();
                assert_eq!(&request, b"hello");
                socket.write_all(b"world").await.unwrap();
            });
            let mut tunnel = tokio::net::TcpStream::connect(address).await.unwrap();
            open_tunnel(&mut tunnel, "93.184.216.34:443".parse().unwrap(), &proxy)
                .await
                .unwrap();
            tunnel.write_all(b"hello").await.unwrap();
            let mut response = [0; 5];
            tunnel.read_exact(&mut response).await.unwrap();
            assert_eq!(&response, b"world");
            server.await.unwrap();
        });
    }
}
