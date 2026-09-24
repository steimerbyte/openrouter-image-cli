//! Integration test: list_models filter correctness.

use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Verifies: filter correctly identifies image-capable models.
#[tokio::test]
async fn test_list_models_filter() {
    let mock_server = MockServer::start().await;

    let all_models_response = serde_json::json!({
        "data": [
            { "id": "openai/gpt-image-2", "name": "GPT Image 2", "context_length": 1024 },
            { "id": "anthropic/claude-3.5-sonnet", "name": "Claude 3.5 Sonnet", "context_length": 200000 },
            { "id": "black-forest-labs/flux-1.1-pro", "name": "Flux 1.1 Pro", "context_length": null },
            { "id": "google/imagen-3", "name": "Imagen 3", "context_length": 8192 },
            { "id": "openai/gpt-4o", "name": "GPT-4o", "context_length": 128000 },
            { "id": "meta/llama-3.1-8b", "name": "Llama 3.1 8B", "context_length": 128000 },
            { "id": "google/gemini-2.0-flash-exp", "name": "Gemini 2.0 Flash Exp", "context_length": 1000000 },
        ]
    });

    Mock::given(method("GET"))
        .and(path("/api/v1/models"))
        .and(header("Authorization", "Bearer sk-test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&all_models_response))
        .mount(&mock_server)
        .await;

    let client = reqwest::Client::new();
    let models = openrouter_image_core::fetch_image_models(
        &client,
        "sk-test",
        Some(&format!("{}/api/v1/models", mock_server.uri())),
    )
    .await
    .expect("fetch should succeed");

    let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();

    // Must include image-capable models
    assert!(
        ids.contains(&"openai/gpt-image-2"),
        "should include openai/gpt-image-2"
    );
    assert!(
        ids.contains(&"black-forest-labs/flux-1.1-pro"),
        "should include flux"
    );
    assert!(ids.contains(&"google/imagen-3"), "should include imagen-3");
    assert!(
        ids.contains(&"google/gemini-2.0-flash-exp"),
        "should include gemini-2.0-flash-exp"
    );

    // Must NOT include non-image models
    assert!(
        !ids.contains(&"anthropic/claude-3.5-sonnet"),
        "should NOT include claude"
    );
    assert!(!ids.contains(&"openai/gpt-4o"), "should NOT include gpt-4o");
    assert!(
        !ids.contains(&"meta/llama-3.1-8b"),
        "should NOT include llama"
    );

    assert_eq!(ids.len(), 4, "expected exactly 4 image models");
}
