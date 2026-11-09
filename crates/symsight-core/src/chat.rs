// Copyright (c) 2026, Nathaniel T. Berry
// Licensed under the Apache License, Version 2.0.

//! OpenAI-compatible chat completions for a model on localhost.
//!
//! Ollama and llama.cpp speak `POST {base}/chat/completions`. This session
//! keeps its own transcript. It never sends `web_search`. An empty API key
//! omits the Authorization header.

use std::time::Duration;

use serde_json::{json, Value};

use crate::error::LlmError;
use crate::llm::{
    blocking_client, is_loopback_base_url, redact_secrets, validate_base_url, CompletionRequest,
    FunctionTool, ModelTurn, ToolCall, ToolResult, HTTP_TIMEOUT_SECS,
};

const HTTP_TIMEOUT: Duration = Duration::from_secs(HTTP_TIMEOUT_SECS);

/// Chat-completions session. `complete` replaces the transcript. `resume`
/// appends tool results and posts again.
pub struct LocalChatSession {
    api_key: String,
    base_url: String,
    http: reqwest::blocking::Client,
    messages: Vec<Value>,
    model: String,
    tools: Vec<FunctionTool>,
}

impl LocalChatSession {
    pub fn new(api_key: impl Into<String>, base_url: impl Into<String>) -> Result<Self, LlmError> {
        let base_url = base_url.into();
        validate_base_url(&base_url)?;
        if !is_loopback_base_url(&base_url) {
            return Err(LlmError::Http(
                "local chat requires a localhost base URL".into(),
            ));
        }
        // A redirect off loopback would carry a bearer token. Stay on this host.
        let http = blocking_client(HTTP_TIMEOUT, false)?;
        Ok(Self {
            api_key: api_key.into(),
            base_url,
            http,
            messages: Vec::new(),
            model: String::new(),
            tools: Vec::new(),
        })
    }

    fn post(&self) -> Result<Value, LlmError> {
        let url = chat_completions_url(&self.base_url);
        let body = chat_completion_body(&self.model, &self.messages, &self.tools);
        let request = self.http.post(&url);
        let request = if self.api_key.trim().is_empty() {
            request
        } else {
            request.bearer_auth(self.api_key.trim())
        };
        let response = request
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
        Ok(value)
    }
}

impl crate::llm::LlmSession for LocalChatSession {
    fn complete(
        &mut self,
        req: &CompletionRequest,
        tools: &[FunctionTool],
    ) -> Result<ModelTurn, LlmError> {
        self.model.clone_from(&req.model);
        self.tools = tools.to_vec();
        self.messages = initial_messages(&req.system, &req.user);
        let value = self.post()?;
        let (turn, assistant) = parse_chat_turn(&value);
        self.messages.push(assistant);
        Ok(turn)
    }

    fn resume(&mut self, results: &[ToolResult]) -> Result<ModelTurn, LlmError> {
        if self.messages.is_empty() {
            return Err(LlmError::Http("no response to resume".into()));
        }
        push_tool_results(&mut self.messages, results);
        let value = self.post()?;
        let (turn, assistant) = parse_chat_turn(&value);
        self.messages.push(assistant);
        Ok(turn)
    }
}

fn initial_messages(system: &str, user: &str) -> Vec<Value> {
    vec![
        json!({"role": "system", "content": system}),
        json!({"role": "user", "content": user}),
    ]
}

fn push_tool_results(messages: &mut Vec<Value>, results: &[ToolResult]) {
    for result in results {
        messages.push(json!({
            "role": "tool",
            "tool_call_id": result.call_id,
            "content": result.output,
        }));
    }
}

pub(crate) fn chat_completions_url(base: &str) -> String {
    format!("{}/chat/completions", base.trim().trim_end_matches('/'))
}

fn chat_completion_body(model: &str, messages: &[Value], tools: &[FunctionTool]) -> Value {
    let mut body = json!({
        "model": model,
        "messages": messages,
    });
    if let Some(tools) = chat_tools_json(tools) {
        body["tools"] = tools;
    }
    body
}

fn chat_tools_json(tools: &[FunctionTool]) -> Option<Value> {
    if tools.is_empty() {
        return None;
    }
    Some(Value::Array(
        tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters,
                    }
                })
            })
            .collect(),
    ))
}

