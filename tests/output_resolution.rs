//! Integration test: output path resolution for n=1 and n>1.
//!
//! Calls the real `openrouter_image_core::resolve_output_paths` (and
//! `default_output_dir`) so the CLI and the library stay in sync.
//!
//! Tests that rely on a specific HOME value set their own temp HOME via
//! `std::env::set_var("HOME", ...)` before invoking the library.



fn home() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
}

#[test]
fn test_default_output_dir_is_home_generated_images() {
    let p = openrouter_image_core::default_output_dir();
    assert_eq!(p, home().join("generated-images"));
}

#[test]
fn test_output_resolution_n_1_single_explicit() {
    let paths = openrouter_image_core::resolve_output_paths(
        1,
        Some(std::path::PathBuf::from("/custom/output.png")),
        None,
        "png",
    );
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0], std::path::PathBuf::from("/custom/output.png"));
}

#[test]
fn test_output_resolution_n_1_default() {
    let paths = openrouter_image_core::resolve_output_paths(1, None, None, "png");
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0], home().join("generated-images").join("output.png"));
}

#[test]
fn test_output_resolution_n_3_multiple_default() {
    let paths = openrouter_image_core::resolve_output_paths(3, None, None, "png");
    assert_eq!(paths.len(), 3);
    let base = home().join("generated-images");
    assert_eq!(paths[0], base.join("output-1.png"));
    assert_eq!(paths[1], base.join("output-2.png"));
    assert_eq!(paths[2], base.join("output-3.png"));
}

#[test]
fn test_output_resolution_n_3_with_dir() {
    let paths = openrouter_image_core::resolve_output_paths(
        3,
        None,
        Some(std::path::PathBuf::from("/tmp/images")),
        "png",
    );
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
    let paths = openrouter_image_core::resolve_output_paths(1, None, None, "jpg");
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0], home().join("generated-images").join("output.jpg"));
}

#[test]
fn test_output_resolution_n_3_svg_ext() {
    let paths = openrouter_image_core::resolve_output_paths(3, None, None, "svg");
    assert_eq!(paths.len(), 3);
    let base = home().join("generated-images");
    assert_eq!(paths[0], base.join("output-1.svg"));
    assert_eq!(paths[2], base.join("output-3.svg"));
}
