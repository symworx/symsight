// Copyright (c) 2026, Nathaniel Berry
// Licensed under the Apache License, Version 2.0.

//! Brand YAML load / resolve parity with `src/symsight/brand.py`.

use std::fs;
use std::path::{Path, PathBuf};

use symsight_core::{
    list_brand_files, list_brands, load_brand_file, resolve_brand, BrandError, ContentFormat,
    GenerateError, GenerateRequest,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_co() -> PathBuf {
    repo_root().join("tests/fixtures/fixture-co.yaml")
}

fn example_writer() -> PathBuf {
    repo_root().join("config/brands/example-writer.yaml")
}

#[test]
fn load_fixture_co_fields_and_type_order() {
    let brand = load_brand_file(fixture_co()).expect("fixture-co");
    assert_eq!(brand.id, "fixture-co");
    assert_eq!(brand.display_name, "Fixture Co");
    assert_eq!(brand.full_name, "Fixture Co");
    assert_eq!(brand.short_name, "Fixture");
    assert_eq!(brand.voice, "Neutral test voice. Plain English.\n");
    assert_eq!(brand.disclaimer, "Fixture disclaimer for tests only.\n");
    assert_eq!(
        brand.forbidden,
        vec!["fixturecorp".to_string(), "fixture-corp".to_string()]
    );
    let keys: Vec<&str> = brand.types.keys().map(String::as_str).collect();
    assert_eq!(keys, ["general", "social-tip"]);
    let general = &brand.types["general"];
    assert_eq!(general.description, "General article");
    assert!(!general.default_use_search);
    assert_eq!(
        general.user_template,
        "Today is {today}. Write a {min_words}–{max_words} word piece on: {topic}.\n\
         Byline spirit: {full_name}. Respond with ONLY the TITLE / --- / body format.\n"
    );
    let social = &brand.types["social-tip"];
    assert_eq!(social.description, "Social tip");
    assert_eq!(
        social.user_template,
        "Write a social post (max {max_chars} chars) about: {topic}. Output ONLY the post.\n"
    );
    assert_eq!(brand.formats.article.default_min_words, 50);
    assert_eq!(brand.formats.article.default_max_words, 100);
    assert_eq!(brand.formats.social.default_max_chars, 200);
}

#[test]
fn load_example_writer_fields_and_type_order() {
    let brand = load_brand_file(example_writer()).expect("example-writer");
    assert_eq!(brand.id, "example-writer");
    assert_eq!(brand.display_name, "Independent Writer");
    assert_eq!(brand.full_name, "Independent Writer");
    assert_eq!(brand.short_name, "Writer");
    assert!(brand.forbidden.is_empty());
    assert_eq!(
        brand.voice,
        "Clear, direct, and curious. Prefer plain language. Explain technical terms\n\
         briefly when they first appear. Educational tone; no hype.\n"
    );
    assert_eq!(
        brand.disclaimer,
        "This content is for general informational and educational purposes only.\n\
         It is not personalized professional advice. Verify facts independently\n\
         before acting on them.\n"
    );
    let keys: Vec<&str> = brand.types.keys().map(String::as_str).collect();
    assert_eq!(keys, ["general", "news", "social-tip"]);
    assert!(!brand.types["general"].default_use_search);
    assert!(brand.types["news"].default_use_search);
    assert!(!brand.types["social-tip"].default_use_search);
    assert_eq!(brand.formats.article.default_min_words, 200);
    assert_eq!(brand.formats.article.default_max_words, 500);
    assert_eq!(brand.formats.social.default_max_chars, 200);
    assert_eq!(
        brand.types["general"].user_template,
        "Today is {today}. Write a {min_words}–{max_words} word piece on: {topic}.\n\
         Practical and trustworthy. Do not include a disclaimer block (we append one).\n\
         Byline spirit: {full_name}.\n\
         Respond with ONLY the TITLE / --- / body format.\n"
    );
}

#[test]
fn missing_short_name_uses_first_token_of_full_name() {
    let dir = tempfile();
    let path = dir.join("acme.yaml");
    fs::write(
        &path,
        "display_name: Acme\nvoice: v\nfull_name: Acme Corporation\n",
    )
    .unwrap();
    let brand = load_brand_file(&path).unwrap();
    assert_eq!(brand.id, "acme");
    assert_eq!(brand.short_name, "Acme");
}

#[test]
fn missing_id_uses_file_stem() {
    let dir = tempfile();
    let path = dir.join("from-stem.yaml");
    fs::write(
        &path,
        "display_name: Stem\nvoice: v\nfull_name: Stem Brand\nshort_name: Stem\n",
    )
    .unwrap();
    let brand = load_brand_file(&path).unwrap();
    assert_eq!(brand.id, "from-stem");
}

#[test]
fn missing_formats_use_model_defaults() {
    let dir = tempfile();
    let path = dir.join("plain.yaml");
    fs::write(
        &path,
        "id: plain\ndisplay_name: Plain\nvoice: v\nfull_name: Plain Brand\n",
    )
    .unwrap();
    let brand = load_brand_file(&path).unwrap();
    assert_eq!(brand.formats.article.default_min_words, 200);
    assert_eq!(brand.formats.article.default_max_words, 500);
    assert_eq!(brand.formats.social.default_max_chars, 200);
    assert!(brand.types.is_empty());
    assert!(brand.forbidden.is_empty());
}

#[test]
fn load_missing_file_is_not_found() {
    let err = load_brand_file(Path::new("/nonexistent/brand.yaml")).unwrap_err();
    assert!(matches!(err, BrandError::NotFound(_)));
    assert!(err.to_string().starts_with("Brand file not found:"));
}

#[test]
fn load_non_mapping_is_error() {
    let dir = tempfile();
    let path = dir.join("list.yaml");
    fs::write(&path, "- just a list\n").unwrap();
    let err = load_brand_file(&path).unwrap_err();
    assert!(matches!(err, BrandError::NotAMapping(_)));
}

#[test]
fn load_missing_required_field_is_invalid() {
    let dir = tempfile();
    let path = dir.join("bad.yaml");
    fs::write(&path, "id: bad\n").unwrap();
    let err = load_brand_file(&path).unwrap_err();
    assert!(matches!(err, BrandError::Invalid { .. }));
    assert!(err.to_string().starts_with("Invalid brand file"));
}

#[test]
fn resolve_by_stem_and_by_in_file_id() {
    let dir = tempfile();
    fs::copy(fixture_co(), dir.join("fixture-co.yaml")).unwrap();
    let by_id = resolve_brand(&dir, Some("fixture-co"), None).unwrap();
    assert_eq!(by_id.full_name, "Fixture Co");

    fs::write(
        dir.join("alias.yml"),
        "id: other-id\ndisplay_name: Other\nvoice: v\nfull_name: Other Brand\n",
    )
    .unwrap();
    let by_file_id = resolve_brand(&dir, Some("other-id"), None).unwrap();
    assert_eq!(by_file_id.id, "other-id");
}

#[test]
fn resolve_brand_path_wins() {
    let dir = tempfile();
    let path = dir.join("custom.yaml");
    fs::copy(example_writer(), &path).unwrap();
    let brand = resolve_brand("/nonexistent", Some("ignored"), Some(&path)).unwrap();
    assert_eq!(brand.id, "example-writer");
}

#[test]
fn resolve_missing_brand() {
    let err = resolve_brand(Path::new("/nonexistent"), Some("nope"), None).unwrap_err();
    match err {
        BrandError::Unresolved {
            brand_id,
            available,
            ..
        } => {
            assert_eq!(brand_id, "nope");
            assert_eq!(available, "(none)");
        }
        other => panic!("expected Unresolved, got {other}"),
    }
}

#[test]
fn resolve_rejects_unsafe_brand_id() {
    let err = resolve_brand(Path::new("."), Some("../etc/passwd"), None).unwrap_err();
    match err {
        BrandError::UnsafeId(id) => assert_eq!(id, "../etc/passwd"),
        other => panic!("expected UnsafeId, got {other}"),
    }
}

#[test]
fn resolve_requires_brand_id() {
    let err = resolve_brand(Path::new("."), None, None).unwrap_err();
    assert!(matches!(err, BrandError::NotSpecified));
}

#[test]
fn list_brand_files_yaml_then_yml() {
    let dir = tempfile();
    fs::write(dir.join("b.yaml"), "x\n").unwrap();
    fs::write(dir.join("a.yml"), "x\n").unwrap();
    fs::write(dir.join("c.yaml"), "x\n").unwrap();
    let names: Vec<String> = list_brand_files(&dir)
        .into_iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["b.yaml", "c.yaml", "a.yml"]);
}

