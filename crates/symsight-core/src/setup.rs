// Copyright (c) 2026, Nathaniel T. Berry
// Licensed under the Apache License, Version 2.0.

//! Project file for `symsight setup`, plus the older model-settings writer.
//! The API key still goes only to `.env` when `apply_setup` is called.

use std::fs;
use std::io::{BufRead, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde_json::Value;

use crate::config::{save_project_config, AppConfig, DEFAULT_LOCAL_BASE_URL};
use crate::error::SetupError;
use crate::llm::{blocking_client, is_loopback_base_url, redact_secrets, validate_base_url};

const MODEL_LIST_TIMEOUT: Duration = Duration::from_secs(5);

const ACTIONS: [&str; 4] = ["new", "edit", "delete", "quit"];

/// Starter `.symsight.toml`. Same text as `config/.symsight.toml.example`.
pub const STARTER_PROJECT_CONFIG: &str = "\
# Copy to .symsight.toml in the project root (or ~/.config/symsight/config.toml).\n\
\n\
active_brand = \"example-writer\"\n\
brands_dir = \"./config/brands\"\n\
drafts_dir = \"./content/drafts\"\n\
final_dir = \"./content/final\"\n\
model = \"grok-4.5\"\n\
base_url = \"https://api.x.ai/v1\"\n\
";

/// `{root}/.symsight.toml`.
pub fn project_config_path(root: &Path) -> PathBuf {
    root.join(".symsight.toml")
}

/// Result of `create`. An existing file is left as it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateProjectConfig {
    Created(PathBuf),
    AlreadyThere(PathBuf),
}

impl CreateProjectConfig {
    pub fn message(&self) -> String {
        match self {
            Self::Created(path) => format!("created: {}", path.display()),
            Self::AlreadyThere(_) => {
                ".symsight.toml was not replaced. Edit it with `symsight edit`.".to_string()
            }
        }
    }
}

/// Write the starter project file. Does not replace a file that is already there.
pub fn create_project_config(root: &Path) -> Result<CreateProjectConfig, SetupError> {
    let dest = project_config_path(root);
    if dest.exists() {
        return Ok(CreateProjectConfig::AlreadyThere(dest));
    }
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&dest)
    {
        Ok(mut file) => {
            file.write_all(STARTER_PROJECT_CONFIG.as_bytes())?;
            Ok(CreateProjectConfig::Created(dest))
        }
        Err(err) if err.kind() == ErrorKind::AlreadyExists => {
            Ok(CreateProjectConfig::AlreadyThere(dest))
        }
        Err(err) => Err(err.into()),
    }
}

/// `$VISUAL`, then `$EDITOR`. Empty values are skipped.
pub fn preferred_editor(visual: Option<&str>, editor: Option<&str>) -> Option<String> {
    nonempty(visual).or_else(|| nonempty(editor))
}

fn nonempty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// Open `.symsight.toml` in `editor`. The program name is not split on spaces.
pub fn edit_project_config(root: &Path, editor: Option<&str>) -> Result<PathBuf, SetupError> {
    let dest = project_config_path(root);
    if !dest.is_file() {
        return Err(SetupError::Message(
            ".symsight.toml is missing. Write it with `symsight create`.".into(),
        ));
    }
    let Some(editor) = editor.map(str::trim).filter(|text| !text.is_empty()) else {
        return Err(SetupError::Message("set VISUAL or EDITOR".into()));
    };
    let status = Command::new(editor).arg(&dest).status()?;
    if !status.success() {
        return Err(SetupError::Message(format!("editor failed ({status})")));
    }
    Ok(dest)
}

/// Remove `.symsight.toml` only. `.env` is left in place.
pub fn delete_project_config(root: &Path) -> Result<PathBuf, SetupError> {
    let dest = project_config_path(root);
    if !dest.is_file() {
        return Err(SetupError::Message(
            ".symsight.toml is missing. Nothing to delete.".into(),
        ));
    }
    fs::remove_file(&dest)?;
    Ok(dest)
}

