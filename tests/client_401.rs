//! Integration test: 401 response → exit 3.

use std::env;
use std::path::PathBuf;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Verifies: HTTP 401 → exit 3 (auth error).
#[tokio::test]
async fn test_client_401_exit_3() {
    let mock_server = MockServer::start().await;

    let prev_url = env::var("OPENROUTER_BASE_URL").ok();
    env::set_var(
        "OPENROUTER_BASE_URL",
        format!("{}/api/v1/images", mock_server.uri()),
    );

    Mock::given(method("POST"))
        .and(path("/api/v1/images"))
        .and(header("Authorization", "Bearer sk-bad-key"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": { "message": "Invalid API key" }
        })))
        .mount(&mock_server)
        .await;

    let params = openrouter_image_core::GenerationParams {
        prompt: "blue circle".to_string(),
        model: "openai/gpt-image-2".to_string(),
        image_refs: vec![],
        output_paths: vec![PathBuf::from("/tmp/output.png")],
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
        api_key: Some("sk-bad-key".to_string()),
        default_model: None,
        config_file: PathBuf::from("/tmp/config.toml"),
        config_file_exists: false,
    };

    let err = openrouter_image_core::run_with_progress(
        params,
        config,
        openrouter_image_core::OutputMode::Human,
    )
    .await
    .unwrap_err();

    if let Some(v) = prev_url {
        env::set_var("OPENROUTER_BASE_URL", v);
    } else {
        env::remove_var("OPENROUTER_BASE_URL");
    }

    let api_err = err.downcast::<openrouter_image_core::ApiError>().unwrap();
    assert_eq!(api_err.exit_code(), 3, "401 should map to exit 3");
}
