//! HTTP client for OpenRouter /images endpoint.
//!
//! Retry policy: exactly 1× retry on 5xx errors with 2-second sleep.
//! No retry on 4xx (including 429 — treated as API error, exit 4).
//!
//! Streaming: when `params.stream = true`, sends `Accept: text/event-stream`
//! and parses SSE events (`partial_image`, `completed`, `error`).

use std::time::{Duration, Instant};

use anyhow::Context;
use reqwest::Client as ReqwestClient;
use tokio::time::sleep;

use crate::config_mod::Config;
use crate::error_mod::ApiError;
use crate::models_mod::{self, ApiResponse, GenerationParams};
use crate::progress_mod::ProgressEvent;
use crate::reference_mod::validate_reference;

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
    /// Override via `new_with_url()` for integration testing.
    base_url: String,
}

impl HttpClient {
    /// Construct a new HttpClient with the default images API endpoint.
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
        let api_key = config.api_key.clone().ok_or(ApiError::NoApiKey)?;

        // Validate all image refs upfront (accepts data: and http(s):// URLs)
        let ref_urls: Result<Vec<_>, _> = params
            .image_refs
            .iter()
            .map(|r| validate_reference(r).map(|_| r.as_str()))
            .collect();

        let ref_urls = ref_urls.map_err(|e| ApiError::Http {
            status: 400,
            message: e.to_string(),
        })?;

        let body = build_request_body(params, &ref_urls);
        let start = Instant::now();

        let result = self
            .do_request(api_key.clone(), &body, params.stream, &mut on_progress)
            .await;

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

                    let retry_result = self
                        .do_request(api_key, &body, params.stream, &mut on_progress)
                        .await;
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

    /// Perform the HTTP request. Handles both buffered and SSE streaming modes.
    async fn do_request<F>(
        &self,
        api_key: String,
        body: &serde_json::Value,
        stream: bool,
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

        if stream {
            req = req.header("Accept", "text/event-stream");
        }

        if let Ok(referer) = std::env::var("OPENROUTER_HTTP_REFERER") {
            if !referer.is_empty() {
                // Spec: HTTP-Referer (hyphen) — OpenRouter tolerates both spellings
                req = req.header("HTTP-Referer", referer);
            }
        }
        if let Ok(title) = std::env::var("OPENROUTER_X_TITLE") {
            if !title.is_empty() {
                req = req.header("X-Title", title);
            }
        }
        // X-Session-Id header: sent when session_id is present in the body
        if let Some(sid) = body.get("session_id").and_then(|v| v.as_str()) {
            req = req.header("X-Session-Id", sid);
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

        if !status_code.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            let err = ApiError::from_status(status, &body_text);
            on_progress(ProgressEvent::ApiError {
                status,
                message: err.to_string(),
            });
            return Err(err);
        }

        if stream {
            self.parse_sse_stream(resp, on_progress).await
        } else {
            let body_text = resp.text().await.unwrap_or_default();
            serde_json::from_str::<ApiResponse>(&body_text).map_err(|e| ApiError::Http {
                status: 500,
                message: format!("failed to parse API response: {}", e),
            })
        }
    }

