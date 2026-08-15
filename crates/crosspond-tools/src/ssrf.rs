//! Reject URLs that would reach the local network or metadata endpoints.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};

use crate::tool::ToolError;

const MAX_REDIRECTS: usize = 5;

pub fn max_redirects() -> usize {
    MAX_REDIRECTS
}

/// Parse and validate that `raw` is an http(s) URL that does not target private hosts.
pub fn validate_fetch_url(raw: &str) -> Result<reqwest::Url, ToolError> {
    let url = reqwest::Url::parse(raw).map_err(|_| ToolError::Failed("invalid URL".into()))?;
    validate_url(&url)?;
    Ok(url)
}

pub fn validate_url(url: &reqwest::Url) -> Result<(), ToolError> {
    match url.scheme() {
        "http" | "https" => {}
        _ => {
            return Err(ToolError::Failed(
                "only http and https URLs are allowed".into(),
            ));
        }
    }
    let Some(host) = url.host_str() else {
        return Err(ToolError::Failed("URL is missing a host".into()));
    };
    if let Ok(ip) = host.parse::<IpAddr>() {
        check_ip(ip)?;
    } else {
        if is_blocked_hostname(host) {
            return Err(ToolError::Failed("URL targets a blocked host".into()));
        }
        resolve_and_check(host)?;
    }
    Ok(())
}

fn is_blocked_hostname(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "localhost"
        || lower.ends_with(".localhost")
        || lower.ends_with(".local")
        || lower == "metadata.google.internal"
}

fn resolve_and_check(hostname: &str) -> Result<(), ToolError> {
    let addrs = (hostname, 0)
        .to_socket_addrs()
        .map_err(|_| ToolError::Failed("could not resolve host".into()))?;
    let mut any = false;
    for addr in addrs {
        any = true;
        check_ip(addr.ip())?;
    }
    if !any {
        return Err(ToolError::Failed("could not resolve host".into()));
    }
    Ok(())
}

fn check_ip(ip: IpAddr) -> Result<(), ToolError> {
    if is_blocked_ip(ip) {
        Err(ToolError::Failed(
            "URL targets a private or local address".into(),
        ))
    } else {
        Ok(())
    }
}

pub fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_blocked_v4(v4),
        IpAddr::V6(v6) => is_blocked_v6(v6),
    }
}

fn is_blocked_v4(ip: Ipv4Addr) -> bool {
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_unspecified()
        || ip.octets()[0] == 0
        // Carrier-grade NAT
        || (ip.octets()[0] == 100 && (ip.octets()[1] & 0b1100_0000) == 0b0100_0000)
        // IETF Protocol Assignments
        || (ip.octets()[0] == 192 && ip.octets()[1] == 0 && ip.octets()[2] == 0)
        // TEST-NET
        || (ip.octets()[0] == 192 && ip.octets()[1] == 0 && ip.octets()[2] == 2)
        || (ip.octets()[0] == 198 && ip.octets()[1] == 51 && ip.octets()[2] == 100)
        || (ip.octets()[0] == 203 && ip.octets()[1] == 0 && ip.octets()[2] == 113)
        // Benchmarking
        || (ip.octets()[0] == 198 && (ip.octets()[1] & 0xfe) == 18)
        // Multicast / reserved
        || ip.octets()[0] >= 224
        // AWS / cloud metadata
        || ip == Ipv4Addr::new(169, 254, 169, 254)
}

fn is_blocked_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_blocked_v4(v4);
    }
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_unique_local()
        || ip.is_unicast_link_local()
        || (ip.segments()[0] & 0xff00) == 0xff00 // multicast
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_file_scheme() {
        let err = validate_fetch_url("file:///etc/passwd").unwrap_err();
        assert!(err.to_string().contains("http"));
    }

    #[test]
    fn rejects_localhost() {
        assert!(validate_fetch_url("http://127.0.0.1/").is_err());
        assert!(validate_fetch_url("http://localhost/").is_err());
        assert!(validate_fetch_url("http://[::1]/").is_err());
    }

    #[test]
    fn rejects_private_literals() {
        assert!(validate_fetch_url("http://10.0.0.1/").is_err());
        assert!(validate_fetch_url("http://192.168.1.1/").is_err());
        assert!(validate_fetch_url("http://172.16.0.1/").is_err());
        assert!(validate_fetch_url("http://169.254.169.254/").is_err());
    }

    #[test]
    fn accepts_public_https() {
        match validate_fetch_url("https://example.com/") {
            Ok(url) => assert_eq!(url.host_str(), Some("example.com")),
            Err(err) => assert!(
                err.to_string().contains("resolve"),
                "unexpected error: {err}"
            ),
        }
    }
}
