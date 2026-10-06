use serde::{Deserialize, Serialize};
use std::{io, net::IpAddr};
use zeroize::Zeroizing;

pub struct UpstreamProxy {
    pub host: String,
    pub port: u16,
    pub authorization: Option<Zeroizing<String>>,
}

pub fn normalize_bypass_pattern(value: &str) -> io::Result<String> {
    let (wildcard, host) = value
        .strip_prefix("*.")
        .map_or((false, value), |host| (true, host));
    if host.contains('*') {
        return Err(io::Error::other(
            "Upstream bypass patterns may only use a leading *. wildcard",
        ));
    }
    let target = Target::parse(&format!("{host}:443"), false)?;
    Ok(if wildcard {
        format!("*.{}", target.host)
    } else {
        target.host
    })
}

pub fn normalize_domain_pattern(value: &str) -> io::Result<String> {
    if value == "*" {
        return Ok(value.to_owned());
    }
    if value.is_empty() || !value.is_ascii() || value.len() > 253 {
        return Err(io::Error::other("Invalid domain pattern"));
    }
    let labels: Vec<_> = value.split('.').collect();
    if labels.iter().any(|label| {
        *label != "*"
            && (label.contains('*')
                || label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'))
    }) {
        return Err(io::Error::other(
            "Domain patterns may use * only as a complete hostname label",
        ));
    }
    let validation_host = labels
        .iter()
        .map(|label| if *label == "*" { "wildcard" } else { label })
        .collect::<Vec<_>>()
        .join(".");
    Target::parse(&format!("{validation_host}:443"), false)?;
    Ok(value.to_ascii_lowercase())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostPattern {
    pub host: String,
    pub port: u16,
}

impl HostPattern {
    pub fn parse(value: &str) -> io::Result<Self> {
        let (host, port) = match value.rsplit_once(':') {
            Some((host, port)) => {
                if port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(io::Error::other(
                        "Remote host patterns require a port from 1 to 65535",
                    ));
                }
                let port = port
                    .parse::<u16>()
                    .ok()
                    .filter(|port| *port != 0)
                    .ok_or_else(|| {
                        io::Error::other("Remote host patterns require a port from 1 to 65535")
                    })?;
                (host, port)
            }
            None => (value, 443),
        };
        if host.starts_with('[') {
            return Err(io::Error::other(
                "Remote host patterns require a DNS hostname, not an IP address",
            ));
        }
        Ok(Self {
            host: normalize_domain_pattern(host)?,
            port,
        })
    }

    pub fn authority(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    #[cfg(any(target_os = "linux", test))]
    pub fn matches(&self, host: &str, port: u16) -> bool {
        self.port == port && matches_domain_pattern(host, &self.host)
    }
}

#[cfg(any(target_os = "linux", test))]
pub fn matches_domain_pattern(host: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        return host.len() > suffix.len()
            && host.ends_with(suffix)
            && host.as_bytes()[host.len() - suffix.len() - 1] == b'.';
    }
    let host_labels: Vec<_> = host.split('.').collect();
    let pattern_labels: Vec<_> = pattern.split('.').collect();
    host_labels.len() == pattern_labels.len()
        && host_labels
            .iter()
            .zip(pattern_labels)
            .all(|(host_label, pattern_label)| pattern_label == "*" || *host_label == pattern_label)
}

#[cfg(any(target_os = "linux", test))]
pub fn is_always_denied_domain(host: &str) -> bool {
    matches!(host, "metadata.google.internal" | "metadata.azure.internal")
}

#[cfg(any(target_os = "linux", test))]
pub fn is_denied_domain(host: &str, patterns: &[String]) -> bool {
    is_always_denied_domain(host)
        || patterns
            .iter()
            .any(|pattern| matches_domain_pattern(host, pattern))
}

#[cfg(any(target_os = "linux", test))]
pub fn matches_bypass(host: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| {
        if let Some(suffix) = pattern.strip_prefix("*.") {
            host.len() > suffix.len()
                && host.ends_with(suffix)
                && host.as_bytes()[host.len() - suffix.len() - 1] == b'.'
        } else {
            host == pattern
        }
    })
}

