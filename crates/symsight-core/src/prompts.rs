// Copyright (c) 2026, Nathaniel T. Berry
// Licensed under the Apache License, Version 2.0.

//! System / user / rewrite prompts (`src/symsight/prompts.py`).
//!
//! Template strings are copied verbatim, including the en-dash in article
//! length bounds.

use std::collections::HashMap;

use crate::error::GenerateError;
use crate::models::{ContentFormat, GenerateRequest};

/// UTC calendar day used when a caller does not supply `{today}`.
pub fn utc_today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

struct PromptContext {
    voice: String,
    tone_id: Option<String>,
    tone_instruction: String,
    /// Formatted article outline, or empty when the brand has none.
    article_template: String,
    kwargs: HashMap<&'static str, String>,
}

fn prompt_context(req: &GenerateRequest, today: &str) -> Result<PromptContext, GenerateError> {
    let tone = req.resolved_tone()?;
    let (tone_id, tone_instruction) = match tone {
        Some((id, spec)) => (Some(id), spec.instruction.trim().to_string()),
        None => (None, String::new()),
    };
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
    kwargs.insert("voice", req.brand.voice.trim().to_string());
    kwargs.insert(
        "tone",
        tone_id.clone().unwrap_or_default(),
    );
    kwargs.insert("tone_instruction", tone_instruction.clone());
    let article_template = safe_format(&req.brand.formats.article.template, &kwargs)
        .trim()
        .to_string();
    kwargs.insert("article_template", article_template.clone());
    Ok(PromptContext {
        voice: req.brand.voice.trim().to_string(),
        tone_id,
        tone_instruction,
        article_template,
        kwargs,
    })
}

/// Blank line after `Voice:` when no tone is selected, so existing prompts stay put.
fn tone_gap(ctx: &PromptContext) -> String {
    match &ctx.tone_id {
        Some(id) => format!("\nTone ({id}): {}\n\n", ctx.tone_instruction),
        None => "\n".to_string(),
    }
}

fn inject_article_template(req: &GenerateRequest, ctx: &PromptContext) -> Result<bool, GenerateError> {
    if req.format != ContentFormat::Article || ctx.article_template.is_empty() {
        return Ok(false);
    }
    let user_template = &req.resolved_type()?.user_template;
    Ok(!user_template.contains("{article_template}"))
}

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

pub fn system_prompt(req: &GenerateRequest, today: &str) -> Result<String, GenerateError> {
    let ctx = prompt_context(req, today)?;
    let name = req.brand.full_name.as_str();
    let voice = ctx.voice.as_str();
    let tone_gap = tone_gap(&ctx);
    if req.format == ContentFormat::Social {
        let max_chars = req.resolved_max_chars();
        return Ok(format!(
            "\
You are a writer for {name}.
Voice: {voice}
{tone_gap}Hard rules:
- When naming the organization or byline, use \"{name}\" exactly if you name it.
- Do not invent credentials, team bios, or regulatory registrations.
- Output a single social post of at most {max_chars} characters.
- Do NOT narrate your process, planning, tool use, or revisions.
- Do NOT include markdown citation markers or footnotes.
- Do NOT output a TITLE line or preamble. Output ONLY the post text.

Output format — the entire response must be ONLY the post text, nothing else.
"
        ));
    }
    let min_w = req.resolved_min_words();
    let max_w = req.resolved_max_words();
    let mut prompt = format!(
        "\
You are a writer for {name}.
Voice: {voice}
{tone_gap}Hard rules:
- Always refer to the organization as \"{name}\" when naming it. Never misspell or alter the name.
- Educational content only unless the brand voice says otherwise. No personalized recommendations.
- Do not invent credentials, AUM, team bios, or regulatory registrations.
- Body length: {min_w}–{max_w} words (title excluded).
- Prefer accurate information. For time-sensitive topics use search; if data is thin, say so—do not invent figures.
- Prefer precise, concrete language. Avoid ambiguous or hedge-filled phrases such as \"it depends\", \"various factors\", \"some people say\", \"can be useful\", \"might help\", or \"it is important to note\" unless the uncertainty is the subject and is named explicitly.
- Do NOT narrate your process, planning, tool use, or revisions. Do NOT include footnotes, citation markers,
  or markdown links like [[1]](url) or [1](url). Weave facts into prose only.
- Do NOT output anything before the TITLE line (no preamble).

Output format — the entire response must be ONLY these parts, nothing else:

TITLE: <engaging, specific one-line title that draws the reader in without hype; max ~80 characters>
---
**TL;DR:** <1–2 concrete sentences that state what happened or what the reader will accomplish. Enough for a reader to decide whether to continue. No vague language.>
<body as markdown paragraphs only; no H1; no second TITLE line; no trailing disclaimer; the TL;DR stays at the top of the body>
"
    );
    if inject_article_template(req, &ctx)? {
        prompt.push('\n');
        prompt.push_str(
            "Article template — shape the body to this outline. Keep the TITLE / --- envelope above. Fill the sections that are listed.\n\n",
        );
        prompt.push_str(ctx.article_template.trim_end());
        prompt.push('\n');
    }
    Ok(prompt)
}

pub fn user_prompt(req: &GenerateRequest, today: &str) -> Result<String, GenerateError> {
    let type_spec = req.resolved_type()?;
    let ctx = prompt_context(req, today)?;
    let mut base = safe_format(&type_spec.user_template, &ctx.kwargs);
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

fn rewrite_guidance(req: &GenerateRequest, today: &str) -> String {
    let Ok(ctx) = prompt_context(req, today) else {
        return String::new();
    };
    let mut lines = Vec::new();
    if let Some(id) = &ctx.tone_id {
        lines.push(format!(
            "Keep the {id} tone: {}",
            ctx.tone_instruction
        ));
    }
    if req.format == ContentFormat::Article && !ctx.article_template.is_empty() {
        lines.push(format!(
            "Keep this article template:\n{}",
            ctx.article_template.trim_end()
        ));
    }
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n\n", lines.join("\n\n"))
    }
}

pub fn length_rewrite_prompt(
    req: &GenerateRequest,
    title: &str,
    body: &str,
    current_count: i64,
    today: &str,
) -> String {
    let guidance = rewrite_guidance(req, today);
    if req.format == ContentFormat::Social {
        let max_c = req.resolved_max_chars();
        let mut text = format!(
            "Rewrite the social post below to at most {max_c} characters. \
             Output ONLY the post text with no preamble, no quotes, no title.\n\n\
             Current character count: {current_count}.\n\n\
             {body}"
        );
        if !guidance.is_empty() {
            if let Some(idx) = text.find("Current character count:") {
                text.insert_str(idx, &guidance);
            }
        }
        return text;
    }
    let min_w = req.resolved_min_words();
    let max_w = req.resolved_max_words();
    let mut text = format!(
        "Rewrite the essay below to {min_w}–{max_w} words. \
         Output ONLY this format with no preamble, no citations, no process notes:\n\
         TITLE: <engaging, specific one-line title>\n---\n**TL;DR:** <1–2 concrete sentences stating what happened or what the reader will accomplish>\n<body>\n\n\
         Current word count: {current_count}.\n\n\
         TITLE: {title}\n---\n{body}"
    );
    if !guidance.is_empty() {
        if let Some(idx) = text.find("Current word count:") {
            text.insert_str(idx, &guidance);
        }
    }
    text
}
