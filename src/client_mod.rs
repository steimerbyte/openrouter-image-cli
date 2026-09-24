//! HTTP client for OpenRouter /images endpoint.
//!
//! Retry policy: exactly 1× retry on 5xx errors with 2-second sleep.
//! No retry on 4xx (including 429 — treated as API error, exit 4).

use std::time::{Duration, Instant};

use reqwest::Client as ReqwestClient;
use tokio::time::sleep;

use crate::config_mod::Config;
use crate::error_mod::ApiError;
use crate::models_mod::{ApiResponse, GenerationParams};
use crate::progress_mod::ProgressEvent;
use crate::reference_mod::validate_data_uri;
use anyhow::Context;

/// Returns the base URL for OpenRouter images API.
/// Override with `OPENROUTER_BASE_URL` env var (for integration tests).
fn images_url() -> String {
    std::env::var("OPENROUTER_BASE_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://openrouter.ai/api/v1/images".to_string())
}

const BACKOFF_DELAY: Duration = Duration::from_secs(2);

pub struct HttpClient {
    http: ReqwestClient,
    /// Full endpoint URL for the images API (e.g. `https://openrouter.ai/api/v1/images`).
    /// Override via `new_with_url()` for integration testing — pass the full endpoint including path.
    base_url: String,
}

impl HttpClient {
    pub fn new(timeout_ms: u64) -> anyhow::Result<Self> {
        Self::new_with_url(timeout_ms, images_url())
    }

    /// Create an HttpClient with a custom images endpoint URL (for integration testing).
    /// `endpoint` should be the full URL including `/api/v1/images` path.
    pub fn new_with_url(timeout_ms: u64, endpoint: String) -> anyhow::Result<Self> {
        let http = ReqwestClient::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .context("failed to build HTTP client")?;
        Ok(Self {
            http,
            base_url: endpoint,
        })
    }

    /// Call the OpenRouter API with single-backoff retry on 5xx.
    #[allow(dead_code)]
    pub async fn call(
        &self,
        params: &GenerationParams,
        config: &Config,
    ) -> Result<(ApiResponse, Duration), ApiError> {
        self.call_with_progress(params, config, |_event| {}).await
    }

    /// Call the API with progress events and single-backoff retry.
    pub async fn call_with_progress<F>(
        &self,
        params: &GenerationParams,
        config: &Config,
        mut on_progress: F,
    ) -> Result<(ApiResponse, Duration), ApiError>
    where
        F: FnMut(ProgressEvent),
    {
        let api_key = config.api_key().ok_or(ApiError::NoApiKey)?;

        // Validate all image refs upfront
        let ref_urls: Result<Vec<_>, _> = params
            .image_refs
            .iter()
            .map(|r| validate_data_uri(r).map(|_| r.as_str()))
            .collect();

        let ref_urls = ref_urls.map_err(|e| ApiError::Http {
            status: 400,
            message: e.to_string(),
        })?;

        let body = build_request_body(params, &ref_urls);
        let start = Instant::now();

        let result = self.do_request(api_key, &body, &mut on_progress).await;

        match result {
            Ok(response) => {
                on_progress(ProgressEvent::HttpStatus { status: 200 });
                Ok((response, start.elapsed()))
            }
            Err(err) => {
                // If it's a server error, retry once with backoff
                let is_server_error = matches!(
                    err,
                    ApiError::Http { status, .. } if (500..=599).contains(&(status as u32))
                );

                if is_server_error {
                    on_progress(ProgressEvent::RetryAttempt {
                        attempt: 1,
                        delay_ms: BACKOFF_DELAY.as_millis() as u64,
                    });
                    sleep(BACKOFF_DELAY).await;

                    let retry_result = self.do_request(api_key, &body, &mut on_progress).await;
                    match retry_result {
                        Ok(response) => {
                            on_progress(ProgressEvent::HttpStatus { status: 200 });
                            return Ok((response, start.elapsed()));
                        }
                        Err(retry_err) => return Err(retry_err),
                    }
                }

                Err(err)
            }
        }
    }

