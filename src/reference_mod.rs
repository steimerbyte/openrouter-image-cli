//! Strict data-URI validation for image reference arguments.
//!
//! Only `data:<media-type>;base64,<payload>` format is accepted.
//! Local file paths and HTTP(S) URLs are NOT supported — use
//! `--image-ref` only with pre-encoded base64 data URIs.

use crate::error_mod::ReferenceError;

// ---------------------------------------------------------------------------
// Data URI validation
// ---------------------------------------------------------------------------

/// Validate that a string is a well-formed data URI and return it on success.
///
/// Format: `data:<media-type>;base64,<payload>`
///
/// Returns the original string on success, or a `ReferenceError` on failure.
pub fn validate_data_uri(s: &str) -> Result<&str, ReferenceError> {
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

    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_png() {
        let uri = "data:image/png;base64,SGVsbG8=";
        assert_eq!(validate_data_uri(uri).unwrap(), uri);
    }

    #[test]
    fn test_valid_jpeg() {
        let uri = "data:image/jpeg;base64,/9j/4AAQ";
        assert_eq!(validate_data_uri(uri).unwrap(), uri);
    }

    #[test]
    fn test_valid_webp() {
        let uri = "data:image/webp;base64,UklGRlY=";
        assert_eq!(validate_data_uri(uri).unwrap(), uri);
    }

    #[test]
    fn test_valid_with_long_payload() {
        let uri = "data:image/png;base64,aGVsbG8gd29ybGQgaGVsbG8gd29ybGQ=";
        assert_eq!(validate_data_uri(uri).unwrap(), uri);
    }

    #[test]
    fn test_missing_data_prefix() {
        let err = validate_data_uri("https://example.com/image.png").unwrap_err();
        assert!(matches!(err, ReferenceError::NotADataUri(_)));
    }

    #[test]
    fn test_missing_semi_colon() {
        let err = validate_data_uri("datapng;base64,SGVsbG8=").unwrap_err();
        assert!(matches!(err, ReferenceError::NotADataUri(_)));
    }

    #[test]
    fn test_missing_base64_marker() {
        let err = validate_data_uri("data:image/png;binary,SGVsbG8=").unwrap_err();
        assert!(matches!(err, ReferenceError::NotBase64(_)));
    }

    #[test]
    fn test_empty_after_data() {
        let err = validate_data_uri("data:").unwrap_err();
        assert!(matches!(err, ReferenceError::Malformed(_)));
    }

    #[test]
    fn test_only_data_prefix() {
        let err = validate_data_uri("data:image/png").unwrap_err();
        assert!(matches!(err, ReferenceError::Malformed(_)));
    }

    #[test]
    fn test_file_path_rejected() {
        let err = validate_data_uri("/home/user/image.png").unwrap_err();
        assert!(matches!(err, ReferenceError::NotADataUri(_)));
    }

    #[test]
    fn test_http_url_rejected() {
        let err = validate_data_uri("http://example.com/image.png").unwrap_err();
        assert!(matches!(err, ReferenceError::NotADataUri(_)));
    }
}
