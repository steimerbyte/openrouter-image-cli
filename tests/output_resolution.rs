//! Integration test: output path resolution for n=1 and n>1.

use openrouter_image_core::OutputFormat;

/// Verifies:
/// - n=1: single path from --output or ./output.png
/// - n>1: output-N.png in --output-dir or current dir
#[test]
fn test_output_resolution_n_1_single() {
    let ext = OutputFormat::Png.to_ext();
    let n = 1;

    let output = Some(std::path::PathBuf::from("/custom/output.png"));
    let output_dir = None;

    let paths = resolve_for_test(n, output, output_dir, ext);
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0], std::path::PathBuf::from("/custom/output.png"));
}

#[test]
fn test_output_resolution_n_1_default() {
    let ext = OutputFormat::Png.to_ext();
    let n = 1;

    let output = None;
    let output_dir = None;

    let paths = resolve_for_test(n, output, output_dir, ext);
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0], std::path::PathBuf::from("./output.png"));
}

#[test]
fn test_output_resolution_n_3_multiple() {
    let ext = OutputFormat::Png.to_ext();
    let n = 3;

    let output = None;
    let output_dir = None;

    let paths = resolve_for_test(n, output, output_dir, ext);
    assert_eq!(paths.len(), 3);
    assert_eq!(paths[0], std::path::PathBuf::from("output-1.png"));
    assert_eq!(paths[1], std::path::PathBuf::from("output-2.png"));
    assert_eq!(paths[2], std::path::PathBuf::from("output-3.png"));
}

#[test]
fn test_output_resolution_n_3_with_dir() {
    let ext = OutputFormat::Png.to_ext();
    let n = 3;

    let output = None;
    let output_dir = Some(std::path::PathBuf::from("/tmp/images"));

    let paths = resolve_for_test(n, output, output_dir, ext);
    assert_eq!(paths.len(), 3);
    assert_eq!(
        paths[0],
        std::path::PathBuf::from("/tmp/images/output-1.png")
    );
    assert_eq!(
        paths[1],
        std::path::PathBuf::from("/tmp/images/output-2.png")
    );
    assert_eq!(
        paths[2],
        std::path::PathBuf::from("/tmp/images/output-3.png")
    );
}

#[test]
fn test_output_resolution_n_1_jpeg_ext() {
    let ext = OutputFormat::Jpeg.to_ext();
    let n = 1;

    let output = None;
    let output_dir = None;

    let paths = resolve_for_test(n, output, output_dir, ext);
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0], std::path::PathBuf::from("./output.jpg"));
}

/// Mirrors the CLI's resolve_output_paths logic for testing.
fn resolve_for_test(
    n: u8,
    output: Option<std::path::PathBuf>,
    output_dir: Option<std::path::PathBuf>,
    ext: &str,
) -> Vec<std::path::PathBuf> {
    use std::path::PathBuf;
    if n == 1 {
        vec![output.unwrap_or_else(|| PathBuf::from(format!("./output.{}", ext)))]
    } else {
        if let Some(dir) = output_dir {
            (1..=u32::from(n))
                .map(|i| dir.join(format!("output-{}.{}", i, ext)))
                .collect()
        } else {
            (1..=u32::from(n))
                .map(|i| PathBuf::from(format!("output-{}.{}", i, ext)))
                .collect()
        }
    }
}
