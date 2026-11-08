// Copyright (c) 2026, Nathaniel T. Berry
// Licensed under the Apache License, Version 2.0.

//! xAI Responses API client (`src/symsight/llm.py`).
//!
//! Thin `reqwest` POST to `{base_url}/responses`. No official OpenAI crate.

use std::collections::VecDeque;
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

pub(crate) fn validate_base_url(base: &str) -> Result<(), LlmError> {
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

/// Client-side function the model may call. Generate advertises none today.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionTool {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub call_id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResult {
    pub call_id: String,
    pub output: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelTurn {
    Message(String),
    Calls(Vec<ToolCall>),
}

/// One generate conversation. `complete` starts it. `resume` continues after tool calls.
pub trait LlmSession {
    fn complete(
        &mut self,
        req: &CompletionRequest,
        tools: &[FunctionTool],
    ) -> Result<ModelTurn, LlmError>;

    fn resume(&mut self, results: &[ToolResult]) -> Result<ModelTurn, LlmError>;
}

pub trait LlmClient {
    fn create_completion(&self, req: &CompletionRequest) -> Result<String, LlmError>;
}

/// Wraps a one-shot client. Tool calls are not resumed.
pub struct OneShotSession<'a> {
    client: &'a dyn LlmClient,
}

impl<'a> OneShotSession<'a> {
    pub fn new(client: &'a dyn LlmClient) -> Self {
        Self { client }
    }
}

impl LlmSession for OneShotSession<'_> {
    fn complete(
        &mut self,
        req: &CompletionRequest,
        _tools: &[FunctionTool],
    ) -> Result<ModelTurn, LlmError> {
        Ok(ModelTurn::Message(self.client.create_completion(req)?))
    }

    fn resume(&mut self, _results: &[ToolResult]) -> Result<ModelTurn, LlmError> {
        Err(LlmError::Http(
            "this client cannot resume a tool call".into(),
        ))
    }
}

#[derive(Debug, Clone)]
struct TurnState {
    model: String,
    use_search: bool,
    tools: Vec<FunctionTool>,
    response_id: Option<String>,
}

/// Blocking xAI client. Do not construct this in unit tests — inject
/// [`ScriptedClient`] instead.
pub struct XaiClient {
    api_key: String,
    base_url: String,
    http: reqwest::blocking::Client,
    turn: Option<TurnState>,
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
            turn: None,
        })
    }

    fn post_json(&self, body: &Value) -> Result<Value, LlmError> {
        let url = responses_url(&self.base_url);
        let response = self
            .http
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(body)
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
        Ok(value)
    }
}

impl LlmClient for XaiClient {
    fn create_completion(&self, req: &CompletionRequest) -> Result<String, LlmError> {
        let value = self.post_json(&completion_body(req, &[]))?;
        Ok(response_text(&value))
    }
}

impl LlmSession for XaiClient {
    fn complete(
        &mut self,
        req: &CompletionRequest,
        tools: &[FunctionTool],
    ) -> Result<ModelTurn, LlmError> {
        let value = self.post_json(&completion_body(req, tools))?;
        let response_id = value.get("id").and_then(Value::as_str).map(str::to_string);
        self.turn = Some(TurnState {
            model: req.model.clone(),
            use_search: req.use_search,
            tools: tools.to_vec(),
            response_id,
        });
        Ok(parse_turn(&value))
    }

    fn resume(&mut self, results: &[ToolResult]) -> Result<ModelTurn, LlmError> {
        let state = self
            .turn
            .clone()
            .ok_or_else(|| LlmError::Http("no response to resume".into()))?;
        let body = resume_body(&state, results)?;
        let value = self.post_json(&body)?;
        let response_id = value
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or(state.response_id);
        self.turn = Some(TurnState {
            response_id,
            ..state
        });
        Ok(parse_turn(&value))
    }
}

fn responses_url(base: &str) -> String {
    let trimmed = base.trim_end_matches('/');
    format!("{trimmed}/responses")
}

pub(crate) fn completion_body(req: &CompletionRequest, tools: &[FunctionTool]) -> Value {
    let mut body = json!({
        "model": req.model,
        "input": [
            {"role": "system", "content": req.system},
            {"role": "user", "content": req.user},
        ],
    });
    if let Some(tools) = tools_json(req.use_search, tools) {
        body["tools"] = tools;
    }
    body
}

fn tools_json(use_search: bool, tools: &[FunctionTool]) -> Option<Value> {
    let mut items = Vec::new();
    if use_search {
        items.push(json!({"type": "web_search"}));
    }
    for tool in tools {
        items.push(json!({
            "type": "function",
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.parameters,
        }));
    }
    if items.is_empty() {
        None
    } else {
        Some(Value::Array(items))
    }
}

fn resume_body(state: &TurnState, results: &[ToolResult]) -> Result<Value, LlmError> {
    let Some(id) = state.response_id.as_deref() else {
        return Err(LlmError::Http(
            "response had no id; cannot resume a tool call".into(),
        ));
    };
    let input: Vec<Value> = results
        .iter()
        .map(|result| {
            json!({
                "type": "function_call_output",
                "call_id": result.call_id,
                "output": result.output,
            })
        })
        .collect();
    let mut body = json!({
        "model": state.model,
        "previous_response_id": id,
        "input": input,
    });
    if let Some(tools) = tools_json(state.use_search, &state.tools) {
        body["tools"] = tools;
    }
    Ok(body)
}

