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
        if addresses.is_empty()
            || (!local
                && addresses
                    .iter()
                    .any(|address| !crate::network::is_globally_routable(address.ip())))
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
