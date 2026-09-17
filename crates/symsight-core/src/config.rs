// Copyright (c) 2026, Nathaniel Berry
// Licensed under the Apache License, Version 2.0.

//! Application settings (`src/symsight/config.py`).
//!
//! Merge order matches `get_config`: CLI overrides and file keys are init
//! kwargs (they beat env). Env / `.env` only fill keys absent from that dict.
//! Do not treat `Cargo.toml` as a project-root marker.

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;

use crate::error::ConfigError;

pub const DEFAULT_MODEL: &str = "grok-4.5";
pub const DEFAULT_BASE_URL: &str = "https://api.x.ai/v1";

/// Walk up from `start` (or cwd) looking for `pyproject.toml`, `.symsight.toml`,
/// or `config/brands/`.
pub fn find_project_root(start: Option<&Path>) -> PathBuf {
    let cur = match start {
        Some(p) => abs_path(p),
        None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };
    let mut path = cur.clone();
    loop {
        if path.join("pyproject.toml").exists() || path.join(".symsight.toml").exists() {
            return path;
        }
        if path.join("config").join("brands").is_dir() {
            return path;
        }
        if !path.pop() {
            return cur;
        }
    }
}

fn abs_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(path)
}

fn user_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".config").join("symsight").join("config.toml"))
}

fn load_toml(path: &Path) -> toml::Table {
    let Ok(text) = fs::read_to_string(path) else {
        return toml::Table::new();
    };
    toml::from_str(&text).unwrap_or_default()
}

fn toml_key_order(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
                return None;
            }
            line.split_once('=')
                .map(|(k, _)| k.trim().to_string())
                .filter(|k| !k.is_empty())
        })
        .collect()
}

fn load_toml_ordered(path: &Path) -> IndexMap<String, toml::Value> {
    let table = load_toml(path);
    let text = fs::read_to_string(path).unwrap_or_default();
    let mut ordered = IndexMap::new();
    for key in toml_key_order(&text) {
        if let Some(v) = table.get(&key) {
            ordered.insert(key, v.clone());
        }
    }
    for (k, v) in table {
        ordered.entry(k).or_insert(v);
    }
    ordered
}

fn load_env_file(path: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(iter) = dotenvy::from_path_iter(path) else {
        return map;
    };
    for item in iter.flatten() {
        map.insert(item.0, item.1);
    }
    map
}

fn lookup(names: &[&str], dotenv: &HashMap<String, String>) -> Option<String> {
    for name in names {
        if let Ok(v) = std::env::var(name) {
            return Some(v);
        }
        if let Some(v) = dotenv.get(*name) {
            return Some(v.clone());
        }
    }
    None
}

fn toml_to_string(val: &toml::Value) -> Option<String> {
    match val {
        toml::Value::String(s) => Some(s.clone()),
        toml::Value::Integer(n) => Some(n.to_string()),
        toml::Value::Float(n) => Some(n.to_string()),
        toml::Value::Boolean(b) => Some(if *b { "true" } else { "false" }.to_string()),
        _ => None,
    }
}

pub fn load_config_file(root: Option<&Path>) -> toml::Table {
    let root = root
        .map(Path::to_path_buf)
        .unwrap_or_else(|| find_project_root(None));
    let mut merged = toml::Table::new();
    if let Some(user) = user_config_path() {
        for (k, v) in load_toml(&user) {
            merged.insert(k, v);
        }
    }
    for (k, v) in load_toml(&root.join(".symsight.toml")) {
        merged.insert(k, v);
    }
    merged
}

fn redact_key(value: &str) -> &'static str {
    if value.is_empty() {
        ""
    } else {
        "[redacted]"
    }
}

/// Optional CLI / caller overrides. `None` means “leave this key to files/env”.
#[derive(Clone, Default)]
pub struct ConfigOverrides {
    pub active_brand: Option<String>,
    pub brands_dir: Option<PathBuf>,
    pub drafts_dir: Option<PathBuf>,
    pub final_dir: Option<PathBuf>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub xai_api_key: Option<String>,
}

impl fmt::Debug for ConfigOverrides {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConfigOverrides")
            .field("active_brand", &self.active_brand)
            .field("brands_dir", &self.brands_dir)
            .field("drafts_dir", &self.drafts_dir)
            .field("final_dir", &self.final_dir)
            .field("model", &self.model)
            .field("base_url", &self.base_url)
            .field("xai_api_key", &self.xai_api_key.as_deref().map(redact_key))
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub xai_api_key: String,
    pub model: String,
    pub base_url: String,
    pub active_brand: String,
    pub brands_dir: PathBuf,
    pub drafts_dir: PathBuf,
    pub final_dir: PathBuf,
    pub project_root: PathBuf,
}

impl fmt::Debug for AppConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppConfig")
            .field("xai_api_key", &redact_key(&self.xai_api_key))
            .field("model", &self.model)
            .field("base_url", &self.base_url)
            .field("active_brand", &self.active_brand)
            .field("brands_dir", &self.brands_dir)
            .field("drafts_dir", &self.drafts_dir)
            .field("final_dir", &self.final_dir)
            .field("project_root", &self.project_root)
            .finish()
    }
}

