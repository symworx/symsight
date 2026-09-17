// Copyright (c) 2026, Nathaniel T. Berry
// Licensed under the Apache License, Version 2.0.

//! xAI Responses API client (`src/symsight/llm.py`).
//!
//! Thin `reqwest` POST to `{base_url}/responses`. No official OpenAI crate.

use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use regex::Regex;
use serde_json::{json, Value};

use crate::error::LlmError;

const HTTP_TIMEOUT_SECS: u64 = 120;

static XAI_KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"xai-[A-Za-z0-9_-]+").expect("XAI_KEY"));
static BEARER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(bearer\s+)\S+").expect("BEARER"));

/// Strip API keys / bearer tokens from error text before it hits logs or stderr.
pub fn redact_secrets(text: &str) -> String {
    let text = XAI_KEY.replace_all(text, "xai-[redacted]");
    BEARER.replace_all(&text, "${1}[redacted]").into_owned()
}

fn validate_base_url(base: &str) -> Result<(), LlmError> {
    let trimmed = base.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("https://") {
        return Ok(());
    }
    let http_ok = lower.starts_with("http://127.0.0.1")
        || lower.starts_with("http://localhost")
        || lower.starts_with("http://[::1]");
    if http_ok {
        return Ok(());
    }
    Err(LlmError::Http(
        "SYMSIGHT_BASE_URL must be https:// (http is allowed only for localhost)".into(),
    ))
}

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub model: String,
    pub system: String,
    pub user: String,
    pub use_search: bool,
}

pub trait LlmClient {
    fn create_completion(&self, req: &CompletionRequest) -> Result<String, LlmError>;
}

/// Blocking xAI client. Do not construct this in unit tests — inject
/// [`ScriptedClient`] instead.
pub struct XaiClient {
    api_key: String,
    base_url: String,
    http: reqwest::blocking::Client,
}

impl XaiClient {
    pub fn new(api_key: impl Into<String>, base_url: impl Into<String>) -> Result<Self, LlmError> {
        let base_url = base_url.into();
        validate_base_url(&base_url)?;
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
            .use_rustls_tls()
            .build()
            .map_err(|e| LlmError::Http(redact_secrets(&e.to_string())))?;
        Ok(Self {
            api_key: api_key.into(),
            base_url,
            http,
        })
    }
}

impl LlmClient for XaiClient {
    fn create_completion(&self, req: &CompletionRequest) -> Result<String, LlmError> {
        let url = responses_url(&self.base_url);
        let mut body = json!({
            "model": req.model,
            "input": [
                {"role": "system", "content": req.system},
                {"role": "user", "content": req.user},
            ],
        });
        if req.use_search {
            body["tools"] = json!([{"type": "web_search"}]);
        }
        let response = self
            .http
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .map_err(|e| LlmError::Http(redact_secrets(&e.to_string())))?;
        let status = response.status();
        let value: Value = response
            .json()
            .map_err(|e| LlmError::Http(redact_secrets(&format!("status {status}: {e}"))))?;
        if !status.is_success() {
            return Err(LlmError::Http(redact_secrets(&format!(
                "status {status}: {value}"
            ))));
        }
        Ok(response_text(&value))
    }
}

fn responses_url(base: &str) -> String {
    let trimmed = base.trim_end_matches('/');
    format!("{trimmed}/responses")
}

/// Pull final assistant text; skip reasoning/tool chatter when possible.
pub fn response_text(response: &Value) -> String {
    if let Some(raw) = response.get("output_text").and_then(Value::as_str) {
        if !raw.trim().is_empty() {
            return raw.to_string();
        }
    }
    let mut parts = Vec::new();
    let Some(output) = response.get("output").and_then(Value::as_array) else {
        return String::new();
    };
    for item in output {
        let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
        if !item_type.is_empty()
            && item_type != "message"
            && item_type != "output_text"
            && (item_type.contains("reason") || item_type.contains("tool"))
        {
            continue;
        }
        let Some(content) = item.get("content").and_then(Value::as_array) else {
            continue;
        };
        for part in content {
            let ctype = part.get("type").and_then(Value::as_str);
            if ctype.is_none() || ctype == Some("output_text") || ctype == Some("text") {
                if let Some(text) = part.get("text").and_then(Value::as_str) {
                    parts.push(text);
                }
            }
        }
    }
    parts.concat()
}

/// Test double: queued completion strings, records each request.
#[derive(Debug)]
pub struct ScriptedClient {
    responses: Mutex<Vec<String>>,
    pub calls: Mutex<Vec<CompletionRequest>>,
}

impl ScriptedClient {
    pub fn new(responses: impl Into<Vec<String>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            calls: Mutex::new(Vec::new()),
        }
    }

    pub fn call_count(&self) -> usize {
        self.calls.lock().expect("scripted calls").len()
    }
}

impl LlmClient for ScriptedClient {
    fn create_completion(&self, req: &CompletionRequest) -> Result<String, LlmError> {
        self.calls.lock().expect("scripted calls").push(req.clone());
        let mut queue = self.responses.lock().expect("scripted responses");
        if queue.is_empty() {
            return Ok(String::new());
        }
        Ok(queue.remove(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_secrets_strips_xai_keys_and_bearer() {
        assert_eq!(
            redact_secrets("got xai-ABC123xyz from API"),
            "got xai-[redacted] from API"
        );
        assert_eq!(
            redact_secrets("Authorization: Bearer super-secret-token"),
            "Authorization: Bearer [redacted]"
        );
    }

    #[test]
    fn rejects_cleartext_remote_http_base_url() {
        let err = XaiClient::new("k", "http://example.com/v1")
            .err()
            .expect("cleartext remote http must fail");
        assert!(err.to_string().contains("https://"), "{err}");
    }

    #[test]
    fn allows_https_and_localhost_http() {
        assert!(XaiClient::new("k", "https://api.x.ai/v1").is_ok());
        assert!(XaiClient::new("k", "http://127.0.0.1:8080/v1").is_ok());
        assert!(XaiClient::new("k", "http://localhost:8080/v1").is_ok());
    }
}
