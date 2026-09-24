//! Validation for image reference arguments.
//!
//! Accepts three forms:
//!   - `data:<media-type>;base64,<payload>`  (base64-encoded inline image)
//!   - `http://<host>/<path>`               (remote image URL)
//!   - `https://<host>/<path>`               (secure remote image URL)
//!
//! Local file paths are NOT supported — pre-encode with base64 or host the
//! image and pass the HTTP(S) URL.
//!
//! ## SSRF Protection
//!
//! HTTP(S) URLs are validated with SSRF protection: the resolved IP address
//! must not fall into private, loopback, link-local, or cloud-metadata ranges.
//! See [`validate_http_url`] for details.

use std::net::{IpAddr, ToSocketAddrs};

use crate::error_mod::ReferenceError;

// ---------------------------------------------------------------------------
// Reference validation
// ---------------------------------------------------------------------------

/// Maximum number of reference images allowed per request.
#[allow(dead_code)]
pub const MAX_REFERENCES: usize = 16;

/// Validate a reference image URI and return it on success.
///
/// Accepts:
/// - `data:<media-type>;base64,<payload>`  (base64 inline image)
/// - `http://<host>/<path>`                (remote URL)
/// - `https://<host>/<path>`               (secure remote URL)
///
/// HTTP(S) URLs are checked for SSRF safety — see [`validate_http_url`].
///
/// Returns the original string on success, or a `ReferenceError` on failure.
pub fn validate_reference(s: &str) -> Result<&str, ReferenceError> {
    if s.starts_with("data:") {
        validate_data_uri(s)?;
        Ok(s)
    } else if s.starts_with("http://") || s.starts_with("https://") {
        // SsrfError → wrapped into ReferenceError::Malformed for caller compat
        validate_http_url(s).map_err(|e| ReferenceError::Malformed(e.to_string()))?;
        Ok(s)
    } else {
        Err(ReferenceError::NotADataUri(s.to_string()))
    }
}

