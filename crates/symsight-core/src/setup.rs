// Copyright (c) 2026, Nathaniel T. Berry
// Licensed under the Apache License, Version 2.0.

//! Write project model settings. The API key goes only to `.env`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

use crate::config::{save_project_config, AppConfig};
use crate::error::SetupError;
use crate::llm::{
    blocking_client, is_loopback_base_url, redact_secrets, validate_base_url,
};

const MODEL_LIST_TIMEOUT: Duration = Duration::from_secs(5);

/// What `symsight setup` should store. `api_key` is remote-only.
/// `None` leaves `.env` alone. A local request rejects a non-empty key.
pub struct SetupRequest {
    pub local: bool,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
}

/// Files touched. The key itself is not in this report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupReport {
    pub config_path: PathBuf,
    pub env_path: Option<PathBuf>,
    pub base_url: String,
    pub model: String,
    pub local: bool,
    pub wrote_api_key: bool,
}

/// Save `model` and `base_url` in `.symsight.toml`. A remote key is upserted
/// into `.env` and is never written to the toml file.
pub fn apply_setup(cfg: &AppConfig, req: &SetupRequest) -> Result<SetupReport, SetupError> {
    let base_url = req.base_url.trim().to_string();
    let model = req.model.trim().to_string();
    if model.is_empty() {
        return Err(SetupError::Message("model is required".into()));
    }
    validate_base_url(&base_url).map_err(|err| SetupError::Message(err.to_string()))?;
    let loopback = is_loopback_base_url(&base_url);
    if req.local && !loopback {
        return Err(SetupError::Message(
            "local setup requires a localhost base URL".into(),
        ));
    }
    if !req.local && loopback {
        return Err(SetupError::Message(
            "remote setup requires an https base URL that is not localhost".into(),
        ));
    }
    let key = normalized_key(req)?;

    let mut next = cfg.clone();
    next.model = model.clone();
    next.base_url.clone_from(&base_url);
    let config_path = save_project_config(&next, None)?;

    let mut env_path = None;
    let mut wrote_api_key = false;
    if let Some(key) = key {
        let path = cfg.project_root.join(".env");
        upsert_env_key(&path, &key)?;
        env_path = Some(path);
        wrote_api_key = true;
    }

    Ok(SetupReport {
        config_path,
        env_path,
        base_url,
        model,
        local: req.local,
        wrote_api_key,
    })
}

/// `GET {base}/models`. OpenAI `data[].id`, otherwise Ollama `models[].name`.
pub fn list_model_ids(base_url: &str) -> Result<Vec<String>, SetupError> {
    let base_url = base_url.trim();
    validate_base_url(base_url).map_err(|err| SetupError::Message(err.to_string()))?;
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let http = blocking_client(MODEL_LIST_TIMEOUT, false)
        .map_err(|err| SetupError::Message(err.to_string()))?;
    let response = http
        .get(url)
        .send()
        .map_err(|err| SetupError::Message(redact_secrets(&err.to_string())))?;
    let status = response.status();
    let value: Value = response.json().map_err(|err| {
        SetupError::Message(redact_secrets(&format!("status {status}: {err}")))
    })?;
    if !status.is_success() {
        return Err(SetupError::Message(redact_secrets(&format!(
            "status {status}: {value}"
        ))));
    }
    Ok(parse_model_ids(&value))
}

pub(crate) fn parse_model_ids(value: &Value) -> Vec<String> {
    let mut from_data = Vec::new();
    if let Some(data) = value.get("data").and_then(Value::as_array) {
        for item in data {
            if let Some(id) = item.get("id").and_then(Value::as_str) {
                if !id.is_empty() {
                    from_data.push(id.to_string());
                }
            }
        }
        if !from_data.is_empty() {
            return from_data;
        }
    }
    let mut from_models = Vec::new();
    if let Some(models) = value.get("models").and_then(Value::as_array) {
        for item in models {
            let name = item
                .get("name")
                .and_then(Value::as_str)
                .or_else(|| item.get("model").and_then(Value::as_str));
            if let Some(name) = name {
                if !name.is_empty() {
                    from_models.push(name.to_string());
                }
            }
        }
    }
    from_models
}

