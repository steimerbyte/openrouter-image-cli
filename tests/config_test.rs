//! Tests for TOML config resolution and API key handling.

use openrouter_image_core::Config;

#[test]
fn test_masked_key_full() {
    let cfg = Config {
        api_key: Some("sk-or-v1-abcdefghijklmnopqrstuvwxyz012345".to_string()),
        default_model: None,
        config_file: std::path::PathBuf::from("/tmp/config.toml"),
        config_file_exists: true,
    };
    assert_eq!(cfg.masked_key(), "sk-or-v1…2345");
}

#[test]
fn test_masked_key_short() {
    let cfg = Config {
        api_key: Some("short".to_string()),
        default_model: None,
        config_file: std::path::PathBuf::from("/tmp/config.toml"),
        config_file_exists: false,
    };
    assert_eq!(cfg.masked_key(), "sho…");
}

#[test]
fn test_masked_key_none() {
    let cfg = Config {
        api_key: None,
        default_model: None,
        config_file: std::path::PathBuf::from("/tmp/config.toml"),
        config_file_exists: false,
    };
    assert_eq!(cfg.masked_key(), "—");
}

#[test]
fn test_toml_parse_valid() {
    let text = r#"
api_key = "sk-or-v1-test-key-123"
default_model = "openai/gpt-image-2"
"#;
    let parsed: toml::Value = toml::from_str(text).unwrap();
    assert_eq!(
        parsed.get("api_key").and_then(|v| v.as_str()),
        Some("sk-or-v1-test-key-123")
    );
}

#[test]
fn test_toml_parse_no_key() {
    let text = r#"
default_model = "openai/gpt-image-2"
"#;
    let parsed: toml::Value = toml::from_str(text).unwrap();
    assert!(parsed.get("api_key").is_none());
}

#[test]
fn test_toml_parse_empty_string() {
    let text = r#"api_key = """#;
    let parsed: toml::Value = toml::from_str(text).unwrap();
    let key = parsed.get("api_key").and_then(|v| v.as_str());
    assert_eq!(key, Some(""));
}
