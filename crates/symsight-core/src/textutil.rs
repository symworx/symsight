// Copyright (c) 2026, Nathaniel T. Berry
// Licensed under the Apache License, Version 2.0.

//! Text helpers: counting, slugify, title/body extraction.
//!
//! Regexes are ported literally from `src/symsight/textutil.py`.

use std::sync::LazyLock;

use regex::Regex;

use crate::error::TextError;

static BAD_TITLE_HINTS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(i am revising|i will |i need |let me |here'?s |okay[,.]|sure[,.]|verify |searching|tool call|web_search|as an ai|revising the essay|preserving the required|format and voice)\b",
    )
    .expect("BAD_TITLE_HINTS")
});
static CITATION_MD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[\d+\]\]\([^)]+\)").expect("CITATION_MD"));
static CITATION_SIMPLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[(\d+)\]\((https?://[^)]+)\)").expect("CITATION_SIMPLE"));
static BARE_FOOTNOTE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[[\d]+\]\]").expect("BARE_FOOTNOTE"));
static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b[\w']+\b").expect("WORD"));
static NON_SLUG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^a-z0-9]+").expect("NON_SLUG"));
static TRAIL_WS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t]+\n").expect("TRAIL_WS"));
static MULTI_SPACE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[ \t]{2,}").expect("MULTI_SPACE"));
static SPACE_DOT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r" +\.").expect("SPACE_DOT"));
static OPEN_FENCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^```(?:\w+)?\n?").expect("OPEN_FENCE"));
static CLOSE_FENCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\n?```$").expect("CLOSE_FENCE"));
static TITLE_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?im)^TITLE:\s*(.+)$").expect("TITLE_LINE"));
// Python: (?i)\bTITLE:\s*(.+?)(?=\n|$). The regex crate has no look-ahead;
// without DOTALL, `.+` already stops at the first newline or EOS.
static TITLE_INLINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bTITLE:\s*(.+)").expect("TITLE_INLINE"));
static SPLIT_TITLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bTITLE:\s*").expect("SPLIT_TITLE"));
static SPLIT_JUNK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[|\s{2,}").expect("SPLIT_JUNK"));
static HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^#+\s*").expect("HEADING"));
static WHITESPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").expect("WHITESPACE"));
static SOCIAL_PREAMBLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)^(here(?:'s| is).*?:\s*)").expect("SOCIAL_PREAMBLE"));
static PARAS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n\s*\n").expect("PARAS"));
static RULE_SEP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^-{3,}\s*\n?").expect("RULE_SEP"));

pub fn word_count(text: &str) -> usize {
    WORD.find_iter(text).count()
}

pub fn char_count(text: &str) -> usize {
    text.chars().count()
}

/// True when `name` is a single relative path segment (no `/`, `..`, or drive).
pub fn is_safe_path_component(name: &str) -> bool {
    if name.is_empty() || name == "." || name == ".." {
        return false;
    }
    if name.contains('/') || name.contains('\\') || name.contains('\0') || name.contains(':') {
        return false;
    }
    true
}

pub fn slugify(text: &str, max_len: usize) -> String {
    let lower = text.to_lowercase();
    let trimmed = lower.trim();
    let dashed = NON_SLUG.replace_all(trimmed, "-");
    let stripped = dashed.trim_matches('-');
    let sliced: String = stripped.chars().take(max_len).collect();
    if sliced.is_empty() {
        "insight".to_string()
    } else {
        sliced
    }
}

pub fn clean_body(body: &str) -> String {
    let mut text = body.trim().to_string();
    while text.starts_with("---") {
        let rest = &text[3..];
        text = rest
            .trim_start_matches(['\n', '\r', '-', ' '])
            .trim_start()
            .to_string();
    }
    let text = CITATION_MD.replace_all(&text, "");
    let text = CITATION_SIMPLE.replace_all(&text, "");
    let text = BARE_FOOTNOTE.replace_all(&text, "");
    let text = TRAIL_WS.replace_all(&text, "\n");
    let text = MULTI_SPACE.replace_all(&text, " ");
    let text = SPACE_DOT.replace_all(&text, ".");
    text.trim().to_string()
}

pub fn is_plausible_title(title: &str) -> bool {
    let t = title.trim();
    if t.is_empty() || t.chars().count() > 120 {
        return false;
    }
    if t.contains("http://") || t.contains("https://") || t.contains("[[") {
        return false;
    }
    if BAD_TITLE_HINTS.is_match(t) {
        return false;
    }
    !(t.matches('.').count() >= 2 && t.chars().count() > 80)
}

fn strip_fence(raw: &str) -> String {
    let mut text = raw.trim().to_string();
    if text.starts_with("```") {
        text = OPEN_FENCE.replace(&text, "").into_owned();
        text = CLOSE_FENCE.replace(&text, "").into_owned();
        text = text.trim().to_string();
    }
    text
}

fn peel_nested_title(candidate: &str) -> String {
    if candidate.to_uppercase().contains("TITLE:") {
        SPLIT_TITLE
            .split(candidate)
            .last()
            .unwrap_or(candidate)
            .trim()
            .to_string()
    } else {
        candidate.to_string()
    }
}

pub fn extract_title_body(raw: &str) -> Result<(String, String), TextError> {
    let text = strip_fence(raw);

    let mut title_hits: Vec<(usize, String)> = TITLE_LINE
        .captures_iter(&text)
        .filter_map(|caps| Some((caps.get(0)?.end(), caps.get(1)?.as_str().trim().to_string())))
        .collect();
    if title_hits.is_empty() {
        title_hits = TITLE_INLINE
            .captures_iter(&text)
            .filter_map(|caps| Some((caps.get(0)?.end(), caps.get(1)?.as_str().trim().to_string())))
            .collect();
    }

    let mut title: Option<String> = None;
    let mut body_start = 0usize;

    if !title_hits.is_empty() {
        let mut chosen = title_hits.len() - 1;
        for (idx, (_end, candidate)) in title_hits.iter().enumerate().rev() {
            let candidate = peel_nested_title(candidate);
            if is_plausible_title(&candidate) {
                chosen = idx;
                title = Some(candidate);
                break;
            }
        }
        if title.is_none() {
            let mut candidate = peel_nested_title(&title_hits[chosen].1);
            candidate = SPLIT_JUNK
                .split(&candidate)
                .next()
                .unwrap_or(&candidate)
                .trim()
                .to_string();
            let sliced: String = candidate.chars().take(120).collect();
            let sliced = sliced.trim();
            title = Some(if sliced.is_empty() {
                "Untitled insight".to_string()
            } else {
                sliced.to_string()
            });
        }
        body_start = title_hits[chosen].0;
    }

    let rest = if title.is_some() {
        text[body_start..].trim_start()
    } else {
        text.as_str()
    };
    let rest = if let Some(sep) = RULE_SEP.find(rest) {
        &rest[sep.end()..]
    } else {
        rest
    };
    let mut body = clean_body(rest);

    if title.is_none() {
        let lines: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|ln| !ln.is_empty())
            .collect();
        if lines.is_empty() {
            return Err(TextError::EmptyTitleBody);
        }
        let first = HEADING.replace(lines[0], "").trim().to_string();
        if is_plausible_title(&first) {
            title = Some(first);
            body = clean_body(&lines[1..].join("\n"));
        } else {
            title = Some("Untitled insight".to_string());
            body = clean_body(&lines.join("\n"));
        }
    }

    let title = title.expect("title set");
    let title = WHITESPACE.replace_all(&title, " ");
    let title = title
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string();
    if body.is_empty() {
        return Err(TextError::EmptyBody);
    }
    Ok((title, body))
}

pub fn extract_social_text(raw: &str) -> Result<String, TextError> {
    let mut text = strip_fence(raw);
    text = SOCIAL_PREAMBLE.replace(&text, "").into_owned();
    let text = clean_body(&text);
    let paras: Vec<&str> = PARAS
        .split(&text)
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    if !paras.is_empty() {
        if paras.len() > 1 && paras[0].to_uppercase().starts_with("TITLE:") {
            return Ok(paras[1].trim().to_string());
        }
        return Ok(paras[0].trim().to_string());
    }
    if text.is_empty() {
        return Err(TextError::EmptySocial);
    }
    Ok(text)
}
