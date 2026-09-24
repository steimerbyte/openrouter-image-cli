//! Validation for image reference arguments.
//!
//! Accepts three forms:
//!   - `data:<media-type>;base64,<payload>`  (base64-encoded inline image)
//!   - `http://<host>/<path>`               (remote image URL)
//!   - `https://<host>/<path>`               (secure remote image URL)
//!
//! Local file paths are NOT supported — pre-encode with base64 or host the
//! image and pass the HTTP(S) URL.

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
/// Returns the original string on success, or a `ReferenceError` on failure.
pub fn validate_reference(s: &str) -> Result<&str, ReferenceError> {
    if s.starts_with("data:") {
        validate_data_uri(s)?;
        Ok(s)
    } else if s.starts_with("http://") || s.starts_with("https://") {
        validate_http_url(s)?;
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

/// Validate an HTTP(S) URL string.
///
/// Requires: scheme + at least one domain label. Empty host ("http://") is rejected.
fn validate_http_url(s: &str) -> Result<(), ReferenceError> {
    let after_scheme = if let Some(rest) = s.strip_prefix("https://") {
        rest
    } else {
        s.strip_prefix("http://").unwrap_or("")
    };

    // Must have a non-empty host
    if after_scheme.is_empty() {
        return Err(ReferenceError::Malformed(s.to_string()));
    }

    // Reject a leading slash without a host (e.g. "http:///path")
    // after_scheme[0] == '/' means empty host
    if after_scheme.starts_with('/') {
        return Err(ReferenceError::Malformed(s.to_string()));
    }

    // Basic domain label check: must contain at least one dot in the host part
    // (or be a known single-label host, which is also acceptable)
    // We just ensure the host is not empty and doesn't start with '/'
    // The actual URL structure is validated by the HTTP client at request time.
    if after_scheme.starts_with(':') {
        // e.g. "http://:8080" — malformed
        return Err(ReferenceError::Malformed(s.to_string()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- data URI tests ---

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

    // --- HTTP(S) URL tests (now ACCEPTED) ---

    #[test]
    fn test_valid_https_url() {
        let url = "https://example.com/images/photo.png";
        assert_eq!(validate_reference(url).unwrap(), url);
    }

    #[test]
    fn test_valid_http_url() {
        let url = "http://example.com/images/photo.png";
        assert_eq!(validate_reference(url).unwrap(), url);
    }

    #[test]
    fn test_valid_https_url_with_port() {
        let url = "https://example.com:8080/images/photo.png";
        assert_eq!(validate_reference(url).unwrap(), url);
    }

    #[test]
    fn test_valid_https_url_query_params() {
        let url = "https://example.com/image.jpg?w=512&h=512";
        assert_eq!(validate_reference(url).unwrap(), url);
    }

    #[test]
    fn test_valid_ip_address_url() {
        let url = "https://192.168.1.1/image.png";
        assert_eq!(validate_reference(url).unwrap(), url);
    }

    #[test]
    fn test_valid_localhost_url() {
        let url = "http://localhost:3000/image.png";
        assert_eq!(validate_reference(url).unwrap(), url);
    }

    // --- malformed HTTP(S) URL tests (still REJECTED) ---

    #[test]
    fn test_empty_http_url_rejected() {
        // "http://" with no host
        let err = validate_reference("http://").unwrap_err();
        assert!(matches!(err, ReferenceError::Malformed(_)));
    }

    #[test]
    fn test_empty_https_url_rejected() {
        // "https://" with no host
        let err = validate_reference("https://").unwrap_err();
        assert!(matches!(err, ReferenceError::Malformed(_)));
    }

    #[test]
    fn test_http_url_slash_only_rejected() {
        // "http:///" with empty host
        let err = validate_reference("http:///path").unwrap_err();
        assert!(matches!(err, ReferenceError::Malformed(_)));
    }

    #[test]
    fn test_http_url_colon_only_rejected() {
        // "http://:" with empty host
        let err = validate_reference("http://:8080/path").unwrap_err();
        assert!(matches!(err, ReferenceError::Malformed(_)));
    }
}
