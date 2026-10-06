use super::ipc;
use crate::{Policy, network::Target};
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
                                .block_on(connect(&target, target.host.parse::<IpAddr>().is_ok()))
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

async fn connect(target: &Target, local: bool) -> io::Result<TcpStream> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let addresses = tokio::net::lookup_host((target.host.as_str(), target.port))
            .await?
            .take(32)
            .collect::<Vec<_>>();
        if addresses.is_empty() || (!local && addresses.iter().any(|address| !public(address.ip())))
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

fn public(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            let address = u32::from(address);
            ![
                (0x0000_0000, 8),  // This network
                (0x0a00_0000, 8),  // Private use
                (0x6440_0000, 10), // Shared address space
                (0x7f00_0000, 8),  // Loopback
                (0xa9fe_0000, 16), // Link local
                (0xac10_0000, 12), // Private use
                (0xc000_0000, 24), // IETF protocol assignments
                (0xc000_0200, 24), // Documentation
                (0xc058_6300, 24), // Deprecated 6to4 relay anycast
                (0xc0a8_0000, 16), // Private use
                (0xc612_0000, 15), // Benchmarking
                (0xc633_6400, 24), // Documentation
                (0xcb00_7100, 24), // Documentation
                (0xe000_0000, 4),  // Multicast
                (0xf000_0000, 4),  // Reserved
            ]
            .iter()
            .any(|(network, prefix)| ipv4_in_subnet(address, *network, *prefix))
        }
        IpAddr::V6(address) => {
            if let Some(address) = address.to_ipv4_mapped() {
                return public(IpAddr::V4(address));
            }
            let segments = address.segments();
            let address = u128::from(address);
            (segments[0] & 0xe000 == 0x2000)
                && ![
                    (0x2001_0000_0000_0000_0000_0000_0000_0000, 23), // IETF assignments
                    (0x2001_0000_0000_0000_0000_0000_0000_0000, 32), // Teredo
                    (0x2001_0002_0000_0000_0000_0000_0000_0000, 48), // Benchmarking
                    (0x2001_0db8_0000_0000_0000_0000_0000_0000, 32), // Documentation
                    (0x2002_0000_0000_0000_0000_0000_0000_0000, 16), // 6to4
                    (0x3fff_0000_0000_0000_0000_0000_0000_0000, 20), // Documentation
                    (0x5f00_0000_0000_0000_0000_0000_0000_0000, 16), // Segment routing
                ]
                .iter()
                .any(|(network, prefix)| ipv6_in_subnet(address, *network, *prefix))
        }
    }
}

fn ipv4_in_subnet(address: u32, network: u32, prefix: u32) -> bool {
    address >> (32 - prefix) == network >> (32 - prefix)
}

fn ipv6_in_subnet(address: u128, network: u128, prefix: u32) -> bool {
    address >> (128 - prefix) == network >> (128 - prefix)
}

#[cfg(test)]
mod tests {
    use super::public;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    #[test]
    fn proxy_routes_reject_non_global_ipv4_and_ipv6_destinations() {
        for address in [
            "192.88.99.1", // deprecated 6to4 relay anycast
            "192.0.2.10",  // documentation
            "198.18.0.1",  // benchmarking
            "255.255.255.255",
            "2001:2::1", // benchmarking
            "2002::1",   // 6to4
            "3fff::1",   // documentation
            "5f00::1",   // segment routing
        ] {
            assert!(
                !public(address.parse().unwrap()),
                "{address} must be denied"
            );
        }
    }

    #[test]
    fn proxy_routes_accept_globally_routable_destinations() {
        assert!(public(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));
        assert!(public("2606:4700:4700::1111".parse::<IpAddr>().unwrap()));
        assert!(!public(IpAddr::V6(Ipv6Addr::LOCALHOST)));
    }
}
