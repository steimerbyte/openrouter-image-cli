//! Integration test: list_models filter correctness.
//!
//! Mocks the dedicated `/api/v1/images/models` endpoint. The new shape uses
//! `supported_parameters` as a map keyed by parameter name, with values
//! being `{type, values, min, max}` objects.

use std::collections::BTreeMap;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn param_enum(values: &[&str]) -> openrouter_image_core::ParamSpec {
    openrouter_image_core::ParamSpec {
        kind: "enum".to_string(),
        values: values.iter().map(|s| s.to_string()).collect(),
        min: None,
        max: None,
    }
}

fn make_entry_json(
    id: &str,
    output_modalities: &[&str],
    sp: BTreeMap<String, openrouter_image_core::ParamSpec>,
) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "name": id,
        "architecture": {
            "input_modalities": ["text"],
            "output_modalities": output_modalities
        },
        "supported_parameters": sp.into_iter().map(|(k, v)| {
            (k, serde_json::json!({"type": v.kind, "values": v.values}))
        }).collect::<serde_json::Map<_, _>>()
    })
}

#[tokio::test]
async fn test_list_models_filter() {
    let mock_server = MockServer::start().await;

    // Image model with full resolution support
    let mut seedream_sp = BTreeMap::new();
    seedream_sp.insert("resolution".to_string(), param_enum(&["1K", "2K", "4K"]));
    let seedream = make_entry_json("bytedance-seed/seedream-4.5", &["image"], seedream_sp);

    // Image model without resolution (only output_format and n)
    let mut ming_sp = BTreeMap::new();
    ming_sp.insert("output_format".to_string(), param_enum(&["png", "webp"]));
    let ming = make_entry_json("inclusionai/ming-image-0.1", &["image"], ming_sp);

    // Image model with 1K only
    let mut krea_sp = BTreeMap::new();
    krea_sp.insert("resolution".to_string(), param_enum(&["1K"]));
    let krea = make_entry_json("krea/krea-2-large", &["image"], krea_sp);

    let body = serde_json::json!({
        "data": [seedream, ming, krea]
    });

    Mock::given(method("GET"))
        .and(path("/api/v1/images/models"))
        .and(header("Authorization", "Bearer sk-test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&mock_server)
        .await;

    let client = reqwest::Client::new();
    let models = openrouter_image_core::fetch_image_models(
        &client,
        "sk-test",
        Some(&format!("{}/api/v1/images/models", mock_server.uri())),
    )
    .await
    .expect("fetch should succeed");

    let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids.len(), 3, "expected 3 image models");
    assert!(ids.contains(&"bytedance-seed/seedream-4.5"));
    assert!(ids.contains(&"inclusionai/ming-image-0.1"));
    assert!(ids.contains(&"krea/krea-2-large"));

    // Resolution enum decoding
    let seedream_entry = models
        .iter()
        .find(|m| m.id == "bytedance-seed/seedream-4.5")
        .unwrap();
    assert!(openrouter_image_core::supports_resolution(seedream_entry));
    assert_eq!(
        openrouter_image_core::resolution_values(seedream_entry),
        Some(vec!["1K".to_string(), "2K".to_string(), "4K".to_string()])
    );

    let ming_entry = models
        .iter()
        .find(|m| m.id == "inclusionai/ming-image-0.1")
        .unwrap();
    assert!(!openrouter_image_core::supports_resolution(ming_entry));
    assert_eq!(openrouter_image_core::resolution_values(ming_entry), None);

    let krea_entry = models.iter().find(|m| m.id == "krea/krea-2-large").unwrap();
    assert!(openrouter_image_core::supports_resolution(krea_entry));
    assert_eq!(
        openrouter_image_core::resolution_values(krea_entry),
        Some(vec!["1K".to_string()])
    );
}