fn normalized_key(req: &SetupRequest) -> Result<Option<String>, SetupError> {
    let Some(raw) = req.api_key.as_deref() else {
        return Ok(None);
    };
    let key = raw.trim();
    if req.local {
        if key.is_empty() {
            return Ok(None);
        }
        return Err(SetupError::Message(
            "local setup does not take an API key".into(),
        ));
    }
    if key.is_empty() {
        return Err(SetupError::Message("API key is empty".into()));
    }
    if key.contains('\n') || key.contains('\r') || key.contains('\0') {
        return Err(SetupError::Message("API key must be a single line".into()));
    }
    Ok(Some(key.to_string()))
}

fn upsert_env_key(path: &Path, key: &str) -> std::io::Result<()> {
    let existed = path.is_file();
    let text = if existed {
        fs::read_to_string(path)?
    } else {
        String::new()
    };
    let updated = rewrite_env(&text, key);
    if existed {
        fs::write(path, updated)
    } else {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        write_new_private(path, &updated)
    }
}

fn rewrite_env(text: &str, key: &str) -> String {
    let assignment = format!("XAI_API_KEY={}", format_env_value(key));
    let mut lines: Vec<&str> = if text.is_empty() {
        Vec::new()
    } else {
        text.split('\n').collect()
    };
    if text.ends_with('\n') && lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    let mut replaced = false;
    let mut owned: Vec<String> = lines
        .into_iter()
        .map(|line| {
            if is_api_key_line(line) {
                replaced = true;
                assignment.clone()
            } else {
                line.to_string()
            }
        })
        .collect();
    if !replaced {
        owned.push(assignment);
    }
    owned.push(String::new());
    owned.join("\n")
}

fn is_api_key_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("XAI_API_KEY=") || trimmed.starts_with("export XAI_API_KEY=")
}

fn format_env_value(key: &str) -> String {
    if key
        .chars()
        .any(|ch| ch.is_whitespace() || matches!(ch, '#' | '"' | '\'' | '\\'))
    {
        let escaped = key.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{escaped}\"")
    } else {
        key.to_string()
    }
}