/// `y`, `Y`, `yes`, or `YES`.
pub fn delete_confirmed(answer: &str) -> bool {
    matches!(answer.trim(), "y" | "Y" | "yes" | "YES")
}

/// Non-interactive `symsight setup` text.
pub fn setup_shortcut_text(root: &Path) -> String {
    let path = project_config_path(root);
    format!(
        "File: {}\nnew:    symsight create\nedit:   symsight edit\ndelete: symsight delete\n",
        path.display()
    )
}

/// Interactive menu header. `arrows` adds the key line used by the Python TTY menu.
pub fn setup_menu_header(root: &Path, arrows: bool) -> String {
    let path = project_config_path(root);
    let mut text = format!(
        "File: {}\n\nShortcuts: symsight create, symsight edit, symsight delete\n\n",
        path.display()
    );
    if arrows {
        text.push_str("Arrow keys move. Enter selects. q or Ctrl-C quits.\n\n");
    }
    text
}

/// Line menu. With `interactive` false, print the shortcuts and return.
/// Pass `editor` in from the caller so tests do not change the process environment.
pub fn run_setup_lines(
    root: &Path,
    interactive: bool,
    editor: Option<&str>,
    reader: &mut impl BufRead,
    out: &mut impl Write,
) -> Result<(), SetupError> {
    if !interactive {
        write!(out, "{}", setup_shortcut_text(root))?;
        return Ok(());
    }
    write!(out, "{}", setup_menu_header(root, false))?;
    loop {
        for action in ACTIONS {
            writeln!(out, "{action}")?;
        }
        let choice = prompt_line(out, reader, "> ")?.to_ascii_lowercase();
        match choice.as_str() {
            "new" => report_create(root, out)?,
            "edit" => report_edit(root, editor, out)?,
            "delete" => line_delete(root, reader, out)?,
            "quit" | "q" | "" => return Ok(()),
            other => writeln!(out, "choose new, edit, delete, or quit (got '{other}')")?,
        }
    }
}

/// Base URL for one `generate` run. `--local` points a remote URL at the
/// default localhost server and leaves a localhost URL, including its port, as saved.
pub fn generate_base_url(configured: &str, local: bool) -> String {
    if local && !is_loopback_base_url(configured) {
        DEFAULT_LOCAL_BASE_URL.to_string()
    } else {
        configured.to_string()
    }
}

fn report_create(root: &Path, out: &mut impl Write) -> Result<(), SetupError> {
    match create_project_config(root) {
        Ok(outcome) => writeln!(out, "{}", outcome.message())?,
        Err(err) => writeln!(out, "error: {err}")?,
    }
    Ok(())
}

fn report_edit(root: &Path, editor: Option<&str>, out: &mut impl Write) -> Result<(), SetupError> {
    match edit_project_config(root, editor) {
        Ok(path) => writeln!(out, "edited: {}", path.display())?,
        Err(err) => writeln!(out, "error: {err}")?,
    }
    Ok(())
}

fn line_delete(
    root: &Path,
    reader: &mut impl BufRead,
    out: &mut impl Write,
) -> Result<(), SetupError> {
    let path = project_config_path(root);
    if !confirm_delete(&path, reader, out)? {
        writeln!(out, ">>> delete cancelled")?;
        return Ok(());
    }
    match delete_project_config(root) {
        Ok(path) => writeln!(out, "deleted: {}", path.display())?,
        Err(err) => writeln!(out, "error: {err}")?,
    }
    Ok(())
}

fn confirm_delete(
    path: &Path,
    reader: &mut impl BufRead,
    out: &mut impl Write,
) -> Result<bool, SetupError> {
    write!(out, "Delete {}? [y/N] ", path.display())?;
    out.flush()?;
    let mut answer = String::new();
    reader.read_line(&mut answer)?;
    Ok(delete_confirmed(&answer))
}

fn prompt_line(
    out: &mut impl Write,
    reader: &mut impl BufRead,
    label: &str,
) -> Result<String, SetupError> {
    write!(out, "{label}")?;
    out.flush()?;
    let mut line = String::new();
    reader.read_line(&mut line)?;
    Ok(line.trim().to_string())
}

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
    let value: Value = response
        .json()
        .map_err(|err| SetupError::Message(redact_secrets(&format!("status {status}: {err}"))))?;
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
        return fs::write(path, updated);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    write_new_private(path, &updated)
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

