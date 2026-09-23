//! Tests for API response parsing and error classification.

use std::time::Duration;

use openrouter_image_core::{
    ApiError, ApiResponse, AspectRatio, Config, GenerationParams, ImageModel, OutputFormat, Quality,
};

#[test]
fn test_success_response_parsing() {
    let body = serde_json::json!({
        "created": 1234567890,
        "data": [{
            "b64_json": "SGVsbG8gV29ybGQ=",
            "media_type": "image/png"
        }],
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 5,
            "total_tokens": 15,
            "cost": 0.001
        }
    });

    let resp: ApiResponse = serde_json::from_value(body).unwrap();
    assert_eq!(resp.created, Some(1234567890));
    assert_eq!(resp.data.as_ref().map(|d| d.len()), Some(1));
    assert_eq!(
        resp.data.as_ref().unwrap()[0].b64_json.as_deref(),
        Some("SGVsbG8gV29ybGQ=")
    );
    assert_eq!(
        resp.data.as_ref().unwrap()[0].media_type.as_deref(),
        Some("image/png")
    );
    let usage = resp.usage.as_ref().unwrap();
    assert_eq!(usage.prompt_tokens, Some(10));
    assert_eq!(usage.total_tokens, Some(15));
    assert!((usage.cost.unwrap() - 0.001).abs() < 1e-9);
}

#[test]
fn test_error_401_parsing() {
    let body = serde_json::json!({
        "error": { "message": "Invalid API key" }
    });
    let resp: ApiResponse = serde_json::from_value(body).unwrap();
    assert!(resp.error.is_some());
    assert_eq!(
        resp.error.as_ref().unwrap().message.as_deref(),
        Some("Invalid API key")
    );
}

#[test]
fn test_error_402_parsing() {
    let body = serde_json::json!({
        "error": { "message": "Insufficient credits" }
    });
    let resp: ApiResponse = serde_json::from_value(body).unwrap();
    assert!(resp.error.is_some());
}

#[test]
fn test_error_429_parsing() {
    let body = serde_json::json!({
        "error": { "message": "Rate limited" }
    });
    let resp: ApiResponse = serde_json::from_value(body).unwrap();
    assert!(resp.error.is_some());
}

#[test]
fn test_error_500_parsing() {
    let body = serde_json::json!({
        "error": { "message": "Internal server error" }
    });
    let resp: ApiResponse = serde_json::from_value(body).unwrap();
    assert!(resp.error.is_some());
}

#[test]
fn test_empty_data_array() {
    let body = serde_json::json!({
        "created": 1234567890,
        "data": [],
        "usage": { "total_tokens": 1 }
    });
    let resp: ApiResponse = serde_json::from_value(body).unwrap();
    assert!(resp.data.as_ref().map(|d| d.is_empty()).unwrap_or(false));
}

#[test]
fn test_missing_b64_json() {
    let body = serde_json::json!({
        "created": 1234567890,
        "data": [{ "media_type": "image/png" }]
    });
    let resp: ApiResponse = serde_json::from_value(body).unwrap();
    assert_eq!(resp.data.as_ref().map(|d| d.len()), Some(1));
    assert!(resp.data.as_ref().unwrap()[0].b64_json.is_none());
}

#[test]
fn test_api_error_from_status_500() {
    let err = ApiError::from_status(500, "{}");
    assert!(matches!(err, ApiError::Http { status: 500, .. }));
}

#[test]
fn test_api_error_from_status_429() {
    let err = ApiError::from_status(429, "{}");
    assert!(matches!(err, ApiError::Http { status: 429, .. }));
}

#[test]
fn test_api_error_from_status_401() {
    let err = ApiError::from_status(401, "{}");
    assert!(matches!(err, ApiError::Http { status: 401, .. }));
}

#[test]
fn test_api_error_from_status_402() {
    let err = ApiError::from_status(402, "{}");
    assert!(matches!(err, ApiError::Http { status: 402, .. }));
}

#[test]
fn test_api_error_timeout_exit_code() {
    let err = ApiError::Timeout(Duration::from_secs(30));
    assert_eq!(err.exit_code(), 5);
}

#[test]
fn test_api_error_no_key_exit_code() {
    let err = ApiError::NoApiKey;
    assert_eq!(err.exit_code(), 3);
}

#[test]
fn test_api_error_http_401_exit_code() {
    let err = ApiError::from_status(401, "{}");
    assert_eq!(err.exit_code(), 3);
}

