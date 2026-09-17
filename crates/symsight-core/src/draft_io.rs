// Copyright (c) 2026, Nathaniel Berry
// Licensed under the Apache License, Version 2.0.

//! Draft markdown read/write with hand-rolled YAML-ish front matter.
//!
//! Port of `src/symsight/draft_io.py`. Do not switch this codec to serde_yaml.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use chrono::Utc;
use indexmap::IndexMap;
use regex::Regex;

use crate::models::{ContentFormat, Draft, DraftMeta, FrontValue};
use crate::textutil::{char_count, is_safe_path_component, slugify, word_count};

static DISCLAIMER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)\n---\s*\n+\*\*Disclaimer\.\*\*.*$").expect("DISCLAIMER"));
static STATUS_QUOTED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?m)^status:\s*".*?""#).expect("STATUS_QUOTED"));
static STATUS_BARE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^status:\s*\S+").expect("STATUS_BARE"));

pub fn parse_front_matter(raw: &str) -> (IndexMap<String, FrontValue>, String) {
    if !raw.starts_with("---") {
        return (IndexMap::new(), raw.to_string());
    }
    let Some((fm_raw, body)) = split_front_matter(raw) else {
        return (IndexMap::new(), raw.to_string());
    };
    let mut meta = IndexMap::new();
    for line in fm_raw.trim().lines() {
        let Some((key, val)) = line.split_once(':') else {
            continue;
        };
        meta.insert(key.trim().to_string(), FrontValue::parse_scalar(val.trim()));
    }
    let body = body.trim_start_matches('\n').to_string();
    (meta, body)
}

fn split_front_matter(raw: &str) -> Option<(&str, &str)> {
    // Same as Python `raw.split("---", 2)`: at most two splits, three parts.
    let rest = raw.strip_prefix("---")?;
    let mid = rest.find("---")?;
    Some((&rest[..mid], &rest[mid + 3..]))
}

pub fn render_front_matter(front: &IndexMap<String, FrontValue>) -> String {
    let mut lines = vec!["---".to_string()];
    for (k, v) in front {
        lines.push(format!("{k}: {}", v.to_yq()));
    }
    lines.push("---".to_string());
    lines.push(String::new());
    lines.join("\n")
}

pub fn strip_disclaimer_from_body(body: &str) -> String {
    DISCLAIMER.replace(body, "").trim().to_string()
}

fn title_from_front(fm: &IndexMap<String, FrontValue>, stem: &str) -> String {
    match fm.get("title") {
        Some(v) if v.is_python_truthy() => v.as_display_string(),
        _ => stem.to_string(),
    }
}

pub fn read_draft(path: &Path) -> io::Result<Draft> {
    let raw = fs::read_to_string(path)?;
    let (fm, body) = parse_front_matter(&raw);
    let body = strip_disclaimer_from_body(&body);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("draft");
    let title = title_from_front(&fm, stem);
    let meta = {
        let meta_path = path.with_extension("meta.json");
        if meta_path.is_file() {
            fs::read_to_string(&meta_path)
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok())
        } else {
            None
        }
    };
    Ok(Draft {
        path: Some(path.to_path_buf()),
        title,
        body,
        front_matter: fm,
        meta,
    })
}

pub fn list_drafts(drafts_dir: &Path) -> Vec<Draft> {
    if !drafts_dir.is_dir() {
        return Vec::new();
    }
    let mut paths: Vec<PathBuf> = match fs::read_dir(drafts_dir) {
        Ok(entries) => entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("md"))
            .collect(),
        Err(_) => return Vec::new(),
    };
    paths.sort();
    paths.reverse();
    paths
        .into_iter()
        .filter_map(|path| read_draft(&path).ok())
        .collect()
}

pub fn write_draft_content(
    path: &Path,
    front: &IndexMap<String, FrontValue>,
    body: &str,
    disclaimer: Option<&str>,
) -> io::Result<()> {
    let mut content = render_front_matter(front);
    content.push_str(body.trim_end());
    if let Some(disc) = disclaimer.filter(|s| !s.is_empty()) {
        content.push_str("\n\n---\n\n**Disclaimer.** ");
        content.push_str(disc.trim());
        content.push('\n');
    } else {
        content.push('\n');
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)
}

pub fn save_draft_body(path: &Path, body: &str, disclaimer: Option<&str>) -> io::Result<Draft> {
    let draft = read_draft(path)?;
    let mut fm = draft.front_matter;
    let fmt = fm
        .get("format")
        .map(FrontValue::as_display_string)
        .unwrap_or_else(|| "article".to_string());
    if fmt == ContentFormat::Social.as_str() {
        fm.insert(
            "char_count".into(),
            FrontValue::Int(char_count(body) as i64),
        );
        fm.shift_remove("word_count");
    } else {
        fm.insert(
            "word_count".into(),
            FrontValue::Int(word_count(body) as i64),
        );
    }
    write_draft_content(path, &fm, body, disclaimer)?;
    read_draft(path)
}

