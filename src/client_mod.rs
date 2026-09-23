//! HTTP client for OpenRouter /images endpoint.

use std::time::{Duration, Instant};

use anyhow::Context;
use reqwest::Client as ReqwestClient;
use tokio::time::sleep;

use crate::config_mod::Config;
use crate::error_mod::ApiError;
use crate::models_mod::{ApiResponse, GenerationParams};
use crate::progress_mod::ProgressEvent;
use crate::reference_mod::resolve_reference;

const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/images";
const MAX_RETRIES: u32 = 2;

pub struct HttpClient {
    http: ReqwestClient,
}

impl HttpClient {
    pub fn new(timeout_ms: u64) -> anyhow::Result<Self> {
        let http = ReqwestClient::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .context("failed to build HTTP client")?;
        Ok(Self { http })
    }

    /// Call the OpenRouter API with automatic retry on 5xx.
    pub async fn call(
        &self,
        params: &GenerationParams,
        config: &Config,
    ) -> Result<(ApiResponse, Duration), ApiError> {
        self.call_with_progress(params, config, |_event| {}).await
    }

    /// Call the API with progress events and retry logic.
    pub async fn call_with_progress<F>(
        &self,
        params: &GenerationParams,
        config: &Config,
        mut on_progress: F,
    ) -> Result<(ApiResponse, Duration), ApiError>
    where
        F: FnMut(ProgressEvent),
    {
        let api_key = config.api_key().ok_or(ApiError::NoApiKey)?.to_string();

        let reference_url = if let Some(ref ref_arg) = params.reference {
            Some(resolve_reference(ref_arg).map_err(|e| ApiError::Http {
                status: 400,
                message: e.to_string(),
            })?)
        } else {
            None
        };

        let body = params.to_request_body(reference_url);
        let start = Instant::now();
        let mut attempt = 0u32;

        loop {
            attempt += 1;
            let result = self
                .do_request(&api_key, &body, params.retry, attempt, &mut on_progress)
                .await;

            match result {
                Ok(response) => {
                    on_progress(ProgressEvent::HttpStatus { status: 200 });
                    return Ok((response, start.elapsed()));
                }
                Err(err) => {
                    let is_server_error = matches!(err, ApiError::Http { status, .. } if (500..=599).contains(&status));

                    if is_server_error && params.retry && attempt < MAX_RETRIES {
                        let delay_ms = (2u64).pow(attempt) * 1000;
                        on_progress(ProgressEvent::RetryAttempt { attempt, delay_ms });
                        sleep(Duration::from_millis(delay_ms)).await;
                        continue;
                    }
                    return Err(err);
                }
            }
        }
    }

    async fn do_request<F>(
        &self,
        api_key: &str,
        body: &serde_json::Value,
        _retry: bool,
        _attempt: u32,
        on_progress: &mut F,
    ) -> Result<ApiResponse, ApiError>
    where
        F: FnMut(ProgressEvent),
    {
        let mut req = self.http.post(OPENROUTER_URL);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = HttpClient::new(120_000);
        assert!(client.is_ok());
    }

    #[test]
    fn test_client_zero_timeout() {
        let client = HttpClient::new(0);
        assert!(client.is_ok());
    }
}
