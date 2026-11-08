// Copyright (c) 2026, Nathaniel T. Berry
// Licensed under the Apache License, Version 2.0.

//! Generation orchestration: prompt → LLM → validate → write draft.
//!
//! Port of `src/symsight/generate.py`. Tests inject [`crate::llm::ScriptedClient`];
//! [`crate::llm::XaiClient`] is only built when no client is supplied.
//!
//! The model turn goes through [`crate::tools::run_to_message`]. The registry
//! is empty, so a tool call stops instead of running. Attached files are read
//! locally and appended to the prompt.

use std::path::{Path, PathBuf};

use chrono::{SecondsFormat, Utc};

use crate::brandcheck::check_text;
use crate::config::AppConfig;
use crate::draft_io::{write_new_draft, WriteNewDraft};
use crate::error::GenerateError;
use crate::llm::{CompletionRequest, LlmClient, LlmSession, ModelTurn, OneShotSession, XaiClient};
use crate::models::{ContentFormat, DraftMeta, GenerateRequest};
use crate::prompts::{length_rewrite_prompt, system_prompt, user_prompt, utc_today};
use crate::textutil::{
    char_count, extract_social_text, extract_title_body, is_plausible_title, word_count,
};
use crate::tools::{context_block, load_context_files, run_to_message, ToolPolicy, ToolRegistry};

/// Return `(title, body, meta)`. Does not write files.
pub fn generate_content(
    req: &GenerateRequest,
    cfg: &AppConfig,
    client: Option<&dyn LlmClient>,
) -> Result<(String, String, DraftMeta), GenerateError> {
    // Resolve type and tone before the API key so a bad id fails closed.
    req.resolved_type()?;
    req.resolved_tone()?;
    let policy = ToolPolicy::for_project(&cfg.project_root);
    let context = load_context_files(&req.context_files, &policy)
        .map_err(|e| GenerateError::Message(e.to_string()))?;
    let registry = ToolRegistry::empty(policy);
    let context_note = context_block(&context);
    let context_paths: Vec<String> = context.iter().map(|file| file.path.clone()).collect();
    let api_key = cfg
        .require_api_key()
        .map_err(|e| GenerateError::Message(e.to_string()))?;
    let model = req.model.clone().unwrap_or_else(|| cfg.model.clone());
    let mut owned: Option<XaiClient> = if client.is_none() {
        Some(
            XaiClient::new(api_key, &cfg.base_url)
                .map_err(|e| GenerateError::Message(e.to_string()))?,
        )
    } else {
        None
    };
    let mut adapter = client.map(OneShotSession::new);
    let session: &mut dyn LlmSession = if let Some(adapter) = adapter.as_mut() {
        adapter
    } else if let Some(owned) = owned.as_mut() {
        owned
    } else {
        unreachable!("client or owned XaiClient")
    };

    let today = utc_today();
    let use_search = req.resolved_use_search()?;
    let sys_p = system_prompt(req, &today)?;
    let mut usr_p = user_prompt(req, &today)?;
    usr_p.push_str(&context_note);
    let raw = run_to_message(
        session,
        &CompletionRequest {
            model: model.clone(),
            system: sys_p.clone(),
            user: usr_p,
            use_search,
        },
        &registry,
    )
    .map_err(|e| GenerateError::Message(e.to_string()))?;
    if raw.trim().is_empty() {
        return Err(GenerateError::Empty);
    }

    let finish = FinishIn {
        model: &model,
        raw: &raw,
        use_search,
        sys_p: &sys_p,
        today: &today,
        context_note: &context_note,
        context_paths: &context_paths,
    };
    let (title, body, meta) = if req.format == ContentFormat::Social {
        finish_social(req, session, &finish)?
    } else {
        finish_article(req, session, &finish)?
    };

    let hits = check_text(&format!("{title}\n{body}"), &req.brand);
    if !hits.is_empty() {
        return Err(GenerateError::Forbidden {
            hits: python_list_repr(&hits),
        });
    }
    Ok((title, body, meta))
}

struct FinishIn<'a> {
    model: &'a str,
    raw: &'a str,
    use_search: bool,
    sys_p: &'a str,
    today: &'a str,
    context_note: &'a str,
    context_paths: &'a [String],
}

