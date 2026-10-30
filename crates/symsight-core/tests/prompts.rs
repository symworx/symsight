// Copyright (c) 2026, Nathaniel T. Berry
// Licensed under the Apache License, Version 2.0.

//! Prompt snapshots against current Python `symsight.prompts`.

use std::collections::HashMap;
use std::path::PathBuf;

use symsight_core::{
    length_rewrite_prompt, load_brand_file, safe_format, system_prompt, user_prompt, ContentFormat,
    GenerateError, GenerateRequest, ToneSpec,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn golden(name: &str) -> String {
    let path = repo_root().join("tests/golden/prompts").join(name);
    std::fs::read_to_string(path).unwrap()
}

fn fixture_req(type_id: &str, format: ContentFormat) -> GenerateRequest {
    let brand = load_brand_file(repo_root().join("tests/fixtures/fixture-co.yaml")).unwrap();
    GenerateRequest {
        brand,
        type_id: type_id.into(),
        format,
        topic: Some("testing".into()),
        min_words: None,
        max_words: None,
        max_chars: None,
        use_search: None,
        model: None,
        tone: None,
    }
}

#[test]
fn article_prompts_match_python() {
    let req = fixture_req("general", ContentFormat::Article);
    assert_eq!(
        system_prompt(&req, "2026-08-05").unwrap(),
        golden("article_system.txt")
    );
    assert_eq!(
        user_prompt(&req, "2026-08-05").unwrap(),
        golden("article_user.txt")
    );
    assert_eq!(
        length_rewrite_prompt(&req, "T", "Body text.", 2, "2026-08-05"),
        golden("article_rewrite.txt").trim_end_matches('\n')
    );
}

#[test]
fn social_prompts_match_python() {
    let mut req = fixture_req("social-tip", ContentFormat::Social);
    req.topic = Some("hydration".into());
    req.max_chars = Some(180);
    assert_eq!(
        system_prompt(&req, "2026-08-05").unwrap(),
        golden("social_system.txt")
    );
    assert_eq!(
        user_prompt(&req, "2026-08-05").unwrap(),
        golden("social_user.txt")
    );
    assert_eq!(
        length_rewrite_prompt(&req, "", "A post", 6, "2026-08-05"),
        golden("social_rewrite.txt").trim_end_matches('\n')
    );
}

#[test]
fn social_without_char_mention_appends_hard_limit() {
    let mut req = fixture_req("social-tip", ContentFormat::Social);
    req.topic = Some("x".into());
    req.type_id = "bare".into();
    req.brand.types.insert("bare".into(), {
        let mut spec = req.brand.types["social-tip"].clone();
        spec.user_template = "Write about {topic} today {today}.".into();
        spec
    });
    assert_eq!(
        user_prompt(&req, "2026-08-05").unwrap(),
        golden("bare_social_user.txt").trim_end_matches('\n')
    );
}

#[test]
fn tone_and_template_enter_the_prompt() {
    let mut req = fixture_req("general", ContentFormat::Article);
    req.brand.tones.insert(
        "briefing".into(),
        ToneSpec {
            description: "Lead".into(),
            instruction: "Open with the fact.\n".into(),
        },
    );
    req.brand.default_tone = "briefing".into();
    req.brand.formats.article.template = "On {today}, explain {topic} for {tone}.\n".into();

    let sys = system_prompt(&req, "2026-08-05").unwrap();
    assert!(sys.contains("Tone (briefing): Open with the fact."));
    assert!(sys.contains("Article template"));
    assert!(sys.contains("On 2026-08-05, explain testing for briefing."));
    assert!(sys.contains("Voice: Neutral test voice. Plain English."));

    req.brand.types.get_mut("general").unwrap().user_template =
        "Outline:\n{article_template}\nTone {tone}.".into();
    let placed = system_prompt(&req, "2026-08-05").unwrap();
    assert!(!placed.contains("Article template"));
    let usr = user_prompt(&req, "2026-08-05").unwrap();
    assert!(usr.contains("On 2026-08-05, explain testing for briefing."));
    assert!(usr.contains("Tone briefing."));

    req.format = ContentFormat::Social;
    req.type_id = "social-tip".into();
    let social = system_prompt(&req, "2026-08-05").unwrap();
    assert!(social.contains("Tone (briefing): Open with the fact."));
    assert!(!social.contains("Article template"));
    assert!(!social.contains("On 2026-08-05"));

    let rewrite = length_rewrite_prompt(&req, "T", "Body", 2, "2026-08-05");
    assert!(rewrite.contains("Keep the briefing tone: Open with the fact."));
    assert!(!rewrite.contains("Keep this article template:"));

    req.format = ContentFormat::Article;
    req.type_id = "general".into();
    let article_rewrite = length_rewrite_prompt(&req, "T", "Body", 2, "2026-08-05");
    assert!(article_rewrite.contains("Keep this article template:"));
    assert!(article_rewrite.contains("On 2026-08-05, explain testing for briefing."));
    assert!(article_rewrite.contains("Current word count: 2."));

    req.tone = Some("missing".into());
    match system_prompt(&req, "2026-08-05").unwrap_err() {
        GenerateError::UnknownTone { tone_id, .. } => assert_eq!(tone_id, "missing"),
        other => panic!("unexpected {other}"),
    }
}

#[test]
fn safe_format_keeps_unknown_and_rejects_unbalanced() {
    let mut kwargs = HashMap::new();
    kwargs.insert("today", "2026-08-05".into());
    assert_eq!(
        safe_format("Today is {today} about {topic}.", &kwargs),
        "Today is 2026-08-05 about {topic}."
    );
    assert_eq!(safe_format("broken {", &kwargs), "broken {");
}