#[test]
fn list_brands_skips_invalid_files() {
    let dir = tempfile();
    fs::copy(fixture_co(), dir.join("ok.yaml")).unwrap();
    fs::write(dir.join("bad.yaml"), "- nope\n").unwrap();
    let brands = list_brands(&dir);
    assert_eq!(brands.len(), 1);
    assert_eq!(brands[0].id, "fixture-co");
}

#[test]
fn generate_request_resolves_defaults_and_overrides() {
    let brand = load_brand_file(fixture_co()).unwrap();
    let req = GenerateRequest {
        brand: brand.clone(),
        type_id: "general".into(),
        format: ContentFormat::Article,
        topic: Some("testing".into()),
        min_words: None,
        max_words: None,
        max_chars: None,
        use_search: None,
        model: None,
    };
    assert_eq!(req.resolved_min_words(), 50);
    assert_eq!(req.resolved_max_words(), 100);
    assert_eq!(req.resolved_max_chars(), 200);
    assert!(!req.resolved_use_search().unwrap());
    assert_eq!(req.resolved_type().unwrap().description, "General article");

    let news = load_brand_file(example_writer()).unwrap();
    let news_req = GenerateRequest {
        brand: news,
        type_id: "news".into(),
        format: ContentFormat::Article,
        topic: None,
        min_words: Some(80),
        max_words: Some(90),
        max_chars: Some(120),
        use_search: None,
        model: None,
    };
    assert_eq!(news_req.resolved_min_words(), 80);
    assert_eq!(news_req.resolved_max_words(), 90);
    assert_eq!(news_req.resolved_max_chars(), 120);
    assert!(news_req.resolved_use_search().unwrap());

    let forced_off = GenerateRequest {
        brand: load_brand_file(example_writer()).unwrap(),
        type_id: "news".into(),
        format: ContentFormat::Article,
        topic: None,
        min_words: None,
        max_words: None,
        max_chars: None,
        use_search: Some(false),
        model: None,
    };
    assert!(!forced_off.resolved_use_search().unwrap());
}

#[test]
fn generate_request_unknown_type() {
    let brand = load_brand_file(fixture_co()).unwrap();
    let req = GenerateRequest {
        brand,
        type_id: "missing".into(),
        format: ContentFormat::Article,
        topic: None,
        min_words: None,
        max_words: None,
        max_chars: None,
        use_search: None,
        model: None,
    };
    let err = req.resolved_type().unwrap_err();
    match err {
        GenerateError::UnknownType {
            type_id,
            brand_id,
            known,
        } => {
            assert_eq!(type_id, "missing");
            assert_eq!(brand_id, "fixture-co");
            assert_eq!(known, "general, social-tip");
        }
        other => panic!("unexpected {other}"),
    }
}

fn tempfile() -> PathBuf {
    let base = std::env::temp_dir().join("symsight-core-brand-tests");
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
