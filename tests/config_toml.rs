//! Integration test: TOML config file is read correctly.

use std::env;
use tempfile::TempDir;

/// Verifies:
/// - TOML file at expected path is read
/// - api_key is extracted correctly
#[tokio::test]
async fn test_config_toml_read() {
    // Create a temp dir and write a TOML config
    let tmp_dir = TempDir::new().unwrap();
    let config_dir = tmp_dir.path().join("openrouter-image");
    std::fs::create_dir_all(&config_dir).unwrap();
    let config_path = config_dir.join("config.toml");

    std::fs::write(
        &config_path,
        r#"
api_key = "sk-or-toml-test-key-123"
default_model = "openai/gpt-image-2"
"#,
    )
    .unwrap();

    // Set mode 0o600 — the resolver rejects world/group-readable configs (F1).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    // Patch the config path lookup by setting HOME to our temp dir
    // Note: dirs::config_dir() uses $XDG_CONFIG_HOME or $HOME/.config
    // We set XDG_CONFIG_HOME to the temp dir
    let original_xdg = env::var("XDG_CONFIG_HOME").ok();
    let original_home = env::var("HOME").ok();

    env::set_var("XDG_CONFIG_HOME", tmp_dir.path());

    // Unset OPENROUTER_API_KEY to force TOML lookup
    let original_api_key = env::var("OPENROUTER_API_KEY").ok();
    env::remove_var("OPENROUTER_API_KEY");

    let config = openrouter_image_core::Config::resolve();

    // Restore env
    if let Some(v) = original_xdg {
        env::set_var("XDG_CONFIG_HOME", v);
    } else {
        env::remove_var("XDG_CONFIG_HOME");
    }
    if let Some(v) = original_home {
        env::set_var("HOME", v);
    }
    if let Some(v) = original_api_key {
        env::set_var("OPENROUTER_API_KEY", v);
    }

    let config = config.expect("Config::resolve() should succeed with TOML file");
    assert_eq!(
        config.api_key(),
        Some("sk-or-toml-test-key-123"),
        "api_key should be read from TOML"
    );
    assert_eq!(
        config.config_file_path(),
        config_path,
        "config_file_path should point to the TOML file"
    );
    assert!(
        config.config_file_exists,
        "config_file_exists should be true"
    );
}
