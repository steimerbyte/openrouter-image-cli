//! Tests for TOML config resolution and API key handling.

use openrouter_image_core::Config;
use openrouter_image_core::ConfigError;

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

// ---------------------------------------------------------------------------
// Security tests: F1 (insecure permissions) and F2 (symlink)
// ---------------------------------------------------------------------------

/// F1: A world-readable config file must be rejected with
/// ConfigError::InsecurePermissions (exit code 3).
#[cfg(unix)]
#[test]
fn test_world_readable_config_rejected() {
    use std::env;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    let tmp_dir = TempDir::new().unwrap();
    let config_dir = tmp_dir.path().join("openrouter-image");
    std::fs::create_dir_all(&config_dir).unwrap();
    let config_path = config_dir.join("config.toml");

    std::fs::write(&config_path, "api_key = \"sk-o-test-key-for-perms-test\"").unwrap();

    // Set world-readable permissions (0o644) — violates the 0o600 rule.
    let mut perms = std::fs::metadata(&config_path).unwrap().permissions();
    perms.set_mode(0o644);
    std::fs::set_permissions(&config_path, perms).unwrap();

    let original_xdg = env::var("XDG_CONFIG_HOME").ok();
    let original_key = env::var("OPENROUTER_API_KEY").ok();
    env::set_var("XDG_CONFIG_HOME", tmp_dir.path());
    env::remove_var("OPENROUTER_API_KEY");

    let result = openrouter_image_core::Config::resolve();

    if let Some(v) = original_xdg {
        env::set_var("XDG_CONFIG_HOME", v);
    } else {
        env::remove_var("XDG_CONFIG_HOME");
    }
    if let Some(v) = original_key {
        env::set_var("OPENROUTER_API_KEY", v);
    }

    let err = result.expect_err("world-readable config must be rejected");
    match &err {
        ConfigError::InsecurePermissions(_mode) => {}
        other => panic!("expected InsecurePermissions, got: {:?}", other),
    }
    assert_eq!(
        err.exit_code(),
        3,
        "InsecurePermissions must exit with code 3"
    );
}

/// F2: A symlink at the config path must be rejected with
/// ConfigError::ConfigIsSymlink (exit code 3) to prevent
/// symlink-target exfiltration attacks.
#[cfg(unix)]
#[test]
fn test_symlink_config_rejected() {
    use std::env;
    use tempfile::TempDir;

    let tmp_dir = TempDir::new().unwrap();
    let config_dir = tmp_dir.path().join("openrouter-image");
    std::fs::create_dir_all(&config_dir).unwrap();

    // Real config file (not accessed via symlink path).
    let real_config = config_dir.join("real_config.toml");
    std::fs::write(
        &real_config,
        "api_key = \"sk-o-symlink-target-should-not-be-read\"",
    )
    .unwrap();

    // Symlink at the expected config path (~/.config/openrouter-image/config.toml).
    let symlink_path = config_dir.join("config.toml");
    std::os::unix::fs::symlink(&real_config, &symlink_path).unwrap();

    let original_xdg = env::var("XDG_CONFIG_HOME").ok();
    let original_key = env::var("OPENROUTER_API_KEY").ok();
    env::set_var("XDG_CONFIG_HOME", tmp_dir.path());
    env::remove_var("OPENROUTER_API_KEY");

    let result = openrouter_image_core::Config::resolve();

    if let Some(v) = original_xdg {
        env::set_var("XDG_CONFIG_HOME", v);
    } else {
        env::remove_var("XDG_CONFIG_HOME");
    }
    if let Some(v) = original_key {
        env::set_var("OPENROUTER_API_KEY", v);
    }

    let err = result.expect_err("symlink config must be rejected");
    match &err {
        ConfigError::ConfigIsSymlink => {}
        other => panic!("expected ConfigIsSymlink, got: {:?}", other),
    }
    assert_eq!(err.exit_code(), 3, "ConfigIsSymlink must exit with code 3");
}