impl UpstreamProxy {
    pub fn parse(value: &str) -> io::Result<Self> {
        use base64::Engine;

        let url = url::Url::parse(value)
            .map_err(|_| io::Error::other("Upstream proxy must be an HTTP proxy URL"))?;
        if url.scheme() != "http"
            || url.host_str().is_none()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(io::Error::other(
                "Upstream proxy must use http://host[:port] without a path, query, or fragment",
            ));
        }
        let host = url
            .host_str()
            .ok_or_else(|| io::Error::other("Upstream proxy URL has no host"))?
            .trim_matches(['[', ']'])
            .to_owned();
        let port = url
            .port_or_known_default()
            .ok_or_else(|| io::Error::other("Upstream proxy URL has no port"))?;
        let authorization = if url.username().is_empty() && url.password().is_none() {
            None
        } else {
            let username = percent_encoding::percent_decode_str(url.username())
                .decode_utf8()
                .map_err(|_| io::Error::other("Invalid upstream proxy username"))?
                .into_owned();
            let username = Zeroizing::new(username);
            let password: Option<Zeroizing<String>> = url
                .password()
                .map(percent_encoding::percent_decode_str)
                .map(|value| value.decode_utf8())
                .transpose()
                .map_err(|_| io::Error::other("Invalid upstream proxy password"))?
                .map(|password| Zeroizing::new(password.into_owned()));
            if username.contains(':') {
                return Err(io::Error::other(
                    "Upstream proxy usernames cannot contain a colon",
                ));
            }
            let mut credentials = Zeroizing::new(String::with_capacity(
                username.len() + password.as_deref().map_or(0, String::len) + 1,
            ));
            credentials.push_str(&username);
            credentials.push(':');
            if let Some(password) = &password {
                credentials.push_str(password);
            }
            let encoded = Zeroizing::new(
                base64::engine::general_purpose::STANDARD.encode(credentials.as_bytes()),
            );
            let mut authorization = Zeroizing::new("Basic ".to_owned());
            authorization.push_str(&encoded);
            Some(authorization)
        };
        Ok(Self {
            host,
            port,
            authorization,
        })
    }
}

impl Clone for UpstreamProxy {
    fn clone(&self) -> Self {
        Self {
            host: self.host.clone(),
            port: self.port,
            authorization: self.authorization.clone(),
        }
    }
}

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

#[cfg(any(target_os = "linux", test))]
pub fn is_private_network(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            let address = u32::from(address);
            [(0x0a00_0000, 8), (0xac10_0000, 12), (0xc0a8_0000, 16)]
                .iter()
                .any(|(network, prefix)| ipv4_in_subnet(address, *network, *prefix))
        }
        IpAddr::V6(address) => {
            if let Some(address) = address.to_ipv4_mapped() {
                return is_private_network(IpAddr::V4(address));
            }
            address.segments()[0] & 0xfe00 == 0xfc00
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
    use super::{
        HostPattern, UpstreamProxy, is_always_denied_domain, is_denied_domain,
        is_globally_routable, is_private_network, matches_bypass, matches_domain_pattern,
        normalize_bypass_pattern, normalize_domain_pattern,
    };
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

    #[test]
    fn recognizes_only_private_network_ranges_for_explicit_bypasses() {
        for address in ["10.0.0.1", "172.16.0.1", "192.168.1.2", "fc00::1"] {
            assert!(is_private_network(address.parse().unwrap()), "{address}");
        }
        for address in ["127.0.0.1", "169.254.169.254", "172.32.0.1", "fe80::1"] {
            assert!(!is_private_network(address.parse().unwrap()), "{address}");
        }
    }

    #[test]
    fn upstream_proxy_urls_validate_and_encode_basic_authentication() {
        let proxy = UpstreamProxy::parse("http://agent:p%40ss@proxy.internal:3128").unwrap();
        assert_eq!(proxy.host, "proxy.internal");
        assert_eq!(proxy.port, 3128);
        assert_eq!(
            proxy.authorization.as_deref().map(String::as_str),
            Some("Basic YWdlbnQ6cEBzcw==")
        );
        assert!(UpstreamProxy::parse("https://proxy.internal:3128").is_err());
        assert!(UpstreamProxy::parse("http://proxy.internal:3128/path").is_err());
    }

    #[test]
    fn upstream_bypass_patterns_match_exact_domains_and_subdomains() {
        let exact = normalize_bypass_pattern("INTERNAL.example.com").unwrap();
        let wildcard = normalize_bypass_pattern("*.dev.example.com").unwrap();
        let patterns = vec![exact, wildcard];
        assert!(matches_bypass("internal.example.com", &patterns));
        assert!(matches_bypass("api.dev.example.com", &patterns));
        assert!(!matches_bypass("dev.example.com", &patterns));
        assert!(!matches_bypass("evildev.example.com", &patterns));
        assert!(normalize_bypass_pattern("api.*.example.com").is_err());
    }

    #[test]
    fn deny_domain_patterns_validate_and_match_complete_labels() {
        let exact = normalize_domain_pattern("ADS.example.com").unwrap();
        let suffix = normalize_domain_pattern("*.tracking.example").unwrap();
        let label = normalize_domain_pattern("build.*.ci.example.com").unwrap();
        assert!(matches_domain_pattern("ads.example.com", &exact));
        assert!(matches_domain_pattern("a.tracking.example", &suffix));
        assert!(!matches_domain_pattern("tracking.example", &suffix));
        assert!(matches_domain_pattern("build.prod.ci.example.com", &label));
        assert!(!matches_domain_pattern(
            "build.prod.staging.ci.example.com",
            &label
        ));
        assert!(matches_domain_pattern("anything.example", "*"));
        assert!(normalize_domain_pattern("api*bad.example.com").is_err());
        assert!(is_always_denied_domain("metadata.google.internal"));
        assert!(!is_always_denied_domain("api.example.com"));
        assert!(is_denied_domain("metadata.google.internal", &[]));
        assert!(is_denied_domain("ads.example.com", &[exact]));
    }

    #[test]
    fn host_grants_support_validated_wildcards_and_match_ports() {
        let suffix = HostPattern::parse("*.example.com").unwrap();
        let label = HostPattern::parse("build.*.ci.example.com:8443").unwrap();
        assert_eq!(suffix.authority(), "*.example.com:443");
        assert!(suffix.matches("api.example.com", 443));
        assert!(!suffix.matches("example.com", 443));
        assert!(label.matches("build.prod.ci.example.com", 8443));
        assert!(!label.matches("build.prod.ci.example.com", 443));
        assert!(HostPattern::parse("api*bad.example.com").is_err());
        assert!(HostPattern::parse("https://api.example.com").is_err());
    }
}

