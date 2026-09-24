//! Integration test: list_models filter correctness.
//!
//! Uses the real `architecture.output_modalities` + `supported_parameters`
//! shape that OpenRouter returns. Filtering is API-driven, not keyword-based.

use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_list_models_filter() {
    let mock_server = MockServer::start().await;

    let all_models_response = serde_json::json!({
        "data": [
            {
                "id": "openai/gpt-5-image",
                "name": "OpenAI: GPT-5 Image",
                "context_length": 400000,
                "supported_parameters": ["temperature", "seed", "resolution"],
                "architecture": {
                    "modality": "text+image+file->text+image",
                    "input_modalities": ["text", "image", "file"],
                    "output_modalities": ["text", "image"]
                }
            },
            {
                "id": "anthropic/claude-3.5-sonnet",
                "name": "Claude 3.5 Sonnet",
                "context_length": 200000,
                "supported_parameters": ["temperature", "tools"],
                "architecture": {
                    "modality": "text+image->text",
                    "input_modalities": ["text", "image"],
                    "output_modalities": ["text"]
                }
            },
            {
                "id": "black-forest-labs/flux-1.1-pro",
                "name": "Flux 1.1 Pro",
                "context_length": null,
                "supported_parameters": ["resolution", "image_size"],
                "architecture": {
                    "modality": "text->image",
                    "input_modalities": ["text"],
                    "output_modalities": ["image"]
                }
            },
            {
                "id": "google/gemini-3-pro-image",
                "name": "Gemini 3 Pro Image",
                "context_length": 1000000,
                "supported_parameters": ["temperature", "tools"],
                "architecture": {
                    "modality": "text+image->text+image",
                    "input_modalities": ["text", "image"],
                    "output_modalities": ["text", "image"]
                }
            },
            {
                "id": "meta/llama-3.1-8b",
                "name": "Llama 3.1 8B",
                "context_length": 128000,
                "supported_parameters": ["temperature"],
                "architecture": {
                    "modality": "text->text",
                    "input_modalities": ["text"],
                    "output_modalities": ["text"]
                }
            }
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

    // Image-capable (output_modalities contains "image") must be included.
    assert!(
        ids.contains(&"openai/gpt-5-image"),
        "should include openai/gpt-5-image"
    );
    assert!(
        ids.contains(&"black-forest-labs/flux-1.1-pro"),
        "should include flux-1.1-pro"
    );
    assert!(
        ids.contains(&"google/gemini-3-pro-image"),
        "should include gemini-3-pro-image"
    );

    // Text-only models must be excluded.
    assert!(
        !ids.contains(&"anthropic/claude-3.5-sonnet"),
        "should NOT include claude"
    );
    assert!(
        !ids.contains(&"meta/llama-3.1-8b"),
        "should NOT include llama"
    );

    assert_eq!(ids.len(), 3, "expected exactly 3 image-capable models");

    // Resolution support is detected from supported_parameters.
    let gpt5 = models.iter().find(|m| m.id == "openai/gpt-5-image").unwrap();
    assert!(
        openrouter_image_core::supports_resolution(gpt5),
        "gpt-5-image should advertise resolution support"
    );

    let gemini = models
        .iter()
        .find(|m| m.id == "google/gemini-3-pro-image")
        .unwrap();
    assert!(
        !openrouter_image_core::supports_resolution(gemini),
        "gemini-3-pro-image should NOT advertise resolution support"
    );
}
