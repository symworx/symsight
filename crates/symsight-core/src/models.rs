// Copyright (c) 2026, Nathaniel T. Berry
// Licensed under the Apache License, Version 2.0.

//! Domain types: brands, generation requests, and drafts.
//!
//! Field-for-field map of `src/symsight/models.py`. On-disk brand YAML is the
//! contract; there is no schema change.

use std::path::PathBuf;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::error::GenerateError;

fn default_min_words() -> i64 {
    200
}

fn default_max_words() -> i64 {
    500
}

fn default_max_chars() -> i64 {
    200
}

/// Output format for a generate request.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentFormat {
    #[default]
    Article,
    Social,
}

impl ContentFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Article => "article",
            Self::Social => "social",
        }
    }
}

impl std::fmt::Display for ContentFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One content type under a brand (e.g. general, news).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeSpec {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub default_use_search: bool,
    pub user_template: String,
}

/// Article length defaults on a brand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArticleFormatSpec {
    #[serde(default = "default_min_words")]
    pub default_min_words: i64,
    #[serde(default = "default_max_words")]
    pub default_max_words: i64,
}

impl Default for ArticleFormatSpec {
    fn default() -> Self {
        Self {
            default_min_words: default_min_words(),
            default_max_words: default_max_words(),
        }
    }
}

/// Social length defaults on a brand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SocialFormatSpec {
    #[serde(default = "default_max_chars")]
    pub default_max_chars: i64,
}

impl Default for SocialFormatSpec {
    fn default() -> Self {
        Self {
            default_max_chars: default_max_chars(),
        }
    }
}

/// Per-format defaults on a brand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FormatSpecs {
    #[serde(default)]
    pub article: ArticleFormatSpec,
    #[serde(default)]
    pub social: SocialFormatSpec,
}

/// Brand identity and prompt templates — loaded only from YAML config.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Brand {
    pub id: String,
    pub display_name: String,
    pub voice: String,
    pub full_name: String,
    #[serde(default)]
    pub short_name: String,
    #[serde(default)]
    pub forbidden: Vec<String>,
    #[serde(default)]
    pub disclaimer: String,
    /// Insertion-ordered. CLI default type is the first key.
    #[serde(default)]
    pub types: IndexMap<String, TypeSpec>,
    #[serde(default)]
    pub formats: FormatSpecs,
}

impl Brand {
    /// Mirrors `Brand.model_post_init`: empty `short_name` → first token of
    /// `full_name`, else `id`.
    pub fn finalize_short_name(&mut self) {
        if self.short_name.is_empty() {
            self.short_name = self
                .full_name
                .split_whitespace()
                .next()
                .map(str::to_string)
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| self.id.clone());
        }
    }
}

/// In-memory generate request (not loaded from YAML).
#[derive(Debug, Clone)]
pub struct GenerateRequest {
    pub brand: Brand,
    pub type_id: String,
    pub format: ContentFormat,
    pub topic: Option<String>,
    pub min_words: Option<i64>,
    pub max_words: Option<i64>,
    pub max_chars: Option<i64>,
    pub use_search: Option<bool>,
    pub model: Option<String>,
}

impl GenerateRequest {
    pub fn resolved_type(&self) -> Result<&TypeSpec, GenerateError> {
        self.brand.types.get(&self.type_id).ok_or_else(|| {
            let known = if self.brand.types.is_empty() {
                "(none)".to_string()
            } else {
                let mut keys: Vec<&str> = self.brand.types.keys().map(String::as_str).collect();
                keys.sort_unstable();
                keys.join(", ")
            };
            GenerateError::UnknownType {
                type_id: self.type_id.clone(),
                brand_id: self.brand.id.clone(),
                known,
            }
        })
    }

    pub fn resolved_min_words(&self) -> i64 {
        self.min_words
            .unwrap_or(self.brand.formats.article.default_min_words)
    }

    pub fn resolved_max_words(&self) -> i64 {
        self.max_words
            .unwrap_or(self.brand.formats.article.default_max_words)
    }

    pub fn resolved_max_chars(&self) -> i64 {
        self.max_chars
            .unwrap_or(self.brand.formats.social.default_max_chars)
    }

    pub fn resolved_use_search(&self) -> Result<bool, GenerateError> {
        if let Some(flag) = self.use_search {
            return Ok(flag);
        }
        Ok(self.resolved_type()?.default_use_search)
    }
}