fn write_new_private(path: &Path, text: &str) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(text.as_bytes())?;
        return Ok(());
    }
    #[cfg(not(unix))]
    {
        fs::write(path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DEFAULT_BASE_URL;
    use serde_json::json;

    fn tmp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "symsight-setup-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cfg(root: &Path) -> AppConfig {
        AppConfig {
            xai_api_key: String::new(),
            model: "grok-4.5".into(),
            base_url: DEFAULT_BASE_URL.into(),
            active_brand: "example-writer".into(),
            brands_dir: root.join("config/brands"),
            drafts_dir: root.join("content/drafts"),
            final_dir: root.join("content/final"),
            project_root: root.to_path_buf(),
        }
    }

    #[test]
    fn parse_model_ids_reads_openai_then_ollama() {
        let openai = json!({"data": [{"id": "llama3.2"}, {"id": "mistral"}]});
        assert_eq!(parse_model_ids(&openai), vec!["llama3.2", "mistral"]);
        let ollama = json!({"models": [{"name": "llama3.2:latest"}, {"model": "phi3"}]});
        assert_eq!(
            parse_model_ids(&ollama),
            vec!["llama3.2:latest", "phi3"]
        );
        let empty_data = json!({"data": [], "models": [{"name": "fallback"}]});
        assert_eq!(parse_model_ids(&empty_data), vec!["fallback"]);
    }

    #[test]
    fn local_setup_writes_toml_and_skips_env() {
        let root = tmp_dir();
        fs::write(root.join(".symsight.toml"), "custom_flag = true\n").unwrap();
        let report = apply_setup(
            &cfg(&root),
            &SetupRequest {
                local: true,
                base_url: "http://127.0.0.1:11434/v1".into(),
                model: "llama3.2".into(),
                api_key: None,
            },
        )
        .unwrap();
        assert!(report.local);
        assert!(!report.wrote_api_key);
        assert!(report.env_path.is_none());
        assert!(!root.join(".env").exists());
        let text = fs::read_to_string(report.config_path).unwrap();
        assert!(text.contains("http://127.0.0.1:11434/v1"), "{text}");
        assert!(text.contains("llama3.2"), "{text}");
        assert!(text.contains("custom_flag"), "{text}");
        assert!(!text.contains("XAI_API_KEY"), "{text}");
    }

    #[test]
    fn remote_setup_writes_the_key_only_in_env() {
        let root = tmp_dir();
        let env_path = root.join(".env");
        fs::write(&env_path, "OTHER=keep\nexport XAI_API_KEY=old\n# comment\n").unwrap();
        let secret = "unit-test-key";
        let report = apply_setup(
            &cfg(&root),
            &SetupRequest {
                local: false,
                base_url: "https://api.x.ai/v1".into(),
                model: "grok-4.5".into(),
                api_key: Some(secret.into()),
            },
        )
        .unwrap();
        assert!(report.wrote_api_key);
        assert!(!format!("{report:?}").contains(secret));
        let env = fs::read_to_string(&env_path).unwrap();
        assert!(env.contains("OTHER=keep"), "{env}");
        assert!(env.contains(&format!("XAI_API_KEY={secret}")), "{env}");
        assert!(env.contains("# comment"), "{env}");
        assert!(!env.contains("export XAI_API_KEY=old"), "{env}");
        let parsed: std::collections::HashMap<String, String> = dotenvy::from_path_iter(&env_path)
            .unwrap()
            .map(|item| item.unwrap())
            .collect();
        assert_eq!(parsed.get("XAI_API_KEY").map(String::as_str), Some(secret));
        assert_eq!(parsed.get("OTHER").map(String::as_str), Some("keep"));
        let toml_text = fs::read_to_string(&report.config_path).unwrap();
        assert!(toml_text.contains("https://api.x.ai/v1"), "{toml_text}");
        assert!(!toml_text.contains(secret), "{toml_text}");
    }

    #[cfg(unix)]
    #[test]
    fn new_env_file_is_owner_only_and_an_existing_mode_stays() {
        use std::os::unix::fs::PermissionsExt;
        let root = tmp_dir();
        let secret = "unit-test-key";
        apply_setup(
            &cfg(&root),
            &SetupRequest {
                local: false,
                base_url: "https://api.x.ai/v1".into(),
                model: "grok-4.5".into(),
                api_key: Some(secret.into()),
            },
        )
        .unwrap();
        let path = root.join(".env");
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode & 0o077, 0, "mode {mode:o} is group- or world-readable");
        assert_eq!(mode & 0o600, 0o600, "mode {mode:o} is missing owner rw");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        apply_setup(
            &cfg(&root),
            &SetupRequest {
                local: false,
                base_url: "https://api.x.ai/v1".into(),
                model: "grok-4.5".into(),
                api_key: Some("unit-test-key-2".into()),
            },
        )
        .unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640);
    }

    #[test]
    fn setup_rejects_mismatched_urls_and_a_local_key() {
        let root = tmp_dir();
        let local_remote = apply_setup(
            &cfg(&root),
            &SetupRequest {
                local: true,
                base_url: "https://api.x.ai/v1".into(),
                model: "llama".into(),
                api_key: None,
            },
        )
        .unwrap_err();
        assert!(local_remote.to_string().contains("localhost"), "{local_remote}");

        let remote_local = apply_setup(
            &cfg(&root),
            &SetupRequest {
                local: false,
                base_url: "http://127.0.0.1:11434/v1".into(),
                model: "grok-4.5".into(),
                api_key: None,
            },
        )
        .unwrap_err();
        assert!(
            remote_local.to_string().contains("not localhost"),
            "{remote_local}"
        );

        let with_key = apply_setup(
            &cfg(&root),
            &SetupRequest {
                local: true,
                base_url: "http://127.0.0.1:11434/v1".into(),
                model: "llama".into(),
                api_key: Some("unit-test-key".into()),
            },
        )
        .unwrap_err();
        assert!(with_key.to_string().contains("does not take an API key"), "{with_key}");
        assert!(!root.join(".symsight.toml").exists());
        assert!(!root.join(".env").exists());
    }

    #[test]
    fn setup_rejects_a_multiline_key_without_writing() {
        let root = tmp_dir();
        let err = apply_setup(
            &cfg(&root),
            &SetupRequest {
                local: false,
                base_url: "https://api.x.ai/v1".into(),
                model: "grok-4.5".into(),
                api_key: Some("one\ntwo".into()),
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("single line"), "{err}");
        assert!(!root.join(".env").exists());
        assert!(!root.join(".symsight.toml").exists());
    }
}
