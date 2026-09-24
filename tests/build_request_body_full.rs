//! Unit test: build_request_body serialises all fields correctly.
//!
//! Covers every new field from the audit:
//!   aspect_ratio, background, output_format, output_compression,
//!   quality, seed, size, user, session_id, provider, trace, stream

use openrouter_image_core::GenerationParams;

fn full_params() -> GenerationParams {
    use std::collections::HashMap;
    GenerationParams {
        prompt: "a majestic mountain at sunset".to_string(),
        model: "openai/dall-e-3".to_string(),
        image_refs: vec!["https://example.com/ref.jpg".to_string()],
        output_paths: vec![std::path::PathBuf::from("/tmp/out.png")],
        n: 2,
        resolution: Some("4K".to_string()),
        aspect_ratio: Some("16:9".to_string()),
        background: Some("transparent".to_string()),
        output_format: Some("webp".to_string()),
        output_compression: Some(75),
        quality: Some("high".to_string()),
        seed: Some(1234567890i64),
        size: Some("2048x2048".to_string()),
        user: Some("user_abc123".to_string()),
        session_id: Some("sess_def456".to_string()),
        provider: Some(openrouter_image_core::ProviderRouting {
            allow_fallbacks: Some(false),
            only: Some(vec!["google".to_string(), "anthropic".to_string()]),
            ignore: Some(vec!["openai".to_string()]),
            order: None,
            sort: Some("latency".to_string()),
        }),
        trace: Some(openrouter_image_core::TraceMetadata {
            trace_id: Some("trace-abc-123".to_string()),
            trace_name: Some("batch-generation".to_string()),
            span_name: Some("image-generation-span".to_string()),
            generation_name: None,
            parent_span_id: Some("span-parent-xyz".to_string()),
            extra: HashMap::new(),
        }),
        stream: true,
        timeout_ms: 120_000,
    }
}

fn minimal_params() -> GenerationParams {
    GenerationParams {
        prompt: "minimal".to_string(),
        model: "test/model".to_string(),
        image_refs: vec![],
        output_paths: vec![],
        n: 1,
        resolution: None,
        aspect_ratio: None,
        background: None,
        output_format: None,
        output_compression: None,
        quality: None,
        seed: None,
        size: None,
        user: None,
        session_id: None,
        provider: None,
        trace: None,
        stream: false,
        timeout_ms: 120_000,
    }
}

#[test]
fn test_all_fields_present() {
    let params = full_params();
    let body = params.to_request_body();

    // Core fields always present
    assert_eq!(body["model"], "openai/dall-e-3");
    assert_eq!(body["prompt"], "a majestic mountain at sunset");
    assert_eq!(body["n"], 2);
    assert_eq!(body["resolution"], "4K");

    // New fields
    assert_eq!(body["aspect_ratio"], "16:9");
    assert_eq!(body["background"], "transparent");
    assert_eq!(body["output_format"], "webp");
    assert_eq!(body["output_compression"], 75);
    assert_eq!(body["quality"], "high");
    assert_eq!(body["seed"], 1234567890i64);
    assert_eq!(body["size"], "2048x2048");
    assert_eq!(body["user"], "user_abc123");
    assert_eq!(body["session_id"], "sess_def456");
    assert_eq!(body["stream"], true);

    // Provider routing
    let prov = &body["provider"];
    assert_eq!(prov["allow_fallbacks"], false);
    assert_eq!(prov["only"][0], "google");
    assert_eq!(prov["only"][1], "anthropic");
    assert_eq!(prov["ignore"][0], "openai");
    assert_eq!(prov["sort"], "latency");
    assert!(prov.get("order").is_none());

    // Trace metadata
    let trace = &body["trace"];
    assert_eq!(trace["trace_id"], "trace-abc-123");
    assert_eq!(trace["trace_name"], "batch-generation");
    assert_eq!(trace["span_name"], "image-generation-span");
    assert_eq!(trace["parent_span_id"], "span-parent-xyz");
    assert!(trace.get("generation_name").is_none());

    // Reference images
    let refs = body["input_references"].as_array().unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0]["type"], "image_url");
    assert_eq!(refs[0]["image_url"]["url"], "https://example.com/ref.jpg");
}

#[test]
fn test_optional_fields_omitted_when_none() {
    let params = minimal_params();
    let body = params.to_request_body();

    assert!(body.get("aspect_ratio").is_none());
    assert!(body.get("background").is_none());
    assert!(body.get("output_format").is_none());
    assert!(body.get("output_compression").is_none());
    assert!(body.get("quality").is_none());
    assert!(body.get("seed").is_none());
    assert!(body.get("size").is_none());
    assert!(body.get("user").is_none());
    assert!(body.get("session_id").is_none());
    assert!(body.get("provider").is_none());
    assert!(body.get("trace").is_none());
    assert!(body.get("stream").is_none()); // false → omitted
    assert!(body.get("input_references").is_none());
    assert!(body.get("resolution").is_none()); // None → omitted
}

#[test]
fn test_n_omitted_when_1() {
    let params = minimal_params();
    let body = params.to_request_body();
    assert!(body.get("n").is_none());
}

#[test]
fn test_n_included_when_gt_1() {
    let mut params = minimal_params();
    params.n = 4;
    let body = params.to_request_body();
    assert_eq!(body["n"], 4);
}

#[test]
fn test_output_format_all_variants() {
    // output_format in GenerationParams is Option<String>
    for (fmt, expected) in [
        ("png", "png"),
        ("jpeg", "jpeg"),
        ("webp", "webp"),
        ("svg", "svg"),
    ] {
        let mut params = minimal_params();
        params.output_format = Some(fmt.to_string());
        let body = params.to_request_body();
        assert_eq!(body["output_format"], expected);
    }
}

#[test]
fn test_trace_extra_fields() {
    use std::collections::HashMap;
    let mut extra = HashMap::new();
    extra.insert("custom_key".to_string(), serde_json::json!("custom_value"));

    let params = GenerationParams {
        trace: Some(openrouter_image_core::TraceMetadata {
            trace_id: Some("t".to_string()),
            trace_name: None,
            span_name: None,
            generation_name: None,
            parent_span_id: None,
            extra,
        }),
        ..minimal_params()
    };

    let body = params.to_request_body();
    let trace = &body["trace"];
    assert_eq!(trace["trace_id"], "t");
    assert_eq!(trace["custom_key"], "custom_value");
}