/// `(turn, assistant message to store)`. Tool calls win over plain text.
/// The stored assistant message uses `content: null` when a call has no text.
pub(crate) fn parse_chat_turn(response: &Value) -> (ModelTurn, Value) {
    let message = response
        .pointer("/choices/0/message")
        .cloned()
        .unwrap_or_else(|| json!({"role": "assistant", "content": ""}));
    let content = message_content(&message);
    let calls = tool_calls_from(&message);
    let turn = if calls.is_empty() {
        ModelTurn::Message(content.clone().unwrap_or_default())
    } else {
        ModelTurn::Calls(calls.clone())
    };
    (turn, assistant_message(content, &calls))
}

fn message_content(message: &Value) -> Option<String> {
    match message.get("content") {
        Some(Value::String(text)) => Some(text.clone()),
        Some(Value::Null) | None => None,
        Some(other) => Some(other.to_string()),
    }
}

fn tool_calls_from(message: &Value) -> Vec<ToolCall> {
    let Some(items) = message.get("tool_calls").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .map(|item| {
            let call_id = item
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let function = item.get("function");
            let name = function
                .and_then(|f| f.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let arguments = match function.and_then(|f| f.get("arguments")) {
                Some(Value::String(text)) => text.clone(),
                Some(other) => other.to_string(),
                None => "{}".into(),
            };
            ToolCall {
                call_id,
                name,
                arguments,
            }
        })
        .collect()
}

fn assistant_message(content: Option<String>, calls: &[ToolCall]) -> Value {
    if calls.is_empty() {
        return json!({
            "role": "assistant",
            "content": content.unwrap_or_default(),
        });
    }
    let tool_calls: Vec<Value> = calls
        .iter()
        .map(|call| {
            json!({
                "id": call.call_id,
                "type": "function",
                "function": {
                    "name": call.name,
                    "arguments": call.arguments,
                }
            })
        })
        .collect();
    let content_value = match content {
        Some(text) if !text.is_empty() => Value::String(text),
        _ => Value::Null,
    };
    json!({
        "role": "assistant",
        "content": content_value,
        "tool_calls": tool_calls,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tool() -> FunctionTool {
        FunctionTool {
            name: "read_file".into(),
            description: "Read a file".into(),
            parameters: json!({"type": "object", "properties": {"path": {"type": "string"}}}),
        }
    }

    #[test]
    fn chat_url_trims_a_trailing_slash() {
        assert_eq!(
            chat_completions_url("http://127.0.0.1:11434/v1/"),
            "http://127.0.0.1:11434/v1/chat/completions"
        );
    }

    #[test]
    fn chat_body_omits_tools_and_never_sends_web_search() {
        let messages = initial_messages("sys", "user");
        let plain = chat_completion_body("llama", &messages, &[]);
        assert!(plain.get("tools").is_none());
        assert!(!plain.to_string().contains("web_search"));
        assert_eq!(plain["messages"][0]["content"], "sys");
        assert_eq!(plain["messages"][1]["role"], "user");

        let with_tool = chat_completion_body("llama", &messages, &[sample_tool()]);
        assert_eq!(with_tool["tools"][0]["type"], "function");
        assert_eq!(with_tool["tools"][0]["function"]["name"], "read_file");
        assert!(with_tool["tools"][0].get("name").is_none());
        assert!(!with_tool.to_string().contains("web_search"));
    }

    #[test]
    fn parse_chat_turn_reads_content_and_tool_calls() {
        let text = json!({
            "choices": [{"message": {"role": "assistant", "content": "TITLE: Hi"}}]
        });
        let (turn, assistant) = parse_chat_turn(&text);
        assert_eq!(turn, ModelTurn::Message("TITLE: Hi".into()));
        assert_eq!(assistant["content"], "TITLE: Hi");

        let calls = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "note",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "read_file", "arguments": "{\"path\":\"a.md\"}"}
                    }]
                }
            }]
        });
        let (turn, assistant) = parse_chat_turn(&calls);
        match turn {
            ModelTurn::Calls(calls) => {
                assert_eq!(calls[0].call_id, "call_1");
                assert_eq!(calls[0].name, "read_file");
                assert_eq!(calls[0].arguments, "{\"path\":\"a.md\"}");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(assistant["content"], "note");
        assert_eq!(assistant["tool_calls"][0]["function"]["name"], "read_file");

        let json_args = json!({
            "choices": [{
                "message": {
                    "content": null,
                    "tool_calls": [{
                        "id": "call_2",
                        "function": {"name": "read_file", "arguments": {"path": "a.md"}}
                    }]
                }
            }]
        });
        let (turn, assistant) = parse_chat_turn(&json_args);
        match turn {
            ModelTurn::Calls(calls) => assert_eq!(calls[0].arguments, "{\"path\":\"a.md\"}"),
            other => panic!("{other:?}"),
        }
        assert!(assistant["content"].is_null());
    }

    #[test]
    fn resume_appends_the_assistant_message_and_tool_results() {
        let response = json!({
            "choices": [{
                "message": {
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "function": {"name": "read_file", "arguments": "{}"}
                    }]
                }
            }]
        });
        let (_turn, assistant) = parse_chat_turn(&response);
        let mut messages = initial_messages("sys", "user");
        messages.push(assistant);
        push_tool_results(
            &mut messages,
            &[ToolResult {
                call_id: "call_1".into(),
                output: "note".into(),
            }],
        );
        let body = chat_completion_body("llama", &messages, &[]);
        assert_eq!(body["messages"][2]["role"], "assistant");
        assert_eq!(body["messages"][2]["tool_calls"][0]["id"], "call_1");
        assert_eq!(body["messages"][3]["role"], "tool");
        assert_eq!(body["messages"][3]["tool_call_id"], "call_1");
        assert_eq!(body["messages"][3]["content"], "note");
        assert!(!body.to_string().contains("web_search"));
    }

    #[test]
    fn new_rejects_a_remote_base_url() {
        let err = LocalChatSession::new("k", "https://api.x.ai/v1").unwrap_err();
        assert!(err.to_string().contains("localhost"), "{err}");
    }

    #[test]
    fn resume_without_a_turn_errors() {
        let mut session = LocalChatSession::new("", "http://127.0.0.1:9/v1").unwrap();
        let err = session.resume(&[]).unwrap_err();
        assert!(err.to_string().contains("no response to resume"), "{err}");
    }

    #[test]
    fn complete_posts_chat_completions_and_skips_auth_when_the_key_is_empty() {
        let (base, handle) = serve_once(r#"{"choices":[{"message":{"role":"assistant","content":"TITLE: Hi"}}]}"#);
        let mut session = LocalChatSession::new("  ", &base).unwrap();
        let turn = session
            .complete(
                &CompletionRequest {
                    model: "llama".into(),
                    system: "sys".into(),
                    user: "user".into(),
                    use_search: true,
                },
                &[],
            )
            .unwrap();
        assert_eq!(turn, ModelTurn::Message("TITLE: Hi".into()));
        let raw = handle.join().unwrap();
        assert!(raw.contains("POST /v1/chat/completions"), "{raw}");
        assert!(!raw.to_ascii_lowercase().contains("authorization"), "{raw}");
        assert!(!raw.contains("web_search"), "{raw}");
        assert!(raw.contains("\"model\":\"llama\"") || raw.contains("\"model\": \"llama\""), "{raw}");
    }

    #[test]
    fn complete_sends_a_bearer_token_when_a_key_is_set() {
        let (base, handle) = serve_once(r#"{"choices":[{"message":{"content":"ok"}}]}"#);
        let mut session = LocalChatSession::new("local-test-key", &base).unwrap();
        session
            .complete(
                &CompletionRequest {
                    model: "llama".into(),
                    system: "sys".into(),
                    user: "user".into(),
                    use_search: false,
                },
                &[],
            )
            .unwrap();
        let raw = handle.join().unwrap();
        assert!(
            raw.to_ascii_lowercase().contains("authorization: bearer local-test-key"),
            "{raw}"
        );
    }

    fn serve_once(body: &str) -> (String, std::thread::JoinHandle<String>) {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let body = body.to_string();
        let handle = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            sock.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut buf = Vec::new();
            let mut tmp = [0u8; 2048];
            loop {
                match sock.read(&mut tmp) {
                    Ok(0) => break,
                    Ok(n) => {
                        buf.extend_from_slice(&tmp[..n]);
                        if let Some(header_end) = header_end(&buf) {
                            let headers = String::from_utf8_lossy(&buf[..header_end]);
                            let have = buf.len().saturating_sub(header_end);
                            if have >= content_length(&headers) {
                                break;
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = sock.write_all(resp.as_bytes());
            String::from_utf8_lossy(&buf).into_owned()
        });
        (format!("http://127.0.0.1:{port}/v1"), handle)
    }

    fn header_end(buf: &[u8]) -> Option<usize> {
        buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
    }

    fn content_length(headers: &str) -> usize {
        headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                if name.eq_ignore_ascii_case("content-length") {
                    value.trim().parse().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0)
    }
}