    /// Parse an SSE event stream and assemble the final ApiResponse.
    async fn parse_sse_stream<F>(
        &self,
        resp: reqwest::Response,
        on_progress: &mut F,
    ) -> Result<ApiResponse, ApiError>
    where
        F: FnMut(ProgressEvent),
    {
        use models_mod::{ImageData, Usage};

        // Read full response text — SSE is UTF-8
        let body_text = resp.text().await.map_err(ApiError::Network)?;
        let mut buffer = body_text;
        let mut final_images: Vec<ImageData> = Vec::new();
        let mut usage: Option<Usage> = None;

        // Process complete lines from the buffer
        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim().to_string();
            buffer = buffer[pos + 1..].to_string();

            if line.is_empty() || line.starts_with(':') {
                continue;
            }

            // Parse SSE "event: <type>" line
            if let Some(event_line) = line.strip_prefix("event: ") {
                let event_type = event_line.trim();
                // Read the next "data: ..." line
                while let Some(nl_pos) = buffer.find('\n') {
                    let data_line = buffer[..nl_pos].trim().to_string();
                    buffer = buffer[nl_pos + 1..].to_string();
                    if data_line.is_empty() || data_line.starts_with(':') {
                        continue;
                    }
                    if let Some(data_payload) = data_line.strip_prefix("data: ") {
                        let data_str = data_payload.trim();
                        match event_type {
                            "partial_image" => {
                                if let Ok(event) =
                                    serde_json::from_str::<SsePartialImageEvent>(data_str)
                                {
                                    on_progress(ProgressEvent::StreamPartial {
                                        index: event.index.unwrap_or(0) as u8,
                                        b64_length: event.b64_json.len(),
                                    });
                                }
                            }
                            "completed" => {
                                if let Ok(event) =
                                    serde_json::from_str::<SseCompletedEvent>(data_str)
                                {
                                    on_progress(ProgressEvent::StreamComplete {
                                        count: event.data.as_ref().map(|d| d.len()).unwrap_or(0)
                                            as u8,
                                    });
                                    if let Some(data) = event.data {
                                        final_images.extend(data);
                                    }
                                    usage = event.usage;
                                }
                            }
                            "error" => {
                                if let Ok(err_obj) = serde_json::from_str::<SseErrorEvent>(data_str)
                                {
                                    let msg = err_obj
                                        .error
                                        .as_ref()
                                        .and_then(|e| e.message.as_ref())
                                        .cloned()
                                        .unwrap_or_else(|| "SSE stream error".into());
                                    return Err(ApiError::Http {
                                        status: 500,
                                        message: msg,
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                    break;
                }
            }
        }

        // Assemble final ApiResponse from streamed data
        let created = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
        );

        Ok(ApiResponse {
            created,
            data: if final_images.is_empty() {
                None
            } else {
                Some(final_images)
            },
            usage,
            error: None,
        })
    }
}

// ---------------------------------------------------------------------------
// SSE event types (local, parsed from SSE data lines)
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
struct SsePartialImageEvent {
    index: Option<i32>,
    #[serde(rename = "b64_json")]
    b64_json: String,
}

#[derive(Debug, serde::Deserialize)]
struct SseCompletedEvent {
    data: Option<Vec<models_mod::ImageData>>,
    #[serde(rename = "usage", default)]
    usage: Option<models_mod::Usage>,
}

#[derive(Debug, serde::Deserialize)]
struct SseErrorEvent {
    error: Option<models_mod::ApiErrorBody>,
}

/// Build the request body from GenerationParams.
/// The `ref_urls` are pre-validated reference image URLs (data: or http(s)://)
/// passed separately because `GenerationParams.image_refs` may contain unvalidated raw input.
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

    if let Some(ref res) = params.resolution {
        body["resolution"] = serde_json::json!(res);
    }
    if let Some(ref ar) = params.aspect_ratio {
        body["aspect_ratio"] = serde_json::json!(ar);
    }
    if let Some(ref bg) = params.background {
        body["background"] = serde_json::json!(bg);
    }
    if let Some(ref fmt) = params.output_format {
        body["output_format"] = serde_json::json!(fmt);
    }
    if let Some(cmp) = params.output_compression {
        body["output_compression"] = serde_json::json!(cmp);
    }
    if let Some(ref q) = params.quality {
        body["quality"] = serde_json::json!(q);
    }
    if let Some(seed) = params.seed {
        body["seed"] = serde_json::json!(seed);
    }
    if let Some(ref size) = params.size {
        body["size"] = serde_json::json!(size);
    }
    if let Some(ref user) = params.user {
        body["user"] = serde_json::json!(user);
    }
    if let Some(ref sid) = params.session_id {
        body["session_id"] = serde_json::json!(sid);
    }
    if let Some(ref provider) = params.provider {
        // Serialize with serde, which skips None fields by default
        body["provider"] = serde_json::to_value(provider).unwrap_or_else(|_| serde_json::json!({}));
    }
    if let Some(ref trace) = params.trace {
        let trace_val = serde_json::to_value(trace).unwrap_or_else(|_| serde_json::json!({}));
        body["trace"] = trace_val;
    }
    if params.stream {
        body["stream"] = serde_json::json!(true);
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
            api_key: Some("sk-or-test".to_string()),
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
    fn test_build_request_body_full_fields() {
        let mut params = test_params();
        params.aspect_ratio = Some("16:9".to_string());
        params.background = Some("transparent".to_string());
        params.output_format = Some("png".to_string());
        params.output_compression = Some(80);
        params.quality = Some("high".to_string());
        params.seed = Some(42);
        params.size = Some("2048x2048".to_string());
        params.user = Some("user_abc".to_string());
        params.session_id = Some("sess_xyz".to_string());
        params.stream = true;

        let body = build_request_body(&params, &[]);
        assert_eq!(body["aspect_ratio"], "16:9");
        assert_eq!(body["background"], "transparent");
        assert_eq!(body["output_format"], "png");
        assert_eq!(body["output_compression"], 80);
        assert_eq!(body["quality"], "high");
        assert_eq!(body["seed"], 42);
        assert_eq!(body["size"], "2048x2048");
        assert_eq!(body["user"], "user_abc");
        assert_eq!(body["session_id"], "sess_xyz");
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn test_build_request_body_provider_routing() {
        let mut params = test_params();
        params.provider = Some(crate::models_mod::ProviderRouting {
            allow_fallbacks: Some(false),
            only: Some(vec!["google".to_string()]),
            ignore: None,
            order: None,
            sort: Some("latency".to_string()),
        });
        let body = build_request_body(&params, &[]);
        let prov = &body["provider"];
        assert_eq!(prov["allow_fallbacks"], false);
        assert_eq!(prov["only"][0], "google");
        assert_eq!(prov["sort"], "latency");
    }

    #[test]
    fn test_build_request_body_trace() {
        let mut params = test_params();
        params.trace = Some(crate::models_mod::TraceMetadata {
            trace_id: Some("trace-abc".to_string()),
            trace_name: Some("my-generation".to_string()),
            span_name: None,
            generation_name: None,
            parent_span_id: None,
            extra: std::collections::HashMap::new(),
        });
        let body = build_request_body(&params, &[]);
        let trace = &body["trace"];
        assert_eq!(trace["trace_id"], "trace-abc");
        assert_eq!(trace["trace_name"], "my-generation");
    }

    #[test]
    fn test_client_creation() {
        let client = HttpClient::new(120_000);
        assert!(client.is_ok());
    }

    #[test]
    fn test_output_format_as_api_str() {
        use crate::models_mod::OutputFormat;
        assert_eq!(OutputFormat::Png.as_api_str(), "png");
        assert_eq!(OutputFormat::Jpeg.as_api_str(), "jpeg");
        assert_eq!(OutputFormat::Webp.as_api_str(), "webp");
        assert_eq!(OutputFormat::Svg.as_api_str(), "svg");
    }
}
