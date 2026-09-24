//! Integration test: HTTP(S) reference images are accepted and passed through.
//!
//! Verifies that `validate_reference` accepts http:// and https:// URLs,
//! and that they are correctly embedded in the request body as `input_references`.

use openrouter_image_core::GenerationParams;

#[test]
fn test_http_url_reference_passed_to_body() {
    let params = GenerationParams {
        prompt: "edit this photo".to_string(),
        model: "openai/gpt-image-2".to_string(),
        image_refs: vec!["https://example.com/user-photo.jpg".to_string()],
        output_paths: vec![],
        n: 1,
        resolution: None,
        aspect_ratio: None,
        background: None,
        output_format: None,
        output_compression: None,
        quality: None,
        seed: None,
        size: None,
        user: None,
        session_id: None,
        provider: None,
        trace: None,
        stream: false,
        timeout_ms: 120_000,
    };

    let body = params.to_request_body();
    let refs = body["input_references"].as_array().unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0]["type"], "image_url");
    assert_eq!(
        refs[0]["image_url"]["url"],
        "https://example.com/user-photo.jpg"
    );
}

#[test]
fn test_multiple_http_urls_passed_to_body() {
    let params = GenerationParams {
        prompt: "blend these images".to_string(),
        model: "openai/gpt-image-2".to_string(),
        image_refs: vec![
            "https://cdn.example.com/photo1.png".to_string(),
            "http://internal.local/photo2.jpg".to_string(),
            "data:image/png;base64,ZXhhbXBsZQ==".to_string(),
        ],
        output_paths: vec![],
        n: 1,
        resolution: None,
        aspect_ratio: None,
        background: None,
        output_format: None,
        output_compression: None,
        quality: None,
        seed: None,
        size: None,
        user: None,
        session_id: None,
        provider: None,
        trace: None,
        stream: false,
        timeout_ms: 120_000,
    };

    let body = params.to_request_body();
    let refs = body["input_references"].as_array().unwrap();
    assert_eq!(refs.len(), 3);
    assert_eq!(
        refs[0]["image_url"]["url"],
        "https://cdn.example.com/photo1.png"
    );
    assert_eq!(
        refs[1]["image_url"]["url"],
        "http://internal.local/photo2.jpg"
    );
    assert_eq!(
        refs[2]["image_url"]["url"],
        "data:image/png;base64,ZXhhbXBsZQ=="
    );
}

#[test]
fn test_validate_reference_accepts_https() {
    let url = "https://openrouter.ai/assets/sample.jpg";
    assert!(openrouter_image_core::validate_reference(url).is_ok());
    assert_eq!(openrouter_image_core::validate_reference(url).unwrap(), url);
}

#[test]
fn test_validate_reference_accepts_http() {
    let url = "http://example.com/image.png";
    assert!(openrouter_image_core::validate_reference(url).is_ok());
}

#[test]
fn test_validate_reference_accepts_https_with_complex_path() {
    let url = "https://storage.googleapis.com/my-bucket/images/photo.png?X-Goog-Signature=abc123";
    assert!(openrouter_image_core::validate_reference(url).is_ok());
}

#[test]
fn test_validate_reference_rejects_empty_http_host() {
    let url = "http://";
    assert!(openrouter_image_core::validate_reference(url).is_err());
    let err = openrouter_image_core::validate_reference(url).unwrap_err();
    assert!(matches!(
        err,
        openrouter_image_core::ReferenceError::Malformed(_)
    ));
}