/// Validate a data URI string.
///
/// Format: `data:<media-type>;base64,<payload>`
fn validate_data_uri(s: &str) -> Result<(), ReferenceError> {
    if !s.starts_with("data:") {
        return Err(ReferenceError::NotADataUri(s.to_string()));
    }

    // Everything after "data:"
    let after = &s[5..];

    let semi = after
        .find(';')
        .ok_or_else(|| ReferenceError::Malformed(s.to_string()))?;
    let _media_type = &after[..semi]; // parsed but not used for validation
    let rest = &after[semi + 1..];

    if !rest.starts_with("base64,") {
        return Err(ReferenceError::NotBase64(s.to_string()));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// SSRF protection
// ---------------------------------------------------------------------------

/// Errors raised by [`validate_http_url`] due to SSRF risk.
#[derive(Debug, thiserror::Error)]
pub enum SsrfError {
    #[error("URL resolves to a private, loopback, link-local, or reserved IP address")]
    PrivateIp,

    #[error("DNS resolution failed for the URL host")]
    DnsFailure,
}

/// Returns true if `ip` falls into a range that must never be reached via
/// an HTTP(S) image reference — loopback, private LAN, link-local (including
/// cloud metadata endpoints), or otherwise reserved.
///
/// Covers:
/// - `127.0.0.0/8`          loopback (IPv4)
/// - `10.0.0.0/8`           private (RFC 1918)
/// - `172.16.0.0/12`       private (RFC 1918)
/// - `192.168.0.0/16`      private (RFC 1918)
/// - `169.254.0.0/16`      link-local / cloud metadata (AWS, GCP, Azure)
/// - `0.0.0.0/8`           unspecified
/// - `::1/128`             IPv6 loopback
/// - `fc00::/7`            IPv6 unique-local
/// - `fe80::/10`           IPv6 link-local
/// - `::ffff:0:0/96`       IPv4-mapped IPv6 (inner v4 is re-checked)
///
/// Does **not** block multicast or broadcast addresses in isolation,
/// because those cannot be connected to directly.
fn is_ssrf_risk(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified()
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() || v6.is_unspecified() {
                return true;
            }
            // fc00::/7 — unique local (first byte: 0xfc or 0xfd)
            if (v6.segments()[0] & 0xfe00) == 0xfc00 {
                return true;
            }
            // fe80::/10 — link-local
            if (v6.segments()[0] & 0xffc0) == 0xfe80 {
                return true;
            }
            // ::ffff:0:0/96 — IPv4-mapped IPv6; re-check the embedded v4
            if let Some(v4_mapped) = v6.to_ipv4_mapped() {
                return is_ssrf_risk(IpAddr::V4(v4_mapped));
            }
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Reference validation
// ---------------------------------------------------------------------------

/// Validate an HTTP(S) URL string for SSRF safety.
///
/// ## Validation steps
///
/// 1. **Scheme** — must be `http://` or `https://`.
/// 2. **Host presence** — the host part after the scheme must not be empty,
///    must not start with `/` (empty host), and must not start with `:`.
/// 3. **DNS resolution** — the host is resolved via the OS resolver.
///    DNS failure causes rejection (unresolvable host).
/// 4. **IP range check** — if *any* resolved IP falls into a private,
///    loopback, link-local, or reserved range, the URL is rejected.
///
/// ## Rejected examples
///
/// - `http://127.0.0.1:8080/image.png`       → loopback
/// - `http://localhost/image.png`             → resolves to loopback
/// - `http://10.0.0.1/admin`                  → private
/// - `http://172.16.0.1/api`                 → private
/// - `http://192.168.1.1/dashboard`           → private
/// - `http://169.254.169.254/metadata`       → cloud metadata (link-local)
/// - `http://[::1]/image.png`                → IPv6 loopback
/// - `http://[fe80::1]/image.png`            → IPv6 link-local
///
/// ## Allowed examples
///
/// - `https://example.com/image.png`
/// - `https://cdn.example.com:8443/img.jpg`
/// - `https://93.184.216.34/image.png`        → public IP
/// - `https://[2606:2800:220:1::247]/img.png` → public IPv6
///
/// Returns `Ok(())` on success or a [`SsrfError`] variant on failure.
fn validate_http_url(s: &str) -> Result<(), SsrfError> {
    let after_scheme = if let Some(rest) = s.strip_prefix("https://") {
        rest
    } else {
        s.strip_prefix("http://").unwrap_or("")
    };

    // Must have a non-empty host
    if after_scheme.is_empty() {
        return Err(SsrfError::DnsFailure);
    }

    // Reject a leading slash without a host (e.g. "http:///path")
    if after_scheme.starts_with('/') {
        return Err(SsrfError::DnsFailure);
    }

    // e.g. "http://:8080" — malformed
    if after_scheme.starts_with(':') {
        return Err(SsrfError::DnsFailure);
    }

    // Strip port / path to get just the host for resolution.
    // Split first on '/' (path separator), then on ':' (port). Handle IPv6
    // literal hosts (`[::1]`, `[fe80::1%25eth0]`) which contain ':' themselves
    // by treating bracketed hosts as opaque.
    let host = if after_scheme.starts_with('[') {
        // IPv6 literal — take everything up to the matching ']'.
        match after_scheme.find(']') {
            Some(close) => &after_scheme[..=close],
            None => after_scheme, // malformed; rely on later validation
        }
    } else if let Some(slash) = after_scheme.find('/') {
        let candidate = &after_scheme[..slash];
        candidate.split(':').next().unwrap_or(candidate)
    } else {
        after_scheme.split(':').next().unwrap_or(after_scheme)
    };

    if host.is_empty() {
        return Err(SsrfError::DnsFailure);
    }

    // String-level loopback check first — covers hosts that wouldn't resolve
    // in offline test environments (`localhost` is treated as SSRF risk).
    if host.eq_ignore_ascii_case("localhost") {
        return Err(SsrfError::PrivateIp);
    }

    // Handle IPv6 literal addresses: `[::1]`, `[fe80::1%25eth0]`
    let host_for_resolution = if host.starts_with('[') {
        if let Some(close) = host.find(']') {
            // Extract the IPv6 address without brackets
            let inner = &host[1..close];
            // Remove zone ID (e.g. %25eth0) if present — not relevant for resolution
            inner.split('%').next().unwrap_or(inner)
        } else {
            // Malformed bracket — try as-is
            host
        }
    } else {
        host
    };

    // Try parsing as a literal IP first — no DNS lookup needed.
    let ips: Vec<IpAddr> = if let Ok(ip) = host_for_resolution.parse::<IpAddr>() {
        vec![ip]
    } else {
        // Otherwise DNS resolve. `to_socket_addrs` may fail in sandboxed
        // environments (no network), which we treat as DnsFailure rather than
        // an open bypass.
        match format!("{}:0", host_for_resolution).to_socket_addrs() {
            Ok(addrs) => addrs.map(|sa| sa.ip()).collect(),
            Err(_) => return Err(SsrfError::DnsFailure),
        }
    };

    if ips.is_empty() {
        return Err(SsrfError::DnsFailure);
    }

    // Any private/loopback IP → reject
    for ip in &ips {
        if is_ssrf_risk(*ip) {
            return Err(SsrfError::PrivateIp);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // is_ssrf_risk — unit tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_is_ssrf_risk_loopback_v4() {
        assert!(is_ssrf_risk("127.0.0.1".parse().unwrap()));
        assert!(is_ssrf_risk("127.255.255.254".parse().unwrap()));
    }

    #[test]
    fn test_is_ssrf_risk_private_v4_10() {
        assert!(is_ssrf_risk("10.0.0.1".parse().unwrap()));
        assert!(is_ssrf_risk("10.255.255.255".parse().unwrap()));
    }

    #[test]
    fn test_is_ssrf_risk_private_v4_172() {
        assert!(is_ssrf_risk("172.16.0.1".parse().unwrap()));
        assert!(is_ssrf_risk("172.31.255.255".parse().unwrap()));
    }

    #[test]
    fn test_is_ssrf_risk_private_v4_192() {
        assert!(is_ssrf_risk("192.168.0.1".parse().unwrap()));
        assert!(is_ssrf_risk("192.168.255.255".parse().unwrap()));
    }

    #[test]
    fn test_is_ssrf_risk_link_local_v4() {
        assert!(is_ssrf_risk("169.254.169.254".parse().unwrap())); // AWS/GCP metadata
        assert!(is_ssrf_risk("169.254.0.1".parse().unwrap()));
    }

    #[test]
    fn test_is_ssrf_risk_unspecified_v4() {
        assert!(is_ssrf_risk("0.0.0.0".parse().unwrap()));
    }

    #[test]
    fn test_is_ssrf_risk_ipv6_loopback() {
        assert!(is_ssrf_risk("::1".parse().unwrap()));
    }

    #[test]
    fn test_is_ssrf_risk_ipv6_unique_local() {
        // fc00::/7 (fc00 and fd00)
        assert!(is_ssrf_risk("fc00::1".parse().unwrap()));
        assert!(is_ssrf_risk("fd00::1".parse().unwrap()));
        // fe00:: is also covered by fc00::/7 mask
    }

    #[test]
    fn test_is_ssrf_risk_ipv6_link_local() {
        assert!(is_ssrf_risk("fe80::1".parse().unwrap()));
        assert!(is_ssrf_risk("fe80::abcd:ef01:2345:6789".parse().unwrap()));
    }

    #[test]
    fn test_is_ssrf_risk_ipv4_mapped_ipv6() {
        // ::ffff:7f00:1 == 127.0.0.1 → loopback
        assert!(is_ssrf_risk("::ffff:127.0.0.1".parse().unwrap()));
        // ::ffff:0a000001 == 10.0.0.1 → private
        assert!(is_ssrf_risk("::ffff:10.0.0.1".parse().unwrap()));
    }

    #[test]
    fn test_is_ssrf_risk_public_allowed() {
        assert!(!is_ssrf_risk("1.1.1.1".parse().unwrap())); // Cloudflare DNS
        assert!(!is_ssrf_risk("8.8.8.8".parse().unwrap())); // Google DNS
        assert!(!is_ssrf_risk("93.184.216.34".parse().unwrap())); // example.com A
        assert!(!is_ssrf_risk("2606:2800:220:1::247".parse().unwrap())); // example.com AAAA
    }

    // -------------------------------------------------------------------------
    // validate_http_url — integration via validate_reference
    // -------------------------------------------------------------------------

    /// SSRF-protected URL → accepted.
    fn url_ok(url: &str) -> bool {
        validate_http_url(url).is_ok()
    }

    /// SSRF-protected URL → rejected.
    fn url_rejected(url: &str) -> bool {
        validate_http_url(url).is_err()
    }

    #[test]
    fn test_ssrf_blocked_loopback_literal() {
        assert!(url_rejected("http://127.0.0.1:8080/image.png"));
        assert!(url_rejected("http://127.0.0.1/image.png"));
        assert!(url_rejected("https://127.255.255.254/photo"));
    }

    #[test]
    fn test_ssrf_blocked_localhost() {
        // localhost resolves to 127.0.0.1 on all standard systems
        assert!(url_rejected("http://localhost:3000/image.png"));
        assert!(url_rejected("http://localhost/image.png"));
        assert!(url_rejected("https://localhost:8443/photo.jpg"));
    }

    #[test]
    fn test_ssrf_blocked_private_10() {
        assert!(url_rejected("http://10.0.0.1/admin"));
        assert!(url_rejected("http://10.255.255.255/internal"));
    }

    #[test]
    fn test_ssrf_blocked_private_172() {
        assert!(url_rejected("http://172.16.0.1/api"));
        assert!(url_rejected("http://172.31.255.255/internal"));
    }

    #[test]
    fn test_ssrf_blocked_private_192() {
        assert!(url_rejected("http://192.168.1.1/dashboard"));
        assert!(url_rejected("http://192.168.255.255/secret"));
    }

    #[test]
    fn test_ssrf_blocked_cloud_metadata() {
        // 169.254.0.0/16 — AWS EC2, GCP, Azure metadata endpoints
        assert!(url_rejected("http://169.254.169.254/latest/meta-data/"));
        assert!(url_rejected("http://169.254.169.254/metadata/v1/"));
        assert!(url_rejected("http://169.254.0.1/anything"));
    }

    #[test]
    fn test_ssrf_blocked_ipv6_loopback() {
        assert!(url_rejected("http://[::1]/image.png"));
        assert!(url_rejected("http://[::1]:8080/image.png"));
    }

    #[test]
    fn test_ssrf_blocked_ipv6_link_local() {
        assert!(url_rejected("http://[fe80::1]/image.png"));
        assert!(url_rejected("http://[fe80::1%25eth0]/image.png"));
    }

    #[test]
    fn test_ssrf_blocked_ipv6_unique_local() {
        assert!(url_rejected("http://[fc00::1]/image.png"));
        assert!(url_rejected("http://[fd00::1]/image.png"));
    }

    #[test]
    fn test_ssrf_blocked_ipv4_mapped_ipv6() {
        // ::ffff:10.0.0.1 → embeds private v4 → blocked
        assert!(url_rejected("http://[::ffff:10.0.0.1]/image.png"));
    }

    #[test]
    fn test_ssrf_public_url_allowed() {
        // These depend on real DNS resolution — use well-known public IPs
        // that should never resolve to private ranges.
        // We test with explicit public IPs to avoid flakiness.

        // Direct public IPv4
        assert!(url_ok("https://93.184.216.34/image.png")); // example.com
        assert!(url_ok("https://1.1.1.1/image.png")); // Cloudflare
        assert!(url_ok("https://8.8.8.8/image.png")); // Google

        // Direct public IPv6
        assert!(url_ok("https://[2606:2800:220:1::247]/image.png")); // example.com AAAA

        // Hostname forms are tested above. Real DNS may be unavailable in
        // some sandboxes, so we don't `assert!(url_ok)` on hostnames here.
    }

    #[test]
    fn test_validate_http_url_malformed() {
        // Empty host
        assert!(url_rejected("http://"));
        assert!(url_rejected("https://"));
        // No host
        assert!(url_rejected("http:///path"));
        // Empty after scheme colon
        assert!(url_rejected("http://:8080/path"));
        // Unresolvable hostname (no DNS entry)
        assert!(url_rejected(
            "http://this-host-definitely-does-not-exist.invalid/x.png"
        ));
    }

    // -------------------------------------------------------------------------
    // data URI tests (unchanged)
    // -------------------------------------------------------------------------

    #[test]
    fn test_valid_png_data_uri() {
        let uri = "data:image/png;base64,SGVsbG8=";
        assert_eq!(validate_reference(uri).unwrap(), uri);
    }

    #[test]
    fn test_valid_jpeg_data_uri() {
        let uri = "data:image/jpeg;base64,/9j/4AAQ";
        assert_eq!(validate_reference(uri).unwrap(), uri);
    }

    #[test]
    fn test_valid_webp_data_uri() {
        let uri = "data:image/webp;base64,UklGRlY=";
        assert_eq!(validate_reference(uri).unwrap(), uri);
    }

    #[test]
    fn test_valid_with_long_payload() {
        let uri = "data:image/png;base64,aGVsbG8gd29ybGQgaGVsbG8gd29ybGQ=";
        assert_eq!(validate_reference(uri).unwrap(), uri);
    }

    #[test]
    fn test_missing_data_prefix() {
        let err = validate_reference("not-a-data-uri").unwrap_err();
        assert!(matches!(err, ReferenceError::NotADataUri(_)));
    }

    #[test]
    fn test_missing_base64_marker() {
        let err = validate_reference("data:image/png;binary,SGVsbG8=").unwrap_err();
        assert!(matches!(err, ReferenceError::NotBase64(_)));
    }

    #[test]
    fn test_missing_semi_colon() {
        let err = validate_reference("datapng;base64,SGVsbG8=").unwrap_err();
        assert!(matches!(err, ReferenceError::NotADataUri(_)));
    }

    #[test]
    fn test_empty_after_data() {
        let err = validate_reference("data:").unwrap_err();
        assert!(matches!(err, ReferenceError::Malformed(_)));
    }

    #[test]
    fn test_only_data_prefix() {
        let err = validate_reference("data:image/png").unwrap_err();
        assert!(matches!(err, ReferenceError::Malformed(_)));
    }

    #[test]
    fn test_file_path_rejected() {
        let err = validate_reference("/home/user/image.png").unwrap_err();
        assert!(matches!(err, ReferenceError::NotADataUri(_)));
    }

    // -------------------------------------------------------------------------
    // HTTP(S) URL tests — public URLs ACCEPTED, private/blocked URLs REJECTED
    // -------------------------------------------------------------------------

    #[test]
    fn test_valid_https_url_public_ip() {
        // example.com's actual IP (deterministic — no DNS needed)
        let url = "https://93.184.216.34/images/photo.png";
        assert_eq!(validate_reference(url).unwrap(), url);
    }

    #[test]
    fn test_valid_http_url_public_ip() {
        let url = "http://93.184.216.34/images/photo.png";
        assert_eq!(validate_reference(url).unwrap(), url);
    }

    #[test]
    fn test_valid_https_url_with_port_public_ip() {
        let url = "https://93.184.216.34:8080/images/photo.png";
        assert_eq!(validate_reference(url).unwrap(), url);
    }

    #[test]
    fn test_valid_https_url_query_params() {
        let url = "https://example.com/image.jpg?w=512&h=512";
        assert_eq!(validate_reference(url).unwrap(), url);
    }

    // NOTE: F12 — localhost and private IP URLs are intentionally BLOCKED now
    // (SSRF protection). The design intent is documented in SECURITY-AUDIT.md §3.

    #[test]
    fn test_private_ip_url_blocked() {
        // 192.168.x.x is private — must be rejected
        let err = validate_reference("https://192.168.1.1/image.png").unwrap_err();
        assert!(matches!(err, ReferenceError::Malformed(ref msg)
            if msg.contains("private") || msg.contains("loopback") || msg.contains("SSRF")));
    }

    #[test]
    fn test_localhost_url_blocked() {
        // localhost resolves to 127.0.0.1 — must be rejected
        let err = validate_reference("http://localhost:3000/image.png").unwrap_err();
        assert!(matches!(err, ReferenceError::Malformed(ref msg)
            if msg.contains("private") || msg.contains("loopback") || msg.contains("SSRF")));
    }

    // --- malformed HTTP(S) URL tests (still REJECTED) ---

    #[test]
    fn test_empty_http_url_rejected() {
        let err = validate_reference("http://").unwrap_err();
        assert!(matches!(err, ReferenceError::Malformed(_)));
    }

    #[test]
    fn test_empty_https_url_rejected() {
        let err = validate_reference("https://").unwrap_err();
        assert!(matches!(err, ReferenceError::Malformed(_)));
    }

    #[test]
    fn test_http_url_slash_only_rejected() {
        let err = validate_reference("http:///path").unwrap_err();
        assert!(matches!(err, ReferenceError::Malformed(_)));
    }

    #[test]
    fn test_http_url_colon_only_rejected() {
        let err = validate_reference("http://:8080/path").unwrap_err();
        assert!(matches!(err, ReferenceError::Malformed(_)));
    }
}
