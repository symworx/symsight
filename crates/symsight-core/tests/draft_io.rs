// Copyright (c) 2026, Nathaniel T. Berry
// Licensed under the Apache License, Version 2.0.

//! Draft I/O and finalize parity with `src/symsight/draft_io.py`.

use std::fs;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde_json::Value;
use symsight_core::{
    finalize_draft, load_brand_file, parse_front_matter, read_draft, render_front_matter,
    set_status, unique_draft_path, write_new_draft, ContentFormat, DraftMeta, FinalizeError,
    FrontValue, WriteNewDraft,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn golden(name: &str) -> PathBuf {
    repo_root().join("tests/golden").join(name)
}

fn tmp_dir() -> PathBuf {
    let base = std::env::temp_dir().join("symsight-core-draft-io");
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

fn front_from_json(value: &Value) -> FrontValue {
    if value == &Value::String("null".into()) {
        return FrontValue::Null;
    }
    let obj = value.as_object().expect("front value object");
    if let Some(s) = obj.get("string") {
        return FrontValue::String(s.as_str().unwrap().to_string());
    }
    if let Some(n) = obj.get("int") {
        return FrontValue::Int(n.as_i64().unwrap());
    }
    if let Some(b) = obj.get("bool") {
        return FrontValue::Bool(b.as_bool().unwrap());
    }
    panic!("unknown front value {value}");
}

#[test]
fn parse_front_matter_goldens() {
    let doc: Value =
        serde_json::from_str(&fs::read_to_string(golden("front_matter.json")).unwrap()).unwrap();
    for case in doc["parse"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let input = case["input"].as_str().unwrap();
        let (fm, body) = parse_front_matter(input);
        assert_eq!(body, case["expected_body"].as_str().unwrap(), "{name} body");
        let expected = case["expected_front"].as_object().unwrap();
        assert_eq!(fm.len(), expected.len(), "{name} keys");
        for (key, val) in expected {
            assert_eq!(fm.get(key), Some(&front_from_json(val)), "{name} {key}");
        }
    }
}

#[test]
fn render_front_matter_goldens() {
    let doc: Value =
        serde_json::from_str(&fs::read_to_string(golden("front_matter.json")).unwrap()).unwrap();
    for case in doc["render"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut front = IndexMap::new();
        for pair in case["front"].as_array().unwrap() {
            let key = pair[0].as_str().unwrap();
            front.insert(key.to_string(), front_from_json(&pair[1]));
        }
        assert_eq!(
            render_front_matter(&front),
            case["expected"].as_str().unwrap(),
            "{name}"
        );
    }
}

fn article_meta() -> DraftMeta {
    DraftMeta {
        model: "test-model".into(),
        type_id: "general".into(),
        format: "article".into(),
        topic: None,
        brand_id: "fixture-co".into(),
        used_web_search: false,
        word_count: Some(12),
        char_count: None,
        generated_at: "2026-08-15T12:00:00.000000+00:00".into(),
        title: None,
        path: None,
    }
}

fn social_meta() -> DraftMeta {
    DraftMeta {
        model: "test-model".into(),
        type_id: "social-tip".into(),
        format: "social".into(),
        topic: None,
        brand_id: "fixture-co".into(),
        used_web_search: false,
        word_count: None,
        char_count: Some(20),
        generated_at: "2026-08-15T12:00:00.000000+00:00".into(),
        title: None,
        path: None,
    }
}

fn assert_meta_golden(path: &Path, golden_name: &str) {
    let got: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    let mut expected: Value =
        serde_json::from_str(&fs::read_to_string(golden(golden_name)).unwrap()).unwrap();
    // sidecar is sibling of the .md; the written path field is the markdown path
    expected["path"] = Value::String(
        path.to_string_lossy()
            .replacen(".meta.json", ".md", 1)
            .to_string(),
    );
    assert_eq!(got, expected);
}

#[test]
fn write_new_draft_article_matches_golden() {
    let dir = tmp_dir();
    let path = write_new_draft(WriteNewDraft {
        drafts_dir: &dir,
        title: "Hello World",
        body: "This is the body of the draft with enough words here.",
        brand_id: "fixture-co",
        brand_display: "Fixture Co",
        type_id: "general",
        format: ContentFormat::Article,
        topic: Some("hello"),
        disclaimer: Some("Test disclaimer."),
        meta: &article_meta(),
    })
    .unwrap();
    assert!(path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .ends_with("-general-hello-world.md"));
    let got = fs::read_to_string(&path).unwrap();
    let expected = fs::read_to_string(golden("write_new_draft/article.md")).unwrap();
    assert_eq!(got, expected);
    assert_meta_golden(
        &path.with_extension("meta.json"),
        "write_new_draft/article.meta.json",
    );
}

#[test]
fn write_new_draft_social_matches_golden() {
    let dir = tmp_dir();
    let path = write_new_draft(WriteNewDraft {
        drafts_dir: &dir,
        title: "",
        body: "A short tip about testing carefully.",
        brand_id: "fixture-co",
        brand_display: "Fixture Co",
        type_id: "social-tip",
        format: ContentFormat::Social,
        topic: None,
        disclaimer: None,
        meta: &social_meta(),
    })
    .unwrap();
    assert!(path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .ends_with("-social-a-short-tip-about-testing-carefully.md"));
    let got = fs::read_to_string(&path).unwrap();
    let expected = fs::read_to_string(golden("write_new_draft/social.md")).unwrap();
    assert_eq!(got, expected);
    assert_meta_golden(
        &path.with_extension("meta.json"),
        "write_new_draft/social.meta.json",
    );
}

#[test]
fn unique_draft_path_rejects_traversal() {
    let dir = tmp_dir();
    let err = unique_draft_path(&dir, "../etc/passwd").unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    let err = unique_draft_path(&dir, "..").unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
}

#[test]
fn unique_draft_path_adds_numeric_suffix() {
    let dir = tmp_dir();
    let first = unique_draft_path(&dir, "same").unwrap();
    fs::write(&first, "a").unwrap();
    let second = unique_draft_path(&dir, "same").unwrap();
    assert_eq!(second.file_name().unwrap(), "same-2.md");
}

#[test]
fn set_status_quoted_unquoted_and_missing() {
    let dir = tmp_dir();
    let quoted = dir.join("q.md");
    fs::write(&quoted, "---\nstatus: \"draft\"\ntitle: \"T\"\n---\nBody\n").unwrap();
    set_status(&quoted, "final").unwrap();
    assert!(fs::read_to_string(&quoted)
        .unwrap()
        .contains("status: \"final\""));

    let bare = dir.join("b.md");
    fs::write(&bare, "---\nstatus: draft\ntitle: \"T\"\n---\nBody\n").unwrap();
    set_status(&bare, "final").unwrap();
    assert!(fs::read_to_string(&bare)
        .unwrap()
        .contains("status: \"final\""));

    let missing = dir.join("m.md");
    fs::write(&missing, "---\ntitle: \"T\"\n---\nBody\n").unwrap();
    set_status(&missing, "final").unwrap();
    let text = fs::read_to_string(&missing).unwrap();
    assert!(text.contains("status: \"final\""));
}

#[test]
fn read_sample_drafts() {
    let article =
        read_draft(&repo_root().join("examples/sample-drafts/article-outline.md")).unwrap();
    assert_eq!(
        article.title,
        "What rising rates mean for short-duration bond funds"
    );
    assert_eq!(
        article.front_matter.get("format"),
        Some(&FrontValue::String("article".into()))
    );
    assert!(!article.body.to_lowercase().contains("disclaimer."));
    assert!(article.body.contains("Sample shape only"));

    let social = read_draft(&repo_root().join("examples/sample-drafts/social-post.md")).unwrap();
    assert_eq!(social.title, "social-post");
    assert_eq!(
        social.front_matter.get("format"),
        Some(&FrontValue::String("social".into()))
    );
    assert!(social.body.contains("Sample social shape only"));
}

#[test]
fn finalize_move_and_copy() {
    let brand = load_brand_file(repo_root().join("tests/fixtures/fixture-co.yaml")).unwrap();
    let root = tmp_dir();
    let drafts = root.join("drafts");
    let final_dir = root.join("final");
    let path = write_new_draft(WriteNewDraft {
        drafts_dir: &drafts,
        title: "Finalize Me",
        body: "Short body for finalize.",
        brand_id: "fixture-co",
        brand_display: "Fixture Co",
        type_id: "general",
        format: ContentFormat::Article,
        topic: None,
        disclaimer: None,
        meta: &article_meta(),
    })
    .unwrap();
    let dest = finalize_draft(&path, &final_dir, Some(&brand), false).unwrap();
    assert!(dest.is_file());
    assert_eq!(dest.parent().unwrap(), final_dir.as_path());
    assert!(!path.exists());
    let moved = read_draft(&dest).unwrap();
    assert_eq!(
        moved.front_matter.get("status"),
        Some(&FrontValue::String("final".into()))
    );

    let copy_src = write_new_draft(WriteNewDraft {
        drafts_dir: &drafts,
        title: "Copy Me",
        body: "Short body for copy finalize.",
        brand_id: "fixture-co",
        brand_display: "Fixture Co",
        type_id: "general",
        format: ContentFormat::Article,
        topic: None,
        disclaimer: None,
        meta: &article_meta(),
    })
    .unwrap();
    let copy_dest = finalize_draft(&copy_src, &final_dir, None, true).unwrap();
    assert!(copy_dest.is_file());
    assert!(copy_src.exists());
}

#[test]
fn finalize_rejects_existing_dest_and_forbidden_terms() {
    let brand = load_brand_file(repo_root().join("tests/fixtures/fixture-co.yaml")).unwrap();
    let root = tmp_dir();
    let drafts = root.join("drafts");
    let final_dir = root.join("final");
    let path = write_new_draft(WriteNewDraft {
        drafts_dir: &drafts,
        title: "Clash",
        body: "ok body",
        brand_id: "fixture-co",
        brand_display: "Fixture Co",
        type_id: "general",
        format: ContentFormat::Article,
        topic: None,
        disclaimer: None,
        meta: &article_meta(),
    })
    .unwrap();
    fs::create_dir_all(&final_dir).unwrap();
    fs::write(final_dir.join(path.file_name().unwrap()), "taken").unwrap();
    let err = finalize_draft(&path, &final_dir, None, false).unwrap_err();
    assert!(matches!(err, FinalizeError::DestinationExists(_)));

    let dirty = write_new_draft(WriteNewDraft {
        drafts_dir: &drafts,
        title: "Dirty",
        body: "Welcome to FixtureCorp services",
        brand_id: "fixture-co",
        brand_display: "Fixture Co",
        type_id: "general",
        format: ContentFormat::Article,
        topic: None,
        disclaimer: None,
        meta: &article_meta(),
    })
    .unwrap();
    let err = finalize_draft(&dirty, &root.join("final2"), Some(&brand), false).unwrap_err();
    match err {
        FinalizeError::BrandCheck { hits } => assert!(hits.contains("fixturecorp")),
        other => panic!("{other}"),
    }
}
