//! Resolve a `--reference` argument to a data URL or pass-through HTTPS URLs.
//!
//! Supports three forms:
//!   - `data:image/…;base64,…` → passed through as-is
//!   - `https://…`            → passed through as-is
//!   - `/path/to/file`        → read, detect MIME, base64-encode

use std::path::Path;

use anyhow::Context;
use base64::Engine;

/// Resolve a reference string to a data URL for the OpenRouter API.
pub fn resolve_reference(input: &str) -> anyhow::Result<String> {
    let input = input.trim();

    // Already a data URL or https URL — pass through
    if input.starts_with("data:") || input.starts_with("https://") || input.starts_with("http://") {
        return Ok(input.to_string());
    }

    // Local file path
    let path = Path::new(input);
    if !path.exists() {
        anyhow::bail!("reference file not found: {}", input);
    }

    let data = std::fs::read(path).with_context(|| format!("failed to read `{}`", input))?;

    let mime = mime_by_ext(path);
    let b64 = base64::engine::general_purpose::STANDARD.encode(&data);

    Ok(format!("data:{};base64,{}", mime, b64))
}

/// Detect MIME type from file extension.
fn mime_by_ext(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    match ext.as_deref() {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("bmp") => "image/bmp",
        Some("ico") => "image/x-icon",
        _ => "image/png",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_data_url_pass_through() {
        let url = "data:image/png;base64,SGVsbG8=";
        assert_eq!(resolve_reference(url).unwrap(), url);
    }

    #[test]
    fn test_https_pass_through() {
        let url = "https://example.com/image.png";
        assert_eq!(resolve_reference(url).unwrap(), url);
    }

    #[test]
    fn test_local_file() {
        let mut f = NamedTempFile::with_suffix(".png").unwrap();
        f.write_all(b"fake png data").unwrap();
        let path = f.path().to_str().unwrap();
        let result = resolve_reference(path).unwrap();
        assert!(result.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn test_missing_file() {
        let result = resolve_reference("/nonexistent/path/to/file.png");
        assert!(result.is_err());
    }

    #[test]
    fn test_mime_detection() {
        let mut f = NamedTempFile::with_suffix(".jpeg").unwrap();
        f.write_all(b"fake").unwrap();
        let result = resolve_reference(f.path().to_str().unwrap()).unwrap();
        assert!(result.starts_with("data:image/jpeg;base64,"));
    }
}
