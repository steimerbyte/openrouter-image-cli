//! Integration test: happy path with wiremock.
//!
//! Uses `HttpClient::new_with_url` to point at the mock server.

use base64::Engine;
use std::path::PathBuf;
use tempfile::TempDir;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Verifies: HTTP 200 with b64 PNG → HttpClient returns parsed response.
#[tokio::test]
async fn test_client_200_single_response_parsing() {
    // Minimal valid 1×1 PNG
    let png_bytes = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // 1×1 px
        0x08, 0x02, 0x00, 0x00, 0x00, // 8-bit RGB
        0x90, 0x77, 0x53, 0xDE, // IHDR CRC
        0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, // IDAT chunk
        0x08, 0xD7, 0x63, 0xF8, 0xFF, 0xFF, 0xFF, 0x00, // compressed pixel
        0x05, 0xFE, 0x02, 0xFE, // IDAT CRC
        0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, // IEND chunk
        0xAE, 0x42, 0x60, 0x82, // IEND CRC
    ];
    let b64_png = base64::engine::general_purpose::STANDARD.encode(png_bytes);

    let mock_server = MockServer::start().await;
    let mock_url = format!("{}/api/v1/images", mock_server.uri());

    Mock::given(method("POST"))
        .and(path("/api/v1/images"))
        .and(header("Authorization", "Bearer sk-or-test"))
        .and(body_json(serde_json::json!({
            "model": "openai/gpt-image-2",
            "prompt": "blue circle"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "created": 1234567890,
            "data": [{
                "b64_json": b64_png,
                "media_type": "image/png"
            }],
            "usage": { "total_tokens": 10 }
        })))
        .mount(&mock_server)
        .await;

    let tmp_dir = TempDir::new().unwrap();
    let output_path = tmp_dir.path().join("output.png");

    let params = openrouter_image_core::GenerationParams {
        prompt: "blue circle".to_string(),
        model: "openai/gpt-image-2".to_string(),
        image_refs: vec![],
        output_paths: vec![output_path.clone()],
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
        timeout_ms: 30_000,
        clobber: false,
        max_image_retries: 0,
        negative_prompt: None,
    };

    let config = openrouter_image_core::Config {
        api_key: Some("sk-or-test".to_string()),
        default_model: None,
        config_file: PathBuf::from("/tmp/config.toml"),
        config_file_exists: false,
    };

    // Create client pointing at mock server
    let client = openrouter_image_core::HttpClient::new_with_url(30_000, mock_url)
        .expect("client creation ok");

    let (response, _elapsed) = client
        .call(&params, &config)
        .await
        .expect("HTTP call should succeed");

    assert_eq!(response.data.as_ref().map(|d| d.len()), Some(1));
    let img = response.data.as_ref().unwrap().first().unwrap();
    assert_eq!(img.media_type.as_deref(), Some("image/png"));

    // Verify file writing end-to-end
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(img.b64_json.as_ref().unwrap())
        .expect("valid base64");
    std::fs::write(&output_path, &decoded).expect("write ok");
    assert_eq!(std::fs::read(&output_path).unwrap(), png_bytes);
}