#[test]
fn test_api_error_http_429_exit_code() {
    let err = ApiError::from_status(429, "{}");
    assert_eq!(err.exit_code(), 4);
}

#[test]
fn test_api_error_http_400_exit_code() {
    let err = ApiError::from_status(400, "{}");
    assert_eq!(err.exit_code(), 4);
}

#[test]
fn test_generation_params_to_request_body() {
    let params = GenerationParams {
        prompt: "test prompt".to_string(),
        model: ImageModel::GptImage2,
        reference: None,
        aspect_ratio: AspectRatio::R16x9,
        quality: Some(Quality::High),
        background: None,
        output_format: OutputFormat::Png,
        resolution: None,
        n: 2,
        seed: Some(42),
        output_dir: std::path::PathBuf::from("/tmp"),
        timeout_ms: 120_000,
        retry: true,
    };

    let body = params.to_request_body(None);
    assert_eq!(body["model"], "openai/gpt-image-2");
    assert_eq!(body["prompt"], "test prompt");
    assert_eq!(body["aspect_ratio"], "16:9");
    assert_eq!(body["quality"], "high");
    assert_eq!(body["output_format"], "png");
    assert_eq!(body["n"], 2);
    assert_eq!(body["seed"], 42);
    assert!(body.get("input_references").is_none());
}

#[test]
fn test_generation_params_with_reference() {
    let params = GenerationParams {
        prompt: "test".to_string(),
        model: ImageModel::GptImage2,
        reference: None,
        aspect_ratio: AspectRatio::R1x1,
        quality: None,
        background: None,
        output_format: OutputFormat::Png,
        resolution: None,
        n: 1,
        seed: None,
        output_dir: std::path::PathBuf::from("/tmp"),
        timeout_ms: 120_000,
        retry: true,
    };

    let body = params.to_request_body(Some("data:image/png;base64,abc".to_string()));
    assert!(body.get("input_references").is_some());
    let refs = body["input_references"].as_array().unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0]["type"], "image_url");
    assert_eq!(refs[0]["image_url"]["url"], "data:image/png;base64,abc");
}

#[test]
fn test_config_masked_key() {
    let cfg = Config {
        api_key: Some("sk-or-v1-abcdef1234567890xyz".to_string()),
        config_file: std::path::PathBuf::from("/tmp/test.json"),
    };
    assert_eq!(cfg.masked_key(), "sk-or-v1…0xyz");
}

#[test]
fn test_config_masked_key_short() {
    let cfg = Config {
        api_key: Some("short".to_string()),
        config_file: std::path::PathBuf::from("/tmp/test.json"),
    };
    assert_eq!(cfg.masked_key(), "sho…");
}

#[test]
fn test_config_masked_key_none() {
    let cfg = Config {
        api_key: None,
        config_file: std::path::PathBuf::from("/tmp/test.json"),
    };
    assert_eq!(cfg.masked_key(), "—");
}

#[test]
fn test_output_format_ext() {
    assert_eq!(OutputFormat::Png.to_ext(), "png");
    assert_eq!(OutputFormat::Jpeg.to_ext(), "jpg");
    assert_eq!(OutputFormat::Webp.to_ext(), "webp");
    assert_eq!(OutputFormat::Svg.to_ext(), "svg");
}

#[test]
fn test_image_model_display() {
    assert_eq!(ImageModel::GptImage2.to_string(), "openai/gpt-image-2");
    assert_eq!(ImageModel::GptImage1.to_string(), "openai/gpt-image-1");
    assert_eq!(ImageModel::GptImage1Mini.to_string(), "openai/gpt-image-1-mini");
    assert_eq!(ImageModel::Gpt5Image.to_string(), "openai/gpt-5-image");
    assert_eq!(ImageModel::Gpt5ImageMini.to_string(), "openai/gpt-5-image-mini");
    assert_eq!(ImageModel::Gpt54Image2.to_string(), "openai/gpt-5.4-image-2");
}

#[test]
fn test_aspect_ratio_display() {
    assert_eq!(AspectRatio::R1x1.to_str(), "1:1");
    assert_eq!(AspectRatio::R16x9.to_str(), "16:9");
    assert_eq!(AspectRatio::R9x16.to_str(), "9:16");
    assert_eq!(AspectRatio::Auto.to_str(), "auto");
}

#[test]
fn test_quality_display() {
    assert_eq!(Quality::Auto.to_str(), "auto");
    assert_eq!(Quality::Low.to_str(), "low");
    assert_eq!(Quality::High.to_str(), "high");
}
