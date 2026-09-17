// Copyright (c) 2026, Nathaniel Berry
// Licensed under the Apache License, Version 2.0.

//! System / user / rewrite prompts (`src/symsight/prompts.py`).
//!
//! Template strings are copied verbatim, including the en-dash in article
//! length bounds.

use std::collections::HashMap;

use crate::error::GenerateError;
use crate::models::{ContentFormat, GenerateRequest};

/// Format a brand `user_template`, leaving unknown `{placeholders}` intact.
/// Invalid format strings (unbalanced braces) are returned unchanged.
pub fn safe_format(template: &str, kwargs: &HashMap<&str, String>) -> String {
    let chars: Vec<char> = template.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '{' => {
                if i + 1 < chars.len() && chars[i + 1] == '{' {
                    out.push('{');
                    i += 2;
                    continue;
                }
                let Some(rel_end) = chars[i + 1..].iter().position(|&c| c == '}') else {
                    return template.to_string();
                };
                let name: String = chars[i + 1..i + 1 + rel_end].iter().collect();
                if name.is_empty() || name.contains(['.', ':', '!', '{']) {
                    return template.to_string();
                }
                if let Some(val) = kwargs.get(name.as_str()) {
                    out.push_str(val);
                } else {
                    out.push('{');
                    out.push_str(&name);
                    out.push('}');
                }
                i += rel_end + 2;
            }
            '}' => {
                if i + 1 < chars.len() && chars[i + 1] == '}' {
                    out.push('}');
                    i += 2;
                } else {
                    return template.to_string();
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

pub fn system_prompt(req: &GenerateRequest) -> String {
    let name = req.brand.full_name.as_str();
    let voice = req.brand.voice.trim();
    if req.format == ContentFormat::Social {
        let max_chars = req.resolved_max_chars();
        return format!(
            "\
You are a writer for {name}.
Voice: {voice}

Hard rules:
- When naming the organization or byline, use \"{name}\" exactly if you name it.
- Do not invent credentials, team bios, or regulatory registrations.
- Output a single social post of at most {max_chars} characters.
- Do NOT narrate your process, planning, tool use, or revisions.
- Do NOT include markdown citation markers or footnotes.
- Do NOT output a TITLE line or preamble. Output ONLY the post text.

Output format — the entire response must be ONLY the post text, nothing else.
"
        );
    }
    let min_w = req.resolved_min_words();
    let max_w = req.resolved_max_words();
    format!(
        "\
You are a writer for {name}.
Voice: {voice}

Hard rules:
- Always refer to the organization as \"{name}\" when naming it. Never misspell or alter the name.
- Educational content only unless the brand voice says otherwise. No personalized recommendations.
- Do not invent credentials, AUM, team bios, or regulatory registrations.
- Body length: {min_w}–{max_w} words (title excluded).
- Prefer accurate information. For time-sensitive topics use search; if data is thin, say so—do not invent figures.
- Do NOT narrate your process, planning, tool use, or revisions. Do NOT include footnotes, citation markers,
  or markdown links like [[1]](url) or [1](url). Weave facts into prose only.
- Do NOT output anything before the TITLE line (no preamble).

Output format — the entire response must be ONLY these three parts, nothing else:

TITLE: <short one-line title, max ~80 characters>
---
<body as markdown paragraphs only; no H1; no second TITLE line; no trailing disclaimer>
"
    )
}

pub fn user_prompt(req: &GenerateRequest, today: &str) -> Result<String, GenerateError> {
    let type_spec = req.resolved_type()?;
    let topic = req
        .topic
        .clone()
        .unwrap_or_else(|| "a useful topic for the intended audience".to_string());
    let mut kwargs = HashMap::new();
    kwargs.insert("today", today.to_string());
    kwargs.insert("topic", topic);
    kwargs.insert("min_words", req.resolved_min_words().to_string());
    kwargs.insert("max_words", req.resolved_max_words().to_string());
    kwargs.insert("max_chars", req.resolved_max_chars().to_string());
    kwargs.insert("full_name", req.brand.full_name.clone());
    kwargs.insert("short_name", req.brand.short_name.clone());
    kwargs.insert("display_name", req.brand.display_name.clone());
    let mut base = safe_format(&type_spec.user_template, &kwargs);
    if req.format == ContentFormat::Social
        && !base.to_lowercase().contains("character")
        && !type_spec.user_template.contains("max_chars")
    {
        base = format!(
            "{}\nHard limit: at most {} characters. Output ONLY the post text.",
            base.trim_end(),
            req.resolved_max_chars()
        );
    }
    Ok(base)
}

pub fn length_rewrite_prompt(
    req: &GenerateRequest,
    title: &str,
    body: &str,
    current_count: i64,
) -> String {
    if req.format == ContentFormat::Social {
        let max_c = req.resolved_max_chars();
        return format!(
            "Rewrite the social post below to at most {max_c} characters. \
             Output ONLY the post text with no preamble, no quotes, no title.\n\n\
             Current character count: {current_count}.\n\n\
             {body}"
        );
    }
    let min_w = req.resolved_min_words();
    let max_w = req.resolved_max_words();
    format!(
        "Rewrite the essay below to {min_w}–{max_w} words. \
         Output ONLY this format with no preamble, no citations, no process notes:\n\
         TITLE: <short title>\n---\n<body>\n\n\
         Current word count: {current_count}.\n\n\
         TITLE: {title}\n---\n{body}"
    )
}
