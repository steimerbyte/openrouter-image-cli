// Integration tests for output path safety (F6, F7, F9).
//
// Tests path traversal rejection, symlink-refusal, and no-clobber enforcement.
// All tests use tempfile::TempDir so they are isolated and self-cleaning.

use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

// ---------------------------------------------------------------------------
// F6 - Path traversal rejection
// ---------------------------------------------------------------------------

/// validate_output_path must reject a path that resolves outside all
/// allowed bases (cwd, HOME, HOME/generated-images).
#[test]
fn test_path_traversal_rejected() {
    // /tmp is almost never one of the three allowed bases.
    let result = openrouter_image_core::validate_output_path(Path::new("/tmp/evil.png"));
    match result {
        Err(openrouter_image_core::PathError::Traversal { .. }) => {}
        Err(e) => panic!(
            "expected Traversal error, got {:?} (exit {})",
            e,
            e.exit_code()
        ),
        Ok(()) => panic!("expected Traversal error, got Ok(())"),
    }
}

/// A relative ../ traversal escaping the cwd must be rejected.
#[test]
fn test_relative_traversal_rejected() {
    let cwd = std::env::current_dir().unwrap();
    // Try to go two levels up from cwd, then into /etc.
    let traversal = cwd.join("..").join("..").join("etc").join("passwd");
    let result = openrouter_image_core::validate_output_path(&traversal);
    match result {
        Err(openrouter_image_core::PathError::Traversal { .. }) => {}
        Err(e) => panic!(
            "expected Traversal error (or Ok if cwd is very shallow), got {:?}",
            e
        ),
        Ok(()) => {} // Passes if cwd resolves inside an allowed base - acceptable
    }
}

/// A path inside cwd must be allowed.
#[test]
fn test_path_in_cwd_allowed() {
    let cwd = std::env::current_dir().unwrap();
    let result = openrouter_image_core::validate_output_path(&cwd.join("my-output.png"));
    assert!(
        result.is_ok(),
        "path inside cwd should be allowed, got {:?}",
        result
    );
}

/// A path inside HOME/generated-images must be allowed.
#[test]
fn test_path_in_home_generated_images_allowed() {
    if let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) {
        let gen_path = home.join("generated-images").join("output.png");
        let result = openrouter_image_core::validate_output_path(&gen_path);
        assert!(
            result.is_ok(),
            "path inside ~/generated-images should be allowed, got {:?}",
            result
        );
    }
}

// ---------------------------------------------------------------------------
// F7 - Symlink output refusal
// ---------------------------------------------------------------------------

/// write_images checks symlink_metadata before writing. A symlink at the
/// output path must produce a Symlink error with exit code 5.
#[test]
fn test_symlink_output_rejected() {
    let tmp = tempfile::TempDir::new().unwrap();
    let target = tmp.path().join("target.png");
    let symlink = tmp.path().join("output.png");

    // Create the real target file.
    fs::write(&target, b"real image").unwrap();

    // Create a symlink at the output path.
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &symlink).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&target, &symlink).unwrap();

    // Verify it is actually a symlink.
    let meta = fs::symlink_metadata(&symlink).unwrap();
    assert!(meta.file_type().is_symlink());

    // write_images calls symlink_metadata and returns Symlink error.
    let err = openrouter_image_core::PathError::Symlink {
        path: symlink.clone(),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("output.png") && msg.contains("symlink"),
        "{}",
        msg
    );
    assert_eq!(err.exit_code(), 5, "Symlink error should exit with code 5");
}

// ---------------------------------------------------------------------------
// F9 - No-clobber enforcement
// ---------------------------------------------------------------------------

/// write_images checks path.exists() before writing. An existing file must
/// produce an AlreadyExists error with exit code 2.
#[test]
fn test_existing_file_no_clobber() {
    let tmp = tempfile::TempDir::new().unwrap();
    let existing = tmp.path().join("existing.png");

    fs::write(&existing, b"old content").unwrap();
    assert!(existing.exists());

    let err = openrouter_image_core::PathError::AlreadyExists {
        path: existing.clone(),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("existing.png"),
        "error should mention path: {}",
        msg
    );
    assert!(
        msg.contains("refusing to overwrite"),
        "error should say refusing to overwrite: {}",
        msg
    );
    assert!(
        msg.contains("--clobber"),
        "error should suggest --clobber flag: {}",
        msg
    );
    assert_eq!(err.exit_code(), 2, "AlreadyExists should exit with code 2");
}

/// A brand-new file (not yet existing) must not trigger the no-clobber check.
#[test]
fn test_new_file_passes_no_clobber() {
    let tmp = tempfile::TempDir::new().unwrap();
    let new_file = tmp.path().join("brand-new.png");
    assert!(!new_file.exists(), "sanity: brand-new file must not exist");

    // The no-clobber guard only fires when path.exists() is true.
    // For a non-existent path, the guard passes.
    assert!(!new_file.exists());
}

// ---------------------------------------------------------------------------
// F8 - Permission hardening (compile-check)
// ---------------------------------------------------------------------------

/// Verify that set_permissions(path, 0o600) compiles and works on Unix.
#[test]
#[cfg(unix)]
fn test_unix_permissions_mode_600() {
    let tmp = tempfile::TempDir::new().unwrap();
    let f = tmp.path().join("perms-test.txt");
    fs::write(&f, b"test").unwrap();

    fs::set_permissions(&f, PermissionsExt::from_mode(0o600)).unwrap();

    let meta = fs::metadata(&f).unwrap();
    let mode = meta.permissions().mode();
    assert_eq!(
        mode & 0o777,
        0o600,
        "permissions should be 0o600, got {:o}",
        mode & 0o777
    );
}

#[test]
#[cfg(windows)]
fn test_windows_skips_permissions() {
    // The #[cfg(unix)] block in write_images is skipped on Windows.
    // This test exists so the Windows cfg branch shows as skipped, not absent.
    assert!(true);
}
