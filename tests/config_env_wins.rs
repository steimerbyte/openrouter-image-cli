//! Integration test: OPENROUTER_API_KEY env var takes precedence over TOML.

use std::env;
use tempfile::TempDir;

/// Verifies: Env var overrides TOML file.
#[tokio::test]
async fn test_config_env_wins_over_toml() {
    let tmp_dir = TempDir::new().unwrap();
    let config_dir = tmp_dir.path().join("openrouter-image");
    std::fs::create_dir_all(&config_dir).unwrap();
    let config_path = config_dir.join("config.toml");

    // TOML contains a different key
    std::fs::write(&config_path, "api_key = \"sk-or-from-toml\"").unwrap();

    let original_xdg = env::var("XDG_CONFIG_HOME").ok();
    let original_api_key = env::var("OPENROUTER_API_KEY").ok();

    env::set_var("XDG_CONFIG_HOME", tmp_dir.path());
    env::set_var("OPENROUTER_API_KEY", "sk-or-from-env-override");

    let config = openrouter_image_core::Config::resolve();

    if let Some(v) = original_xdg {
        env::set_var("XDG_CONFIG_HOME", v);
    } else {
        env::remove_var("XDG_CONFIG_HOME");
    }
    if let Some(v) = original_api_key {
        env::set_var("OPENROUTER_API_KEY", v);
    } else {
        env::remove_var("OPENROUTER_API_KEY");
    }

    let config = config.expect("Config::resolve() should succeed");
    assert_eq!(
        config.api_key(),
        Some("sk-or-from-env-override"),
        "env var should take precedence over TOML"
    );
}