pub fn set_status(path: &Path, status: &str) -> io::Result<()> {
    let raw = fs::read_to_string(path)?;
    let quoted = format!(r#"status: "{status}""#);
    let mut updated = STATUS_QUOTED.replace(&raw, quoted.as_str()).into_owned();
    if updated == raw {
        updated = STATUS_BARE.replace(&raw, quoted.as_str()).into_owned();
    }
    if updated == raw && raw.starts_with("---") {
        if let Some((fm, rest)) = split_front_matter(&raw) {
            let fm = format!("{}\nstatus: \"{status}\"\n", fm.trim_end());
            updated = format!("---{fm}---{rest}");
        }
    }
    fs::write(path, updated)
}

pub fn unique_draft_path(drafts_dir: &Path, stem: &str) -> io::Result<PathBuf> {
    if !is_safe_path_component(stem) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsafe draft stem: {stem:?}"),
        ));
    }
    fs::create_dir_all(drafts_dir)?;
    let mut path = drafts_dir.join(format!("{stem}.md"));
    let mut n = 2;
    while path.exists() {
        path = drafts_dir.join(format!("{stem}-{n}.md"));
        n += 1;
    }
    Ok(path)
}

/// Arguments for [`write_new_draft`].
pub struct WriteNewDraft<'a> {
    pub drafts_dir: &'a Path,
    pub title: &'a str,
    pub body: &'a str,
    pub brand_id: &'a str,
    pub brand_display: &'a str,
    pub type_id: &'a str,
    pub format: ContentFormat,
    pub topic: Option<&'a str>,
    pub disclaimer: Option<&'a str>,
    pub meta: &'a DraftMeta,
}

pub fn write_new_draft(req: WriteNewDraft<'_>) -> io::Result<PathBuf> {
    let date = Utc::now().format("%Y-%m-%d").to_string();
    let type_slug = slugify(req.type_id, 40);
    let stem = if req.format == ContentFormat::Social {
        let slug_src = if req.title.is_empty() {
            req.body.chars().take(40).collect::<String>()
        } else {
            req.title.to_string()
        };
        format!("{date}-social-{}", slugify(&slug_src, 60))
    } else {
        format!("{date}-{type_slug}-{}", slugify(req.title, 60))
    };
    let path = unique_draft_path(req.drafts_dir, &stem)?;

    let mut front = IndexMap::new();
    front.insert("title".into(), FrontValue::String(req.title.to_string()));
    front.insert("type".into(), FrontValue::String(req.type_id.to_string()));
    front.insert(
        "format".into(),
        FrontValue::String(req.format.as_str().to_string()),
    );
    front.insert("brand".into(), FrontValue::String(req.brand_id.to_string()));
    front.insert(
        "brand_name".into(),
        FrontValue::String(req.brand_display.to_string()),
    );
    front.insert(
        "generated_at".into(),
        FrontValue::String(req.meta.generated_at.clone()),
    );
    front.insert("status".into(), FrontValue::String("draft".into()));
    front.insert(
        "disclaimer".into(),
        FrontValue::Bool(req.disclaimer.is_some_and(|s| !s.is_empty())),
    );
    front.insert(
        "topic".into(),
        match req.topic {
            Some(t) => FrontValue::String(t.to_string()),
            None => FrontValue::Null,
        },
    );
    if req.format == ContentFormat::Social {
        front.insert(
            "char_count".into(),
            FrontValue::Int(char_count(req.body) as i64),
        );
    } else {
        front.insert(
            "word_count".into(),
            FrontValue::Int(word_count(req.body) as i64),
        );
    }

    write_draft_content(&path, &front, req.body, req.disclaimer)?;

    let mut meta_out = req.meta.clone();
    meta_out.title = Some(req.title.to_string());
    meta_out.path = Some(path.to_string_lossy().into_owned());
    let meta_path = path.with_extension("meta.json");
    fs::write(&meta_path, draft_meta_json(&meta_out)?)?;
    Ok(path)
}

pub fn write_meta_json(path: &Path, data: &serde_json::Value) -> io::Result<()> {
    let text = serde_json::to_string_pretty(data).map_err(io::Error::other)?;
    fs::write(path, text)
}

pub fn draft_meta_json(meta: &DraftMeta) -> io::Result<String> {
    serde_json::to_string_pretty(meta).map_err(io::Error::other)
}