    async fn do_request<F>(
        &self,
        api_key: &str,
        body: &serde_json::Value,
        on_progress: &mut F,
    ) -> Result<ApiResponse, ApiError>
    where
        F: FnMut(ProgressEvent),
    {
        let mut req = self.http.post(&self.base_url);

        req = req
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json");

        if let Ok(referer) = std::env::var("OPENROUTER_HTTP_REFERER") {
            if !referer.is_empty() {
                req = req.header("HTTP-Referer", referer);
            }
        }
        if let Ok(title) = std::env::var("OPENROUTER_X_TITLE") {
            if !title.is_empty() {
                req = req.header("X-Title", title);
            }
        }

        let resp = req
            .body(serde_json::to_string(body).unwrap())
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ApiError::Timeout(Duration::from_millis(120_000))
                } else {
                    ApiError::Network(e)
                }
            })?;

        let status_code = resp.status();
        let status = status_code.as_u16();
        let body_text = resp.text().await.unwrap_or_default();

        if !status_code.is_success() {
            let err = ApiError::from_status(status, &body_text);
            on_progress(ProgressEvent::ApiError {
                status,
                message: err.to_string(),
            });
            return Err(err);
        }

        serde_json::from_str::<ApiResponse>(&body_text).map_err(|e| ApiError::Http {
            status: 500,
            message: format!("failed to parse API response: {}", e),
        })
    }
}

/// Build the request body from GenerationParams.
fn build_request_body(params: &GenerationParams, ref_urls: &[&str]) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": params.model,
        "prompt": params.prompt,
    });

    if params.n > 1 {
        body["n"] = serde_json::json!(params.n);
    }

    if !ref_urls.is_empty() {
        let refs: Vec<serde_json::Value> = ref_urls
            .iter()
            .map(|uri| {
                serde_json::json!({
                    "type": "image_url",
                    "image_url": { "url": uri }
                })
            })
            .collect();
        body["input_references"] = serde_json::json!(refs);
    }

    if let Some(res) = &params.resolution {
        body["resolution"] = serde_json::json!(res);
    }

    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_mod::Config;
    use crate::models_mod::GenerationParams;

    #[allow(dead_code)]
    fn test_config() -> Config {
        Config {
            api_key: Some("sk-test-key".to_string()),
            default_model: None,
            config_file: std::path::PathBuf::from("/tmp/config.toml"),
            config_file_exists: false,
        }
    }

    fn test_params() -> GenerationParams {
        GenerationParams {
            prompt: "a blue circle".to_string(),
            model: "openai/gpt-image-2".to_string(),
            image_refs: vec![],
            output_paths: vec![std::path::PathBuf::from("./output.png")],
            n: 1,
            resolution: None,
            timeout_ms: 120_000,
        }
    }

    #[test]
    fn test_build_request_body_single() {
        let params = test_params();
        let body = build_request_body(&params, &[]);
        assert_eq!(body["model"], "openai/gpt-image-2");
        assert_eq!(body["prompt"], "a blue circle");
        assert!(body.get("input_references").is_none());
    }

    #[test]
    fn test_build_request_body_with_ref() {
        let params = test_params();
        let body = build_request_body(&params, &["data:image/png;base64,SGVsbG8="]);
        let refs = body["input_references"].as_array().unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(
            refs[0]["image_url"]["url"],
            "data:image/png;base64,SGVsbG8="
        );
    }

    #[test]
    fn test_build_request_body_n_gt_1() {
        let mut params = test_params();
        params.n = 3;
        let body = build_request_body(&params, &[]);
        assert_eq!(body["n"], 3);
    }

    #[test]
    fn test_build_request_body_with_resolution() {
        let mut params = test_params();
        params.resolution = Some("2K".to_string());
        let body = build_request_body(&params, &[]);
        assert_eq!(body["resolution"], "2K");
    }

    #[test]
    fn test_client_creation() {
        let client = HttpClient::new(120_000);
        assert!(client.is_ok());
    }
}
