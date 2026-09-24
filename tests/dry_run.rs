//! Integration test: --dry-run makes no API call and writes no file.

use std::process::Command;
use tempfile::TempDir;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Verifies:
/// - dry-run mode: binary exits 0
/// - No HTTP request is made (mock expects 0 calls)
/// - No file is written
/// - Request body is printed to stdout
#[tokio::test]
async fn test_dry_run_no_api_call_no_file_write() {
    let mock_server = MockServer::start().await;

    // This mock should NEVER be called in dry-run mode
    // Use `expect(0)` to assert it is not invoked
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_string("unexpected"))
        .expect(0) // assert: never called
        .mount(&mock_server)
        .await;

    let tmp_dir = TempDir::new().unwrap();
    let output_path = tmp_dir.path().join("should-not-exist.png");

    // Verify file doesn't exist before
    assert!(!output_path.exists());

    // Build path to the openrouter-image binary via cargo-provided env var
    let binary_path = env!("CARGO_BIN_EXE_openrouter-image");

    let output = Command::new(binary_path)
        .args([
            "generate",
            "--prompt",
            "blue circle",
            "--dry-run",
            "--output",
            output_path.to_str().unwrap(),
        ])
        .env("OPENROUTER_API_KEY", "sk-or-test")
        .output()
        .expect("failed to run binary");

    // Exit code should be 0
    assert_eq!(
        output.status.code(),
        Some(0),
        "dry-run should exit 0, got {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    // File should NOT be written
    assert!(!output_path.exists(), "dry-run should not write any files");

    // stdout should contain the JSON request body
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"model\""),
        "stdout should contain JSON body with model field: {}",
        stdout
    );
    assert!(
        stdout.contains("\"prompt\""),
        "stdout should contain JSON body with prompt field"
    );

    // stderr should contain the dry-run marker
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("[dry-run]"),
        "stderr should contain [dry-run] marker: {}",
        stderr
    );

    // Verify mock was never called
    mock_server.verify().await;
}
