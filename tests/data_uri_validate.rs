//! Integration test: data-URI validation for --image-ref.

/// Verifies: valid data URIs pass, invalid ones return ReferenceError.
#[test]
fn test_data_uri_validate_valid_cases() {
    let cases = &[
        "data:image/png;base64,SGVsbG8=",
        "data:image/jpeg;base64,/9j/4AAQ",
        "data:image/webp;base64,UklGRlY=",
        "data:image/gif;base64,R0lGODlh",
        "data:image/png;base64,aGVsbG8gd29ybGQgaGVsbG8gd29ybGQ=",
    ];

    for uri in cases {
        let result = openrouter_image_core::validate_data_uri(uri);
        assert!(
            result.is_ok(),
            "expected {:?} to be valid, got {:?}",
            uri,
            result
        );
        assert_eq!(result.unwrap(), *uri);
    }
}

#[test]
fn test_data_uri_validate_invalid_cases() {
    // (input, expected error variant)
    let cases: &[(&str, &str)] = &[
        ("https://example.com/image.png", "NotADataUri"),
        ("http://example.com/image.png", "NotADataUri"),
        ("/home/user/image.png", "NotADataUri"),
        ("data:image/png;binary,SGVsbG8=", "NotBase64"),
        ("data:image/png", "Malformed"),
        ("data:", "Malformed"),
        ("not-a-data-uri", "NotADataUri"),
    ];

    for (input, expected_variant) in cases {
        let result = openrouter_image_core::validate_data_uri(input);
        assert!(
            result.is_err(),
            "expected {:?} to be invalid, got {:?}",
            input,
            result
        );
        let err = result.unwrap_err();
        let err_debug = format!("{:?}", err);
        assert!(
            err_debug.contains(expected_variant),
            "expected error for {:?} to be variant {:?}, got {:?}",
            input,
            expected_variant,
            err_debug
        );
    }
}
