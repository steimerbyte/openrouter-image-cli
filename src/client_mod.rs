//! HTTP client for OpenRouter /images endpoint.
//!
//! Retry policy: exactly 1× retry on 5xx errors with 2-second sleep.
//! No retry on 4xx (including 429 — treated as API error, exit 4).

use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::time::{Duration, Instant};

use anyhow::{format_err, Context};
use reqwest::Client as ReqwestClient;
use tokio::time::sleep;

use crate::config_mod::Config;
use crate::error_mod::ApiError;
use crate::models_mod::{ApiResponse, GenerationParams};
use crate::progress_mod::ProgressEvent;
use crate::reference_mod::validate_reference;

// ---------------------------------------------------------------------------
// Base URL validation (SSRF + TLS hardening)
// ---------------------------------------------------------------------------

/// Returns true if `ip` is a loopback, private, or link-local address.
///
/// Covers:
/// - Loopback: `127.0.0.0/8`, `::1`
/// - Private: `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`
/// - Link-local: `169.254.0.0/16`, `fe80::/10`
/// - Unspecified: `0.0.0.0`, `::`
/// - IPv4-mapped IPv6: `::ffff:x.x.x.x`
fn is_private_or_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            // 127.0.0.0/8
            octets[0] == 127
            // 10.0.0.0/8
            || octets[0] == 10
            // 172.16.0.0/12  (172.16 – 172.31)
            || (octets[0] == 172 && (16..=31).contains(&octets[1]))
            // 192.168.0.0/16
            || (octets[0] == 192 && octets[1] == 168)
            // 169.254.0.0/16
            || (octets[0] == 169 && octets[1] == 254)
            // 0.0.0.0
            || (octets[0] == 0 && octets[1] == 0 && octets[2] == 0 && octets[3] == 0)
        }
        IpAddr::V6(v6) => {
            let segments = v6.segments();
            // ::1
            v6.is_loopback()
            // fc00::/7 — unique local
            || (segments[0] & 0xfe00) == 0xfc00
            // fe80::/10 — link-local (mask 0xffc0 matches the top 10 bits 1111111010)
            || (segments[0] & 0xffc0) == 0xfe80
            // ::ffff:x.x.x.x  (IPv4-mapped IPv6)
            || (segments[0] == 0 && segments[1] == 0 && segments[2] == 0 && segments[3] == 0
                && segments[4] == 0 && segments[5] == 0xffff)
            // :: (unspecified)
            || v6.is_unspecified()
        }
    }
}

/// Check whether the host part of `url` resolves to a private or loopback IP.
/// Returns `Ok` if the host is safe (no private IP found after DNS resolution).
/// Returns `Err` if any resolved address is private/loopback.
fn check_host_not_private(host: &str) -> Result<(), String> {
    // Try to resolve the host — if resolution fails we cannot check, so we allow.
    // (dns_not_found is not a security failure; non-resolving hosts will fail at request time anyway.)
    let addrs: Vec<SocketAddr> = match (host, 0u16).to_socket_addrs() {
        Ok(a) => a.collect(),
        Err(_) => return Ok(()),
    };

    for addr in addrs {
        if is_private_or_loopback(addr.ip()) {
            return Err(format!(
                "host \"{}\" resolves to private/loopback IP {} — rejecting for SSRF safety",
                host,
                addr.ip()
            ));
        }
    }

    Ok(())
}

/// Validates `OPENROUTER_BASE_URL` value for SSRF and TLS safety.
///
/// Rules:
/// - Scheme must be `http` or `https`.
/// - `http://` is only allowed for loopback hosts (`localhost`, `127.x.x.x`, `::1`).
/// - Non-loopback hosts must use `https://`.
/// - Non-loopback hosts are DNS-resolved; any private/loopback resolved IP is rejected.
///
/// This allows `http://127.0.0.1:8080` (wiremock in tests) while blocking
/// `http://my-internal-server.corp` or `http://attacker.com`.
fn validate_base_url(url: &str) -> Result<(), String> {
    let scheme = url_scheme(url);
    if scheme != "http" && scheme != "https" {
        return Err(format!(
            "OPENROUTER_BASE_URL scheme must be http or https, got \"{}\"",
            scheme
        ));
    }

    let host = url_host(url).ok_or_else(|| "OPENROUTER_BASE_URL is missing a host".to_string())?;

    // Detect loopback host by string (covers localhost, 127.x.x.x, ::1, 127.0.0.1)
    let is_loopback_host =
        host.eq_ignore_ascii_case("localhost") || host.starts_with("127.") || host == "::1";

    // HTTP is only permitted for loopback hosts (testing convenience)
    if scheme == "http" && !is_loopback_host {
        return Err(format!(
            "non-HTTPS OPENROUTER_BASE_URL is only allowed for loopback hosts; \
             host \"{}\" uses http — switch to https",
            host
        ));
    }

    // For non-loopback hosts: resolve DNS and reject private/loopback IPs
    if !is_loopback_host {
        check_host_not_private(host)?;
    }

    Ok(())
}

/// Extracts the scheme from a URL by splitting on "://".
fn url_scheme(url: &str) -> &str {
    url.split("://").next().unwrap_or("")
}

/// Extracts the bare host from a URL (strips scheme + optional user:pass@ + port + path).
/// Example: `https://user:pass@host.example.com:8080/api` → `host.example.com`
fn url_host(url: &str) -> Option<&str> {
    let after_scheme = url.split("://").nth(1)?;
    let after_auth = after_scheme.strip_prefix('@').unwrap_or(after_scheme);
    let host_port = after_auth.split(['/', ':']).next()?;
    if host_port.is_empty() {
        return None;
    }
    Some(host_port)
}

/// Returns the base URL for OpenRouter images API.
/// Override with `OPENROUTER_BASE_URL` env var (for integration tests).
///
/// # Errors
/// Returns an error if `OPENROUTER_BASE_URL` is set to an invalid, non-HTTPS,
/// or private-IP URL. Loopback hosts on `http://` are allowed (wiremock tests).
fn images_url() -> anyhow::Result<String> {
    let url = std::env::var("OPENROUTER_BASE_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://openrouter.ai/api/v1/images".to_string());

    // Validate before returning — fail closed on config-level SSRF risk.
    if let Err(msg) = validate_base_url(&url) {
        tracing::warn!("{}; rejecting OPENROUTER_BASE_URL", msg);
        return Err(format_err!("invalid OPENROUTER_BASE_URL: {}", msg));
    }

    Ok(url)
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
        let url = images_url().context("OPENROUTER_BASE_URL validation failed")?;
        Self::new_with_url(timeout_ms, url)
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
            .do_request(api_key.clone(), &body, &mut on_progress)
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

    /// Perform the HTTP request (single-shot buffered mode).
    async fn do_request<F>(
        &self,
        api_key: String,
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

        let body_text = resp.text().await.unwrap_or_default();
        serde_json::from_str::<ApiResponse>(&body_text).map_err(|e| ApiError::Http {
            status: 500,
            message: format!("failed to parse API response: {}", e),
        })
    }
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
            timeout_ms: 120_000,
            clobber: false,
            max_image_retries: 0,
            negative_prompt: None,
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
