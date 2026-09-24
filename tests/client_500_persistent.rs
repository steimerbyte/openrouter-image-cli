//! Integration test: two consecutive 500 responses → exit 4 after 1 backoff.

use std::env;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Verifies:
/// - Two 500 responses → single backoff, then return error → exit 4
#[tokio::test]
async fn test_client_500_persistent_exit_4() {
    let mock_server = MockServer::start().await;

    let prev_url = env::var("OPENROUTER_BASE_URL").ok();
    env::set_var(
        "OPENROUTER_BASE_URL",
        format!("{}/api/v1/images", mock_server.uri()),
    );

    let call_count = Arc::new(AtomicUsize::new(0));
    let call_count_clone = call_count.clone();

    Mock::given(method("POST"))
        .and(path("/api/v1/images"))
        .and(header("Authorization", "Bearer sk-test"))
        .respond_with(move |_req: &wiremock::Request| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
            ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "error": { "message": "Internal server error" }
            }))
        })
        .mount(&mock_server)
        .await;

    let params = openrouter_image_core::GenerationParams {
        prompt: "blue circle".to_string(),
        model: "openai/gpt-image-2".to_string(),
        image_refs: vec![],
        output_paths: vec![PathBuf::from("/tmp/output.png")],
        n: 1,
        timeout_ms: 30_000,
    };

    let config = openrouter_image_core::Config {
        api_key: Some("sk-test".to_string()),
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
    assert_eq!(
        api_err.exit_code(),
        4,
        "500 after 1 retry should map to exit 4"
    );

    // Verify that 2 requests were made (initial + 1 retry)
    assert_eq!(
        call_count.load(Ordering::SeqCst),
        2,
        "should have made 2 requests"
    );
}