/// Final text, or client-side `function_call` items. Hosted tool items are ignored.
pub fn parse_turn(response: &Value) -> ModelTurn {
    let mut calls = Vec::new();
    if let Some(output) = response.get("output").and_then(Value::as_array) {
        for item in output {
            if item.get("type").and_then(Value::as_str) != Some("function_call") {
                continue;
            }
            let call_id = item
                .get("call_id")
                .or_else(|| item.get("id"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let name = item
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let arguments = match item.get("arguments") {
                Some(Value::String(text)) => text.clone(),
                Some(other) => other.to_string(),
                None => "{}".into(),
            };
            calls.push(ToolCall {
                call_id,
                name,
                arguments,
            });
        }
    }
    if calls.is_empty() {
        ModelTurn::Message(response_text(response))
    } else {
        ModelTurn::Calls(calls)
    }
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

/// Replays [`ModelTurn`]s. Used to test the tool loop without HTTP.
#[derive(Debug)]
pub struct ScriptedSession {
    turns: VecDeque<ModelTurn>,
    pub prompts: Vec<CompletionRequest>,
    pub resumes: Vec<Vec<ToolResult>>,
}

impl ScriptedSession {
    pub fn new(turns: impl Into<Vec<ModelTurn>>) -> Self {
        Self {
            turns: VecDeque::from(turns.into()),
            prompts: Vec::new(),
            resumes: Vec::new(),
        }
    }

    fn pop(&mut self) -> Result<ModelTurn, LlmError> {
        self.turns
            .pop_front()
            .ok_or_else(|| LlmError::Http("scripted session has no turns left".into()))
    }
}

impl LlmSession for ScriptedSession {
    fn complete(
        &mut self,
        req: &CompletionRequest,
        _tools: &[FunctionTool],
    ) -> Result<ModelTurn, LlmError> {
        self.prompts.push(req.clone());
        self.pop()
    }

    fn resume(&mut self, results: &[ToolResult]) -> Result<ModelTurn, LlmError> {
        self.resumes.push(results.to_vec());
        self.pop()
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

    fn sample_req(use_search: bool) -> CompletionRequest {
        CompletionRequest {
            model: "grok".into(),
            system: "sys".into(),
            user: "user".into(),
            use_search,
        }
    }

    #[test]
    fn completion_body_keeps_hosted_search_and_omits_empty_tools() {
        let with_search = completion_body(&sample_req(true), &[]);
        assert_eq!(with_search["tools"], json!([{"type": "web_search"}]));
        assert!(with_search.get("previous_response_id").is_none());

        let plain = completion_body(&sample_req(false), &[]);
        assert!(plain.get("tools").is_none());
        assert_eq!(plain["input"][0]["content"], "sys");
        assert_eq!(plain["input"][1]["content"], "user");
    }

    #[test]
    fn completion_body_can_advertise_a_function_beside_search() {
        let tool = FunctionTool {
            name: "read_file".into(),
            description: "Read a file".into(),
            parameters: json!({"type": "object", "properties": {"path": {"type": "string"}}}),
        };
        let body = completion_body(&sample_req(true), &[tool]);
        assert_eq!(body["tools"][0]["type"], "web_search");
        assert_eq!(body["tools"][1]["type"], "function");
        assert_eq!(body["tools"][1]["name"], "read_file");
    }

    #[test]
    fn resume_body_uses_previous_response_id() {
        let state = TurnState {
            model: "grok".into(),
            use_search: false,
            tools: Vec::new(),
            response_id: Some("resp_1".into()),
        };
        let body = resume_body(
            &state,
            &[ToolResult {
                call_id: "call_1".into(),
                output: "note".into(),
            }],
        )
        .unwrap();
        assert_eq!(body["previous_response_id"], "resp_1");
        assert_eq!(body["input"][0]["type"], "function_call_output");
        assert_eq!(body["input"][0]["call_id"], "call_1");
        assert_eq!(body["input"][0]["output"], "note");
        assert!(body.get("tools").is_none());
    }

    #[test]
    fn parse_turn_keeps_text_and_collects_function_calls() {
        let message = json!({
            "output_text": "TITLE: Ok",
            "output": [
                {"type": "web_search_call", "id": "ws_1"},
                {"type": "message", "content": [{"type": "output_text", "text": "TITLE: Ok"}]}
            ]
        });
        assert_eq!(parse_turn(&message), ModelTurn::Message("TITLE: Ok".into()));

        let calls = json!({
            "output": [
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "read_file",
                    "arguments": "{\"path\":\"notes.md\"}"
                },
                {"type": "message", "content": [{"type": "output_text", "text": "partial"}]}
            ]
        });
        match parse_turn(&calls) {
            ModelTurn::Calls(items) => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].call_id, "call_1");
                assert_eq!(items[0].name, "read_file");
                assert!(items[0].arguments.contains("notes.md"));
            }
            ModelTurn::Message(text) => panic!("expected calls, got {text}"),
        }
    }
}
