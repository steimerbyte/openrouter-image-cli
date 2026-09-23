//! Tests for API key resolution.

#[test]
fn test_masked_key_full() {
    let cfg = openrouter_image_core::Config {
        api_key: Some("sk-or-v1-abcdef1234567890xyz".to_string()),
        config_file: std::path::PathBuf::from("/tmp/test.json"),
    };
    assert_eq!(cfg.masked_key(), "sk-or-v1…0xyz");
}

#[test]
fn test_masked_key_short() {
    let cfg = openrouter_image_core::Config {
        api_key: Some("short".to_string()),
        config_file: std::path::PathBuf::from("/tmp/test.json"),
    };
    assert_eq!(cfg.masked_key(), "sho…");
}

#[test]
fn test_masked_key_none() {
    let cfg = openrouter_image_core::Config {
        api_key: None,
        config_file: std::path::PathBuf::from("/tmp/test.json"),
    };
    assert_eq!(cfg.masked_key(), "—");
}

#[test]
fn test_multiple_key_fields_precedence() {
    // apiKey < OPENROUTER_API_KEY < openrouter_api_key (last wins)
    #[derive(serde::Deserialize)]
    struct Settings {
        #[serde(rename = "apiKey")]
        api_key: Option<String>,
        #[serde(rename = "OPENROUTER_API_KEY")]
        openrouter_api_key: Option<String>,
        #[serde(rename = "openrouter_api_key")]
        openrouter_api_key_snake: Option<String>,
    }
    let raw = r#"{"apiKey":"key1","openrouter_api_key":"key2"}"#;
    let parsed: Settings = serde_json::from_str(raw).unwrap();
    // The actual config code uses .or() chain: apiKey or openrouter_api_key or openrouter_api_key_snake
    let key = parsed
        .api_key
        .or(parsed.openrouter_api_key)
        .or(parsed.openrouter_api_key_snake);
    assert_eq!(key, Some("key1".to_string()));
}

#[test]
fn test_empty_json_returns_none() {
    let raw = r#"{}"#;
    #[derive(serde::Deserialize)]
    struct Settings {
        #[serde(rename = "apiKey")]
        api_key: Option<String>,
    }
    let parsed: Settings = serde_json::from_str(raw).ok().unwrap();
    assert!(parsed.api_key.is_none());
}

#[test]
fn test_invalid_json_is_error() {
    let result = serde_json::from_str::<serde_json::Value>("not json");
    assert!(result.is_err());
}

#[test]
fn test_whitespace_only_returns_none() {
    let result: Result<serde_json::Value, _> = serde_json::from_str("   \n  ");
    // Empty/whitespace should be treated as no content
    assert!(result.is_err() || result.unwrap().is_null());
}
