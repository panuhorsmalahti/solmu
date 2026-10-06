use serde::{Deserialize, Serialize};
use std::{io, net::IpAddr};

/// Returns whether a remote route resolves to an address that may be globally
/// reachable. This intentionally denies special-use ranges even where IANA
/// marks a more-specific assignment as globally reachable.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn is_globally_routable(address: IpAddr) -> bool {
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
                return is_globally_routable(IpAddr::V4(address));
            }
            let segments = address.segments();
            let address = u128::from(address);
            (segments[0] & 0xe000 == 0x2000)
                && ![
                    (0x2001_0000_0000_0000_0000_0000_0000_0000, 23), // IETF assignments
                    (0x2001_0000_0000_0000_0000_0000_0000_0000, 32), // Teredo
                    (0x2001_0002_0000_0000_0000_0000_0000_0000, 48), // Benchmarking
                    (0x2001_0db8_0000_0000_0000_0000_0000_0000, 32), // Documentation
                    (0x2001_0010_0000_0000_0000_0000_0000_0000, 28), // Deprecated ORCHID
                    (0x2001_0020_0000_0000_0000_0000_0000_0000, 28), // ORCHIDv2
                    (0x2001_0030_0000_0000_0000_0000_0000_0000, 28), // Drone Remote ID
                    (0x2002_0000_0000_0000_0000_0000_0000_0000, 16), // 6to4
                    (0x3fff_0000_0000_0000_0000_0000_0000_0000, 20), // Documentation
                    (0x5f00_0000_0000_0000_0000_0000_0000_0000, 16), // Segment routing
                ]
                .iter()
                .any(|(network, prefix)| ipv6_in_subnet(address, *network, *prefix))
        }
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn ipv4_in_subnet(address: u32, network: u32, prefix: u32) -> bool {
    address >> (32 - prefix) == network >> (32 - prefix)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn ipv6_in_subnet(address: u128, network: u128, prefix: u32) -> bool {
    address >> (128 - prefix) == network >> (128 - prefix)
}

#[cfg(test)]
mod address_tests {
    use super::is_globally_routable;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    #[test]
    fn rejects_non_global_ipv4_and_ipv6_destinations() {
        for address in [
            "192.88.99.1", // deprecated 6to4 relay anycast
            "192.0.2.10",  // documentation
            "198.18.0.1",  // benchmarking
            "255.255.255.255",
            "2001:2::1",  // benchmarking
            "2001:10::1", // deprecated ORCHID
            "2001:20::1", // ORCHIDv2
            "2001:30::1", // Drone Remote ID
            "2002::1",    // 6to4
            "3fff::1",    // documentation
            "5f00::1",    // segment routing
        ] {
            assert!(
                !is_globally_routable(address.parse().unwrap()),
                "{address} must be denied"
            );
        }
    }

    #[test]
    fn accepts_public_routes_and_rejects_ipv4_mapped_special_addresses() {
        assert!(is_globally_routable(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));
        assert!(is_globally_routable(
            "2606:4700:4700::1111".parse::<IpAddr>().unwrap()
        ));
        assert!(!is_globally_routable(IpAddr::V6(Ipv6Addr::LOCALHOST)));
        assert!(!is_globally_routable(
            "::ffff:192.168.1.10".parse::<IpAddr>().unwrap()
        ));
    }
}

pub fn profiles() -> &'static [(&'static str, &'static [&'static str])] {
    &[
        (
            "minimal",
            &[
                "api.openai.com",
                "api.anthropic.com",
                "generativelanguage.googleapis.com",
            ],
        ),
        (
            "developer",
            &[
                "api.openai.com",
                "api.anthropic.com",
                "generativelanguage.googleapis.com",
                "github.com",
                "api.github.com",
                "raw.githubusercontent.com",
                "codeload.github.com",
                "registry.npmjs.org",
                "pypi.org",
                "files.pythonhosted.org",
                "index.crates.io",
                "static.crates.io",
            ],
        ),
    ]
}

pub fn profile_hosts(name: &str) -> io::Result<Vec<String>> {
    profiles()
        .iter()
        .find(|(profile, _)| *profile == name)
        .map(|(_, hosts)| hosts.iter().map(|host| (*host).to_owned()).collect())
        .ok_or_else(|| {
            io::Error::other(format!(
                "Unknown network profile '{name}'; available profiles: minimal, developer"
            ))
        })
}

pub fn command(args: &[std::ffi::OsString]) -> io::Result<i32> {
    if args.len() == 2 && args[1] == "profiles" {
        let profiles: Vec<_> = profiles()
            .iter()
            .map(|(name, hosts)| serde_json::json!({"name":name,"hosts":hosts}))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&profiles).map_err(io::Error::other)?
        );
        Ok(0)
    } else {
        Err(io::Error::other("Usage: boxer network profiles"))
    }
}

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
