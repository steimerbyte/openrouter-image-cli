//! Tests for image reference validation.

#[test]
fn test_valid_png_data_uri() {
    let uri = "data:image/png;base64,SGVsbG8=";
    assert_eq!(openrouter_image_core::validate_reference(uri).unwrap(), uri);
}

#[test]
fn test_valid_jpeg_data_uri() {
    let uri = "data:image/jpeg;base64,/9j/4AAQ";
    assert_eq!(openrouter_image_core::validate_reference(uri).unwrap(), uri);
}

#[test]
fn test_valid_webp_data_uri() {
    let uri = "data:image/webp;base64,UklGRlY=";
    assert_eq!(openrouter_image_core::validate_reference(uri).unwrap(), uri);
}

// --- HTTP(S) URLs are now ACCEPTED per OpenRouter spec ---

#[test]
fn test_valid_https_url_accepted() {
    let url = "https://example.com/images/photo.png";
    assert_eq!(openrouter_image_core::validate_reference(url).unwrap(), url);
}

#[test]
fn test_valid_http_url_accepted() {
    let url = "http://example.com/image.png";
    assert_eq!(openrouter_image_core::validate_reference(url).unwrap(), url);
}

#[test]
fn test_valid_https_url_with_port_accepted() {
    let url = "https://example.com:8080/images/photo.png";
    assert_eq!(openrouter_image_core::validate_reference(url).unwrap(), url);
}

// --- Malformed HTTP(S) URLs are still REJECTED ---

#[test]
fn test_empty_http_url_rejected() {
    let err = openrouter_image_core::validate_reference("http://").unwrap_err();
    assert!(matches!(
        err,
        openrouter_image_core::ReferenceError::Malformed(_)
    ));
}

#[test]
fn test_empty_https_url_rejected() {
    let err = openrouter_image_core::validate_reference("https://").unwrap_err();
    assert!(matches!(
        err,
        openrouter_image_core::ReferenceError::Malformed(_)
    ));
}

// --- Invalid data URIs are still REJECTED ---

#[test]
fn test_non_url_string_rejected() {
    // Plain strings without a valid URL/data-URI scheme are rejected
    let err = openrouter_image_core::validate_reference("just-some-text").unwrap_err();
    assert!(matches!(
        err,
        openrouter_image_core::ReferenceError::NotADataUri(_)
    ));
}

#[test]
fn test_missing_base64_marker() {
    let err =
        openrouter_image_core::validate_reference("data:image/png;binary,SGVsbG8=").unwrap_err();
    assert!(matches!(
        err,
        openrouter_image_core::ReferenceError::NotBase64(_)
    ));
}

#[test]
fn test_missing_semicolon() {
    let err = openrouter_image_core::validate_reference("datapng;base64,SGVsbG8=").unwrap_err();
    assert!(matches!(
        err,
        openrouter_image_core::ReferenceError::NotADataUri(_)
    ));
}

#[test]
fn test_empty_after_data() {
    let err = openrouter_image_core::validate_reference("data:").unwrap_err();
    assert!(matches!(
        err,
        openrouter_image_core::ReferenceError::Malformed(_)
    ));
}

#[test]
fn test_only_data_prefix_no_semicolon() {
    let err = openrouter_image_core::validate_reference("data:image/png").unwrap_err();
    assert!(matches!(
        err,
        openrouter_image_core::ReferenceError::Malformed(_)
    ));
}

#[test]
fn test_file_path_rejected() {
    let err = openrouter_image_core::validate_reference("/home/user/image.png").unwrap_err();
    assert!(matches!(
        err,
        openrouter_image_core::ReferenceError::NotADataUri(_)
    ));
}