#[cfg(unix)]
fn write_new_private(path: &Path, text: &str) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(text.as_bytes())?;
    Ok(())
}

#[cfg(not(unix))]
fn write_new_private(path: &Path, text: &str) -> std::io::Result<()> {
    fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DEFAULT_BASE_URL, DEFAULT_LOCAL_BASE_URL};
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
        assert_eq!(parse_model_ids(&ollama), vec!["llama3.2:latest", "phi3"]);
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
        assert!(
            local_remote.to_string().contains("localhost"),
            "{local_remote}"
        );

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
        assert!(
            with_key.to_string().contains("does not take an API key"),
            "{with_key}"
        );
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

    #[test]
    fn starter_matches_the_example_when_present() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/.symsight.toml.example");
        let Ok(example) = fs::read_to_string(&path) else {
            return;
        };
        assert_eq!(example, STARTER_PROJECT_CONFIG);
    }

    #[test]
    fn create_writes_the_starter_and_does_not_replace_it() {
        let root = tmp_dir();
        let created = create_project_config(&root).unwrap();
        assert!(matches!(created, CreateProjectConfig::Created(_)));
        assert!(
            created.message().starts_with("created: "),
            "{}",
            created.message()
        );
        assert_eq!(
            fs::read_to_string(project_config_path(&root)).unwrap(),
            STARTER_PROJECT_CONFIG
        );
        fs::write(project_config_path(&root), "kept = true\n").unwrap();
        let again = create_project_config(&root).unwrap();
        assert!(matches!(again, CreateProjectConfig::AlreadyThere(_)));
        assert!(
            again.message().contains("was not replaced"),
            "{}",
            again.message()
        );
        assert_eq!(
            fs::read_to_string(project_config_path(&root)).unwrap(),
            "kept = true\n"
        );
        assert!(!root.join(".env").exists());
    }

    #[test]
    fn edit_requires_a_file_and_an_editor() {
        let root = tmp_dir();
        let missing = edit_project_config(&root, Some("/bin/true")).unwrap_err();
        assert!(missing.to_string().contains("symsight create"), "{missing}");
        create_project_config(&root).unwrap();
        let no_editor = edit_project_config(&root, None).unwrap_err();
        assert!(
            no_editor.to_string().contains("VISUAL or EDITOR"),
            "{no_editor}"
        );
        let empty = edit_project_config(&root, Some("  ")).unwrap_err();
        assert!(empty.to_string().contains("VISUAL or EDITOR"), "{empty}");
    }

    #[cfg(unix)]
    #[test]
    fn edit_runs_the_program_on_the_file_and_reports_failure() {
        use std::os::unix::fs::PermissionsExt;
        let root = tmp_dir();
        create_project_config(&root).unwrap();
        let script = root.join("mark-editor.sh");
        fs::write(&script, "#!/bin/sh\necho marked >> \"$1\"\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let path = edit_project_config(&root, Some(script.to_str().unwrap())).unwrap();
        assert_eq!(path, project_config_path(&root));
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("marked"), "{text}");
        let failed = edit_project_config(&root, Some("/bin/false")).unwrap_err();
        assert!(failed.to_string().contains("editor failed"), "{failed}");
    }

    #[test]
    fn delete_removes_only_the_project_file() {
        let root = tmp_dir();
        let missing = delete_project_config(&root).unwrap_err();
        assert!(
            missing.to_string().contains("Nothing to delete"),
            "{missing}"
        );
        create_project_config(&root).unwrap();
        fs::write(root.join(".env"), "XAI_API_KEY=unit-test-key\n").unwrap();
        let path = delete_project_config(&root).unwrap();
        assert_eq!(path, project_config_path(&root));
        assert!(!path.exists());
        assert!(root.join(".env").is_file());
    }

    #[test]
    fn delete_answers() {
        assert!(delete_confirmed("y"));
        assert!(delete_confirmed(" Y "));
        assert!(delete_confirmed("yes"));
        assert!(delete_confirmed("YES"));
        assert!(!delete_confirmed(""));
        assert!(!delete_confirmed("n"));
        assert!(!delete_confirmed("yeah"));
    }

    #[test]
    fn generate_local_flag_switches_only_a_remote_url() {
        assert_eq!(
            generate_base_url("https://api.x.ai/v1", true),
            DEFAULT_LOCAL_BASE_URL
        );
        assert_eq!(
            generate_base_url("http://127.0.0.1:9999/v1", true),
            "http://127.0.0.1:9999/v1"
        );
        assert_eq!(
            generate_base_url("https://api.x.ai/v1", false),
            "https://api.x.ai/v1"
        );
        assert_eq!(
            generate_base_url("http://localhost:11434/v1", false),
            "http://localhost:11434/v1"
        );
    }

    #[test]
    fn visual_wins_over_editor() {
        assert_eq!(
            preferred_editor(Some("vim"), Some("nano")).as_deref(),
            Some("vim")
        );
        assert_eq!(
            preferred_editor(Some("  "), Some("nano")).as_deref(),
            Some("nano")
        );
        assert_eq!(preferred_editor(None, Some("")).as_deref(), None);
    }

    #[test]
    fn plain_setup_prints_shortcuts_and_exits() {
        use std::io::Cursor;
        let root = tmp_dir();
        let mut input = Cursor::new("new\n");
        let mut out = Vec::new();
        run_setup_lines(&root, false, None, &mut input, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text, setup_shortcut_text(&root));
        assert!(text.contains("new:    symsight create\n"), "{text}");
        assert!(text.contains("edit:   symsight edit\n"), "{text}");
        assert!(text.contains("delete: symsight delete\n"), "{text}");
        assert!(!text.contains("> "), "{text}");
        assert!(!project_config_path(&root).exists());
    }

    #[test]
    fn line_menu_rejects_unknown_then_quits() {
        use std::io::Cursor;
        let root = tmp_dir();
        let mut input = Cursor::new("create\nq\n");
        let mut out = Vec::new();
        run_setup_lines(&root, true, None, &mut input, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("Shortcuts: symsight create, symsight edit, symsight delete"),
            "{text}"
        );
        assert!(
            text.contains("choose new, edit, delete, or quit (got 'create')"),
            "{text}"
        );
        assert!(!project_config_path(&root).exists());
    }

    #[test]
    fn line_menu_new_then_quit_writes_the_starter() {
        use std::io::Cursor;
        let root = tmp_dir();
        let mut input = Cursor::new("new\nquit\n");
        let mut out = Vec::new();
        run_setup_lines(&root, true, None, &mut input, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("created: "), "{text}");
        assert_eq!(
            fs::read_to_string(project_config_path(&root)).unwrap(),
            STARTER_PROJECT_CONFIG
        );
    }

    #[test]
    fn line_menu_delete_asks_and_leaves_env() {
        use std::io::Cursor;
        let root = tmp_dir();
        create_project_config(&root).unwrap();
        fs::write(root.join(".env"), "XAI_API_KEY=unit-test-key\n").unwrap();
        let mut cancelled = Cursor::new("delete\nn\nquit\n");
        let mut out = Vec::new();
        run_setup_lines(&root, true, None, &mut cancelled, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains(">>> delete cancelled"), "{text}");
        assert!(project_config_path(&root).is_file());

        let mut confirmed = Cursor::new("delete\ny\nquit\n");
        let mut out = Vec::new();
        run_setup_lines(&root, true, None, &mut confirmed, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("deleted: "), "{text}");
        assert!(!project_config_path(&root).exists());
        assert!(root.join(".env").is_file());
    }

    #[test]
    fn line_menu_edit_reports_a_missing_file() {
        use std::io::Cursor;
        let root = tmp_dir();
        let mut input = Cursor::new("edit\n\n");
        let mut out = Vec::new();
        run_setup_lines(&root, true, None, &mut input, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("error: .symsight.toml is missing. Write it with `symsight create`."),
            "{text}"
        );
    }
}
