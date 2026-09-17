// Copyright (c) 2026, Nathaniel Berry
// Licensed under the Apache License, Version 2.0.

//! Prompt snapshots against current Python `symsight.prompts`.

use std::collections::HashMap;
use std::path::PathBuf;

use symsight_core::{
    length_rewrite_prompt, load_brand_file, safe_format, system_prompt, user_prompt, ContentFormat,
    GenerateRequest,
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
    }
}

#[test]
fn article_prompts_match_python() {
    let req = fixture_req("general", ContentFormat::Article);
    assert_eq!(system_prompt(&req), golden("article_system.txt"));
    assert_eq!(
        user_prompt(&req, "2026-08-05").unwrap(),
        golden("article_user.txt")
    );
    assert_eq!(
        length_rewrite_prompt(&req, "T", "Body text.", 2),
        golden("article_rewrite.txt").trim_end_matches('\n')
    );
}

#[test]
fn social_prompts_match_python() {
    let mut req = fixture_req("social-tip", ContentFormat::Social);
    req.topic = Some("hydration".into());
    req.max_chars = Some(180);
    assert_eq!(system_prompt(&req), golden("social_system.txt"));
    assert_eq!(
        user_prompt(&req, "2026-08-05").unwrap(),
        golden("social_user.txt")
    );
    assert_eq!(
        length_rewrite_prompt(&req, "", "A post", 6),
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
fn safe_format_keeps_unknown_and_rejects_unbalanced() {
    let mut kwargs = HashMap::new();
    kwargs.insert("today", "2026-08-05".into());
    assert_eq!(
        safe_format("Today is {today} about {topic}.", &kwargs),
        "Today is 2026-08-05 about {topic}."
    );
    assert_eq!(safe_format("broken {", &kwargs), "broken {");
}