impl AppConfig {
    pub fn resolve_paths(&self) -> Self {
        let root = abs_path(&self.project_root);
        let rel = |p: &Path| -> PathBuf {
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                root.join(p)
            }
        };
        Self {
            project_root: root.clone(),
            brands_dir: rel(&self.brands_dir),
            drafts_dir: rel(&self.drafts_dir),
            final_dir: rel(&self.final_dir),
            xai_api_key: self.xai_api_key.clone(),
            model: self.model.clone(),
            base_url: self.base_url.clone(),
            active_brand: self.active_brand.clone(),
        }
    }

    pub fn require_api_key(&self) -> Result<String, ConfigError> {
        let mut key = if self.xai_api_key.is_empty() {
            std::env::var("XAI_API_KEY").unwrap_or_default()
        } else {
            self.xai_api_key.clone()
        };
        key = key.trim().to_string();
        if key.is_empty() {
            let file = load_env_file(&self.project_root.join(".env"));
            key = lookup(&["XAI_API_KEY"], &file)
                .or_else(|| Some(self.xai_api_key.clone()))
                .unwrap_or_default()
                .trim()
                .to_string();
        }
        if key.is_empty() {
            return Err(ConfigError::MissingApiKey);
        }
        Ok(key)
    }
}

pub fn get_config(root: Option<&Path>, overrides: Option<&ConfigOverrides>) -> AppConfig {
    let root = match root {
        Some(p) => abs_path(p),
        None => find_project_root(None),
    };
    let cwd_env = std::env::current_dir()
        .map(|cwd| load_env_file(&cwd.join(".env")))
        .unwrap_or_default();
    let file_data = load_config_file(Some(&root));

    let from_file = |key: &str| -> Option<String> { file_data.get(key).and_then(toml_to_string) };

    let pick = |override_val: Option<String>, file_key: &str, env_names: &[&str], default: &str| {
        if let Some(v) = override_val {
            return v;
        }
        if let Some(v) = from_file(file_key) {
            return v;
        }
        lookup(env_names, &cwd_env).unwrap_or_else(|| default.to_string())
    };

    let ov = overrides.cloned().unwrap_or_default();

    let mut cfg = AppConfig {
        project_root: root,
        active_brand: pick(
            ov.active_brand,
            "active_brand",
            &["SYMSIGHT_ACTIVE_BRAND", "active_brand"],
            "example-writer",
        ),
        brands_dir: PathBuf::from(pick(
            ov.brands_dir.map(|p| p.to_string_lossy().into_owned()),
            "brands_dir",
            &["SYMSIGHT_BRANDS_DIR", "brands_dir"],
            "./config/brands",
        )),
        drafts_dir: PathBuf::from(pick(
            ov.drafts_dir.map(|p| p.to_string_lossy().into_owned()),
            "drafts_dir",
            &["SYMSIGHT_DRAFTS_DIR", "drafts_dir"],
            "./content/drafts",
        )),
        final_dir: PathBuf::from(pick(
            ov.final_dir.map(|p| p.to_string_lossy().into_owned()),
            "final_dir",
            &["SYMSIGHT_FINAL_DIR", "final_dir"],
            "./content/final",
        )),
        model: pick(
            ov.model,
            "model",
            &["SYMSIGHT_MODEL", "model"],
            DEFAULT_MODEL,
        ),
        base_url: pick(
            ov.base_url,
            "base_url",
            &["SYMSIGHT_BASE_URL", "base_url"],
            DEFAULT_BASE_URL,
        ),
        xai_api_key: {
            if let Some(v) = ov.xai_api_key {
                v
            } else {
                lookup(
                    &["XAI_API_KEY", "SYMSIGHT_XAI_API_KEY", "xai_api_key"],
                    &cwd_env,
                )
                .unwrap_or_default()
            }
        },
    };
    if cfg.xai_api_key.is_empty() {
        if let Ok(v) = std::env::var("XAI_API_KEY") {
            cfg.xai_api_key = v;
        }
    }
    cfg.resolve_paths()
}

pub fn save_project_config(cfg: &AppConfig, path: Option<&Path>) -> std::io::Result<PathBuf> {
    let root = &cfg.project_root;
    let out = path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.join(".symsight.toml"));
    let mut existing = if out.is_file() {
        load_toml_ordered(&out)
    } else {
        IndexMap::new()
    };

    let as_str = |p: &Path| -> String {
        p.strip_prefix(root)
            .map(|rel| rel.to_string_lossy().into_owned())
            .unwrap_or_else(|_| p.to_string_lossy().into_owned())
    };

    existing.insert(
        "active_brand".into(),
        toml::Value::String(cfg.active_brand.clone()),
    );
    existing.insert(
        "brands_dir".into(),
        toml::Value::String(as_str(&cfg.brands_dir)),
    );
    existing.insert(
        "drafts_dir".into(),
        toml::Value::String(as_str(&cfg.drafts_dir)),
    );
    existing.insert(
        "final_dir".into(),
        toml::Value::String(as_str(&cfg.final_dir)),
    );
    existing.insert("model".into(), toml::Value::String(cfg.model.clone()));
    existing.insert("base_url".into(), toml::Value::String(cfg.base_url.clone()));

    let mut lines = vec!["# symsight project config".to_string(), String::new()];
    for (key, val) in &existing {
        lines.push(format_toml_line(key, val));
    }
    lines.push(String::new());
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&out, lines.join("\n"))?;
    Ok(out)
}

fn format_toml_line(key: &str, val: &toml::Value) -> String {
    match val {
        toml::Value::Boolean(b) => format!("{key} = {}", if *b { "true" } else { "false" }),
        toml::Value::Integer(n) => format!("{key} = {n}"),
        toml::Value::Float(n) => format!("{key} = {n}"),
        toml::Value::String(s) => {
            let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
            format!("{key} = \"{escaped}\"")
        }
        other => {
            let escaped = other.to_string().replace('\\', "\\\\").replace('"', "\\\"");
            format!("{key} = \"{escaped}\"")
        }
    }
}