/// Sidecar `.meta.json`. Serialize all fields, including nulls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DraftMeta {
    pub model: String,
    #[serde(rename = "type")]
    pub type_id: String,
    pub format: String,
    pub topic: Option<String>,
    pub brand_id: String,
    pub used_web_search: bool,
    pub word_count: Option<i64>,
    pub char_count: Option<i64>,
    pub generated_at: String,
    pub title: Option<String>,
    pub path: Option<String>,
}

/// Scalar stored in hand-rolled draft front matter. Not a YAML AST.
#[derive(Debug, Clone, PartialEq)]
pub enum FrontValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

impl FrontValue {
    pub fn as_display_string(&self) -> String {
        match self {
            Self::Null => "null".to_string(),
            Self::Bool(true) => "true".to_string(),
            Self::Bool(false) => "false".to_string(),
            Self::Int(n) => n.to_string(),
            Self::Float(n) => n.to_string(),
            Self::String(s) => s.clone(),
        }
    }

    /// Python truthiness for `fm.get(key) or fallback`.
    pub fn is_python_truthy(&self) -> bool {
        match self {
            Self::Null => false,
            Self::Bool(b) => *b,
            Self::Int(0) => false,
            Self::Int(_) => true,
            Self::Float(n) => *n != 0.0,
            Self::String(s) => !s.is_empty(),
        }
    }

    /// `_yq` from `draft_io.py`.
    pub fn to_yq(&self) -> String {
        match self {
            Self::Null => "null".to_string(),
            Self::Bool(true) => "true".to_string(),
            Self::Bool(false) => "false".to_string(),
            Self::Int(n) => n.to_string(),
            Self::Float(n) => n.to_string(),
            Self::String(s) => format!("\"{}\"", s.replace('"', "\\\"")),
        }
    }

    pub fn parse_scalar(val: &str) -> Self {
        if val == "true" {
            Self::Bool(true)
        } else if val == "false" {
            Self::Bool(false)
        } else if val == "null" {
            Self::Null
        } else if INT_SCALAR.is_match(val) {
            val.parse::<i64>()
                .map(Self::Int)
                .unwrap_or_else(|_| Self::String(val.to_string()))
        } else if val.starts_with('"') && val.ends_with('"') && val.len() >= 2 {
            let inner = &val[1..val.len() - 1];
            Self::String(inner.replace("\\\"", "\""))
        } else {
            Self::String(val.to_string())
        }
    }
}

static INT_SCALAR: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"^-?\d+$").expect("INT_SCALAR"));

/// In-memory draft representation.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    pub path: Option<PathBuf>,
    pub title: String,
    pub body: String,
    pub front_matter: IndexMap<String, FrontValue>,
    pub meta: Option<DraftMeta>,
}

impl Draft {
    pub fn status(&self) -> String {
        self.front_matter
            .get("status")
            .map(FrontValue::as_display_string)
            .unwrap_or_else(|| "draft".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_status_defaults_to_draft() {
        let draft = Draft {
            path: None,
            title: "T".into(),
            body: "B".into(),
            front_matter: IndexMap::new(),
            meta: None,
        };
        assert_eq!(draft.status(), "draft");
    }

    #[test]
    fn draft_status_reads_front_matter() {
        let mut front = IndexMap::new();
        front.insert("status".into(), FrontValue::String("final".into()));
        let draft = Draft {
            path: None,
            title: "T".into(),
            body: "B".into(),
            front_matter: front,
            meta: None,
        };
        assert_eq!(draft.status(), "final");
    }

    #[test]
    fn draft_meta_json_includes_nulls_and_renames_type() {
        let meta = DraftMeta {
            model: "grok-4.5".into(),
            type_id: "general".into(),
            format: "article".into(),
            topic: None,
            brand_id: "fixture-co".into(),
            used_web_search: false,
            word_count: Some(12),
            char_count: None,
            generated_at: "2026-08-15T00:00:00.000000+00:00".into(),
            title: None,
            path: None,
        };
        let json = serde_json::to_string_pretty(&meta).expect("serialize");
        assert!(json.contains("\"type\": \"general\""));
        assert!(json.contains("\"topic\": null"));
        assert!(json.contains("\"char_count\": null"));
        assert!(json.contains("\"word_count\": 12"));
        assert!(!json.contains("type_id"));
    }
}
