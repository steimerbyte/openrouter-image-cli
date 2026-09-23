//! Tests for reference image resolution.

use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_data_url_pass_through() {
    let url = "data:image/png;base64,SGVsbG8=";
    assert_eq!(
        openrouter_image_core::resolve_reference(url).unwrap(),
        url
    );
}

#[test]
fn test_https_pass_through() {
    let url = "https://example.com/image.png";
    assert_eq!(
        openrouter_image_core::resolve_reference(url).unwrap(),
        url
    );
}

#[test]
fn test_http_pass_through() {
    let url = "http://example.com/image.png";
    assert_eq!(
        openrouter_image_core::resolve_reference(url).unwrap(),
        url
    );
}

#[test]
fn test_local_file_png() {
    let mut f = NamedTempFile::with_suffix(".png").unwrap();
    f.write_all(b"fake png content").unwrap();
    let path = f.path().to_str().unwrap();
    let result = openrouter_image_core::resolve_reference(path).unwrap();
    assert!(result.starts_with("data:image/png;base64,"));
}

#[test]
fn test_local_file_jpeg() {
    let mut f = NamedTempFile::with_suffix(".jpeg").unwrap();
    f.write_all(b"fake jpeg").unwrap();
    let result =
        openrouter_image_core::resolve_reference(f.path().to_str().unwrap()).unwrap();
    assert!(result.starts_with("data:image/jpeg;base64,"));
}

#[test]
fn test_local_file_webp() {
    let mut f = NamedTempFile::with_suffix(".webp").unwrap();
    f.write_all(b"fake webp").unwrap();
    let result =
        openrouter_image_core::resolve_reference(f.path().to_str().unwrap()).unwrap();
    assert!(result.starts_with("data:image/webp;base64,"));
}

#[test]
fn test_local_file_unknown_ext_defaults_png() {
    let f = NamedTempFile::new().unwrap();
    let result =
        openrouter_image_core::resolve_reference(f.path().to_str().unwrap()).unwrap();
    // Unknown extension defaults to image/png
    assert!(result.starts_with("data:image/png;base64,"));
}

#[test]
fn test_missing_file() {
    let result = openrouter_image_core::resolve_reference("/nonexistent/path/file.png");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[test]
fn test_base64_decoding_roundtrip() {
    use base64::Engine;
    let data = b"Hello, World!";
    let encoded = base64::engine::general_purpose::STANDARD.encode(data);
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&encoded)
        .unwrap();
    assert_eq!(decoded, data);
}