fn finish_article(
    req: &GenerateRequest,
    session: &mut dyn LlmSession,
    pass: &FinishIn<'_>,
) -> Result<(String, String, DraftMeta), GenerateError> {
    let (mut title, mut body) =
        extract_title_body(pass.raw).map_err(|e| GenerateError::Message(e.to_string()))?;
    let mut wc = word_count(&body);
    let min_w = req.resolved_min_words();
    let max_w = req.resolved_max_words();

    if wc < min_w as usize || wc > max_w as usize || !is_plausible_title(&title) {
        let adjust = length_rewrite_prompt(req, &title, &body, wc as i64, pass.today);
        let raw2 = fresh_message(
            session,
            &CompletionRequest {
                model: pass.model.to_string(),
                system: pass.sys_p.to_string(),
                user: format!("{adjust}{}", pass.context_note),
                use_search: false,
            },
        )?;
        if !raw2.trim().is_empty() {
            if let Ok((t, b)) = extract_title_body(&raw2) {
                title = t;
                body = b;
                wc = word_count(&body);
            }
        }
    }

    if !is_plausible_title(&title) {
        let fallback = req
            .topic
            .clone()
            .unwrap_or_else(|| python_title(&req.type_id.replace('-', " ")));
        title = python_title(fallback.trim());
        title = title.chars().take(80).collect();
        eprintln!(
            "Warning: model title looked like process chatter; using fallback: '{}'",
            title.replace('\'', "\\'")
        );
    }

    if wc < min_w as usize || wc > max_w as usize {
        eprintln!(
            "Warning: word count {wc} outside {min_w}–{max_w}. Edit the draft before finalizing."
        );
    }

    let meta = DraftMeta {
        model: pass.model.to_string(),
        type_id: req.type_id.clone(),
        format: ContentFormat::Article.as_str().to_string(),
        topic: req.topic.clone(),
        tone: req.resolved_tone()?.map(|(id, _)| id),
        brand_id: req.brand.id.clone(),
        used_web_search: pass.use_search,
        word_count: Some(wc as i64),
        char_count: None,
        generated_at: now_iso(),
        title: None,
        path: None,
        context_files: pass.context_paths.to_vec(),
    };
    Ok((title, body, meta))
}

fn finish_social(
    req: &GenerateRequest,
    session: &mut dyn LlmSession,
    pass: &FinishIn<'_>,
) -> Result<(String, String, DraftMeta), GenerateError> {
    let mut body =
        extract_social_text(pass.raw).map_err(|e| GenerateError::Message(e.to_string()))?;
    let mut cc = char_count(&body);
    let max_c = req.resolved_max_chars();

    if cc > max_c as usize {
        let adjust = length_rewrite_prompt(req, "", &body, cc as i64, pass.today);
        let raw2 = fresh_message(
            session,
            &CompletionRequest {
                model: pass.model.to_string(),
                system: pass.sys_p.to_string(),
                user: format!("{adjust}{}", pass.context_note),
                use_search: false,
            },
        )?;
        if !raw2.trim().is_empty() {
            if let Ok(b) = extract_social_text(&raw2) {
                body = b;
                cc = char_count(&body);
            }
        }
    }

    if cc > max_c as usize {
        eprintln!(
            "Warning: character count {cc} exceeds {max_c}. Edit the draft before finalizing."
        );
    }

    let first_line = body.trim().split('\n').next().unwrap_or("");
    let mut title: String = first_line.chars().take(80).collect();
    if title.is_empty() {
        title = "social-post".into();
    }

    let meta = DraftMeta {
        model: pass.model.to_string(),
        type_id: req.type_id.clone(),
        format: ContentFormat::Social.as_str().to_string(),
        topic: req.topic.clone(),
        tone: req.resolved_tone()?.map(|(id, _)| id),
        brand_id: req.brand.id.clone(),
        used_web_search: pass.use_search,
        word_count: None,
        char_count: Some(cc as i64),
        generated_at: now_iso(),
        title: None,
        path: None,
        context_files: pass.context_paths.to_vec(),
    };
    Ok((title, body, meta))
}

/// Length rewrite is a fresh call with no tools. A tool call here is an error.
fn fresh_message(
    session: &mut dyn LlmSession,
    req: &CompletionRequest,
) -> Result<String, GenerateError> {
    match session
        .complete(req, &[])
        .map_err(|e| GenerateError::Message(e.to_string()))?
    {
        ModelTurn::Message(text) => Ok(text),
        ModelTurn::Calls(_) => Err(GenerateError::Message(
            "length rewrite returned tool calls".into(),
        )),
    }
}

pub fn generate_and_write(
    req: &GenerateRequest,
    cfg: &AppConfig,
    drafts_dir: Option<&Path>,
    client: Option<&dyn LlmClient>,
) -> Result<PathBuf, GenerateError> {
    let (title, body, meta) = generate_content(req, cfg, client)?;
    let out_dir = drafts_dir.unwrap_or(&cfg.drafts_dir);
    let disclaimer = {
        let d = req.brand.disclaimer.trim();
        if d.is_empty() {
            None
        } else {
            Some(d)
        }
    };
    write_new_draft(WriteNewDraft {
        drafts_dir: out_dir,
        title: &title,
        body: &body,
        brand_id: &req.brand.id,
        brand_display: &req.brand.display_name,
        type_id: &req.type_id,
        format: req.format,
        topic: req.topic.as_deref(),
        disclaimer,
        meta: &meta,
    })
    .map_err(|e| GenerateError::Message(e.to_string()))
}

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Micros, false)
}

fn python_list_repr(items: &[String]) -> String {
    let inner = items
        .iter()
        .map(|s| format!("'{s}'"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{inner}]")
}

/// Approximate `str.title()` for hyphen-free fallback titles.
fn python_title(s: &str) -> String {
    s.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
