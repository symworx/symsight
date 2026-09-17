// Copyright (c) 2026, Nathaniel Berry
// Licensed under the Apache License, Version 2.0.

//! Generation with [`symsight_core::ScriptedClient`] — no live HTTP.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use symsight_core::{
    generate_and_write, generate_content, load_brand_file, response_text, AppConfig, ContentFormat,
    GenerateError, GenerateRequest, ScriptedClient,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn tmp_dir() -> PathBuf {
    let base = std::env::temp_dir().join("symsight-core-generate");
    fs::create_dir_all(&base).unwrap();
    let dir = base.join(format!(
        "case-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn test_cfg(root: &std::path::Path) -> AppConfig {
    AppConfig {
        xai_api_key: "test-key".into(),
        model: "test-model".into(),
        base_url: "https://api.x.ai/v1".into(),
        active_brand: "fixture-co".into(),
        brands_dir: root.to_path_buf(),
        drafts_dir: root.join("drafts"),
        final_dir: root.join("final"),
        project_root: root.to_path_buf(),
    }
    .resolve_paths()
}

fn article_words(n: usize) -> String {
    format!("TITLE: Test Title\n---\n{}", vec!["word"; n].join(" "))
}

#[test]
fn generate_article_scripted_writes_draft() {
    let root = tmp_dir();
    let brand = load_brand_file(repo_root().join("tests/fixtures/fixture-co.yaml")).unwrap();
    let client = ScriptedClient::new(vec![article_words(60)]);
    let cfg = test_cfg(&root);
    let req = GenerateRequest {
        brand,
        type_id: "general".into(),
        format: ContentFormat::Article,
        topic: Some("testing".into()),
        min_words: Some(50),
        max_words: Some(100),
        max_chars: None,
        use_search: Some(false),
        model: None,
    };
    let path = generate_and_write(&req, &cfg, None, Some(&client)).unwrap();
    assert!(path.is_file());
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("Test Title"));
    assert!(text.contains("word"));
    assert_eq!(client.call_count(), 1);
    assert!(!client.calls.lock().unwrap()[0].use_search);
}

#[test]
fn generate_social_scripted_writes_draft() {
    let root = tmp_dir();
    let brand = load_brand_file(repo_root().join("tests/fixtures/fixture-co.yaml")).unwrap();
    let client = ScriptedClient::new(vec!["A short tip about testing carefully.".into()]);
    let cfg = test_cfg(&root);
    let req = GenerateRequest {
        brand,
        type_id: "social-tip".into(),
        format: ContentFormat::Social,
        topic: Some("testing".into()),
        min_words: None,
        max_words: None,
        max_chars: Some(200),
        use_search: Some(false),
        model: None,
    };
    let path = generate_and_write(&req, &cfg, None, Some(&client)).unwrap();
    assert!(fs::read_to_string(path).unwrap().contains("short tip"));
    assert_eq!(client.call_count(), 1);
}

#[test]
fn article_rewrites_when_too_short() {
    let root = tmp_dir();
    let brand = load_brand_file(repo_root().join("tests/fixtures/fixture-co.yaml")).unwrap();
    let client = ScriptedClient::new(vec![article_words(5), article_words(60)]);
    let cfg = test_cfg(&root);
    let req = GenerateRequest {
        brand,
        type_id: "general".into(),
        format: ContentFormat::Article,
        topic: Some("testing".into()),
        min_words: Some(50),
        max_words: Some(100),
        max_chars: None,
        use_search: Some(true),
        model: None,
    };
    generate_content(&req, &cfg, Some(&client)).unwrap();
    assert_eq!(client.call_count(), 2);
    let calls = client.calls.lock().unwrap();
    assert!(calls[0].use_search);
    assert!(!calls[1].use_search);
    assert!(calls[1].user.contains("Current word count: 5"));
}

#[test]
fn article_rewrites_implausible_title_then_falls_back() {
    let root = tmp_dir();
    let brand = load_brand_file(repo_root().join("tests/fixtures/fixture-co.yaml")).unwrap();
    let body = vec!["word"; 60].join(" ");
    let bad = format!("TITLE: I will revise the essay now carefully\n---\n{body}");
    let still_bad = format!("TITLE: I will write something now\n---\n{body}");
    let client = ScriptedClient::new(vec![bad, still_bad]);
    let cfg = test_cfg(&root);
    let req = GenerateRequest {
        brand,
        type_id: "general".into(),
        format: ContentFormat::Article,
        topic: Some("testing".into()),
        min_words: Some(50),
        max_words: Some(100),
        max_chars: None,
        use_search: Some(false),
        model: None,
    };
    let (title, _, _) = generate_content(&req, &cfg, Some(&client)).unwrap();
    assert_eq!(title, "Testing");
    assert_eq!(client.call_count(), 2);
}

#[test]
fn social_rewrites_when_over_max_chars() {
    let root = tmp_dir();
    let brand = load_brand_file(repo_root().join("tests/fixtures/fixture-co.yaml")).unwrap();
    let long = "x".repeat(50);
    let client = ScriptedClient::new(vec![long, "short ok".into()]);
    let cfg = test_cfg(&root);
    let req = GenerateRequest {
        brand,
        type_id: "social-tip".into(),
        format: ContentFormat::Social,
        topic: Some("testing".into()),
        min_words: None,
        max_words: None,
        max_chars: Some(20),
        use_search: Some(false),
        model: None,
    };
    let (_, body, _) = generate_content(&req, &cfg, Some(&client)).unwrap();
    assert_eq!(body, "short ok");
    assert_eq!(client.call_count(), 2);
    assert!(!client.calls.lock().unwrap()[1].use_search);
}

#[test]
fn forbidden_term_fails_generation() {
    let root = tmp_dir();
    let brand = load_brand_file(repo_root().join("tests/fixtures/fixture-co.yaml")).unwrap();
    let body = format!(
        "TITLE: Ok Title\n---\nWelcome to FixtureCorp {}",
        vec!["word"; 55].join(" ")
    );
    let client = ScriptedClient::new(vec![body]);
    let cfg = test_cfg(&root);
    let req = GenerateRequest {
        brand,
        type_id: "general".into(),
        format: ContentFormat::Article,
        topic: Some("testing".into()),
        min_words: Some(50),
        max_words: Some(100),
        max_chars: None,
        use_search: Some(false),
        model: None,
    };
    let err = generate_content(&req, &cfg, Some(&client)).unwrap_err();
    match err {
        GenerateError::Forbidden { hits } => assert!(hits.contains("fixturecorp")),
        other => panic!("{other}"),
    }
}

#[test]
fn empty_model_response_is_error() {
    let root = tmp_dir();
    let brand = load_brand_file(repo_root().join("tests/fixtures/fixture-co.yaml")).unwrap();
    let client = ScriptedClient::new(vec![String::new()]);
    let cfg = test_cfg(&root);
    let req = GenerateRequest {
        brand,
        type_id: "general".into(),
        format: ContentFormat::Article,
        topic: Some("testing".into()),
        min_words: Some(50),
        max_words: Some(100),
        max_chars: None,
        use_search: Some(false),
        model: None,
    };
    let err = generate_content(&req, &cfg, Some(&client)).unwrap_err();
    assert!(matches!(err, GenerateError::Empty));
}

#[test]
fn response_text_fixtures() {
    let dir = repo_root().join("tests/golden/response_text");
    let prefer: Value =
        serde_json::from_str(&fs::read_to_string(dir.join("output_text.json")).unwrap()).unwrap();
    assert_eq!(response_text(&prefer), "Hello from output_text");

    let walk: Value =
        serde_json::from_str(&fs::read_to_string(dir.join("message_and_reasoning.json")).unwrap())
            .unwrap();
    assert_eq!(response_text(&walk), "Visible answer");

    let empty: Value =
        serde_json::from_str(&fs::read_to_string(dir.join("empty.json")).unwrap()).unwrap();
    assert_eq!(response_text(&empty), "");
}