pub fn profiles() -> &'static [(&'static str, &'static [&'static str])] {
    const LLM_APIS: &[&str] = &[
        "api.openai.com",
        "api.anthropic.com",
        "generativelanguage.googleapis.com",
    ];
    const DEVELOPER: &[&str] = &[
        "api.openai.com",
        "api.anthropic.com",
        "generativelanguage.googleapis.com",
        "registry.npmjs.org",
        "pypi.org",
        "files.pythonhosted.org",
        "index.crates.io",
        "static.crates.io",
        "github.com",
        "api.github.com",
        "raw.githubusercontent.com",
        "codeload.github.com",
        "fulcio.sigstore.dev",
        "rekor.sigstore.dev",
        "tuf-repo-cdn.sigstore.dev",
        "docs.python.org",
        "developer.mozilla.org",
        "doc.rust-lang.org",
    ];
    const ENTERPRISE: &[&str] = &[
        "api.openai.com",
        "api.anthropic.com",
        "generativelanguage.googleapis.com",
        "registry.npmjs.org",
        "pypi.org",
        "files.pythonhosted.org",
        "index.crates.io",
        "static.crates.io",
        "github.com",
        "api.github.com",
        "raw.githubusercontent.com",
        "codeload.github.com",
        "fulcio.sigstore.dev",
        "rekor.sigstore.dev",
        "tuf-repo-cdn.sigstore.dev",
        "docs.python.org",
        "developer.mozilla.org",
        "doc.rust-lang.org",
        "*.googleapis.com",
        "*.openai.azure.com",
        "*.cognitiveservices.azure.com",
        "*.bedrock.amazonaws.com",
        "*.bedrock-runtime.amazonaws.com",
    ];
    &[
        ("minimal", LLM_APIS),
        ("developer", DEVELOPER),
        ("claude-code", DEVELOPER),
        ("codex", DEVELOPER),
        ("opencode", DEVELOPER),
        ("enterprise", ENTERPRISE),
    ]
}

pub fn profile_hosts(name: &str) -> io::Result<Vec<String>> {
    profiles()
        .iter()
        .find(|(profile, _)| *profile == name)
        .map(|(_, hosts)| hosts.iter().map(|host| (*host).to_owned()).collect())
        .ok_or_else(|| {
            io::Error::other(format!(
                "Unknown network profile '{name}'; available profiles: minimal, developer, claude-code, codex, opencode, enterprise"
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
