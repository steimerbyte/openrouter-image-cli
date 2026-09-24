//! Tests for data-URI validation of image reference arguments.

#[test]
fn test_valid_png_data_uri() {
    let uri = "data:image/png;base64,SGVsbG8=";
    assert_eq!(openrouter_image_core::validate_data_uri(uri).unwrap(), uri);
}

#[test]
fn test_valid_jpeg_data_uri() {
    let uri = "data:image/jpeg;base64,/9j/4AAQ";
    assert_eq!(openrouter_image_core::validate_data_uri(uri).unwrap(), uri);
}

#[test]
fn test_valid_webp_data_uri() {
    let uri = "data:image/webp;base64,UklGRlY=";
    assert_eq!(openrouter_image_core::validate_data_uri(uri).unwrap(), uri);
}

#[test]
fn test_missing_data_prefix() {
    let err =
        openrouter_image_core::validate_data_uri("https://example.com/image.png").unwrap_err();
    assert!(matches!(
        err,
        openrouter_image_core::ReferenceError::NotADataUri(_)
    ));
}

#[test]
fn test_missing_base64_marker() {
    let err =
        openrouter_image_core::validate_data_uri("data:image/png;binary,SGVsbG8=").unwrap_err();
    assert!(matches!(
        err,
        openrouter_image_core::ReferenceError::NotBase64(_)
    ));
}

#[test]
fn test_missing_semicolon() {
    let err = openrouter_image_core::validate_data_uri("datapng;base64,SGVsbG8=").unwrap_err();
    assert!(matches!(
        err,
        openrouter_image_core::ReferenceError::NotADataUri(_)
    ));
}

#[test]
fn test_empty_after_data() {
    let err = openrouter_image_core::validate_data_uri("data:").unwrap_err();
    assert!(matches!(
        err,
        openrouter_image_core::ReferenceError::Malformed(_)
    ));
}

#[test]
fn test_only_data_prefix_no_semicolon() {
    let err = openrouter_image_core::validate_data_uri("data:image/png").unwrap_err();
    assert!(matches!(
        err,
        openrouter_image_core::ReferenceError::Malformed(_)
    ));
}

#[test]
fn test_file_path_rejected() {
    let err = openrouter_image_core::validate_data_uri("/home/user/image.png").unwrap_err();
    assert!(matches!(
        err,
        openrouter_image_core::ReferenceError::NotADataUri(_)
    ));
}

#[test]
fn test_http_url_rejected() {
    let err = openrouter_image_core::validate_data_uri("http://example.com/image.png").unwrap_err();
    assert!(matches!(
        err,
        openrouter_image_core::ReferenceError::NotADataUri(_)
    ));
}
