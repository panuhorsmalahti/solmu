use serde::{Deserialize, Serialize};
use std::{io, net::IpAddr};

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub host: String,
    pub port: u16,
}

impl Target {
    pub fn parse(value: &str, local: bool) -> io::Result<Self> {
        if value.is_empty()
            || value.len() > 260
            || !value.is_ascii()
            || value
                .bytes()
                .any(|byte| byte.is_ascii_whitespace() || b"/@?#%\\".contains(&byte))
        {
            return Err(io::Error::other(
                "Use an exact host[:port], without URLs, credentials, or wildcards",
            ));
        }
        let (host, port) = if let Some(rest) = value.strip_prefix('[') {
            let (host, port) = rest
                .split_once("]:")
                .ok_or_else(|| io::Error::other("IPv6 routes require [address]:port"))?;
            (host, Some(port))
        } else if let Some((host, port)) = value.rsplit_once(':') {
            (host, Some(port))
        } else {
            (value, None)
        };
        let port = match port {
            Some(port) if !port.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit()) => {
                port.parse::<u16>().ok().filter(|port| *port != 0)
            }
            None if !local => Some(443),
            _ => None,
        }
        .ok_or_else(|| {
            io::Error::other(
                "Routes require a port from 1 to 65535; local routes require an explicit port",
            )
        })?;
        let host = if local {
            let address = host.parse::<IpAddr>().map_err(|_| {
                io::Error::other(
                    "Local routes require a loopback IP address, such as 127.0.0.1:3000",
                )
            })?;
            if !address.is_loopback() {
                return Err(io::Error::other(
                    "Local routes must use a loopback IP address",
                ));
            }
            address.to_string()
        } else {
            if host.parse::<IpAddr>().is_ok()
                || host.len() > 253
                || host.split('.').any(|label| {
                    label.is_empty()
                        || label.len() > 63
                        || label.starts_with('-')
                        || label.ends_with('-')
                        || !label
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                })
                || !host
                    .rsplit('.')
                    .next()
                    .unwrap_or_default()
                    .bytes()
                    .any(|byte| byte.is_ascii_alphabetic())
            {
                return Err(io::Error::other(
                    "Remote routes require an exact DNS hostname; use --allow-local for loopback addresses",
                ));
            }
            host.to_ascii_lowercase()
        };
        Ok(Self { host, port })
    }

    pub fn authority(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}
