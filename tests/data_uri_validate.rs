//! Integration test: reference image validation for --image-ref.
//!
//! Verifies: valid data URIs and HTTP(S) URLs pass; malformed inputs return ReferenceError.

#[test]
fn test_valid_data_uri_cases() {
    let cases = &[
        "data:image/png;base64,SGVsbG8=",
        "data:image/jpeg;base64,/9j/4AAQ",
        "data:image/webp;base64,UklGRlY=",
        "data:image/gif;base64,R0lGODlh",
        "data:image/png;base64,aGVsbG8gd29ybGQgaGVsbG8gd29ybGQ=",
    ];

    for uri in cases {
        let result = openrouter_image_core::validate_reference(uri);
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
fn test_valid_http_https_url_cases() {
    // HTTP(S) URLs are now accepted per OpenRouter spec — but only public hosts.
    // Private/loopback/link-local IPs are rejected by the SSRF guard (F10).
    // We use example.com's actual A record (93.184.216.34) to avoid DNS in tests.
    let cases = &[
        "https://93.184.216.34/images/photo.png",
        "http://93.184.216.34/images/photo.png",
        "https://93.184.216.34:8080/images/photo.png",
        "https://93.184.216.34/image.jpg?w=512&h=512",
    ];

    for url in cases {
        let result = openrouter_image_core::validate_reference(url);
        assert!(
            result.is_ok(),
            "expected {:?} to be accepted, got {:?}",
            url,
            result
        );
    }
}

#[test]
fn test_invalid_cases() {
    // (input, expected error variant)
    let cases: &[(&str, &str)] = &[
        // file paths are not accepted
        ("/home/user/image.png", "NotADataUri"),
        ("/tmp/photo.jpg", "NotADataUri"),
        // malformed data URIs
        ("data:image/png;binary,SGVsbG8=", "NotBase64"),
        ("data:image/png", "Malformed"),
        ("data:", "Malformed"),
        ("not-a-data-uri", "NotADataUri"),
        ("datapng;base64,SGVsbG8=", "NotADataUri"),
        // malformed HTTP(S) URLs (empty host)
        ("http://", "Malformed"),
        ("https://", "Malformed"),
        ("http:///path", "Malformed"),
        ("http://:8080/path", "Malformed"),
    ];

    for (input, expected_variant) in cases {
        let result = openrouter_image_core::validate_reference(input);
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
