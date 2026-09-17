// Copyright (c) 2026, Nathaniel T. Berry
// Licensed under the Apache License, Version 2.0.

//! Shared golden vectors from `tests/golden/` (exported from Python).

use std::fs;
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::Value;
use symsight_core::{
    char_count, clean_body, extract_social_text, extract_title_body, is_plausible_title, slugify,
    word_count,
};

fn golden(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/golden")
        .join(name);
    let raw = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn cases(name: &str) -> Vec<Value> {
    golden(name)["cases"].as_array().expect("cases").clone()
}

#[derive(Debug, Deserialize)]
struct TitleBody {
    title: String,
    body: String,
}

#[test]
fn word_count_goldens() {
    for case in cases("word_count.json") {
        let name = case["name"].as_str().unwrap();
        let input = case["input"].as_str().unwrap();
        let expected = case["expected"].as_u64().unwrap() as usize;
        assert_eq!(word_count(input), expected, "{name}");
    }
}

#[test]
fn char_count_goldens() {
    for case in cases("char_count.json") {
        let name = case["name"].as_str().unwrap();
        let input = case["input"].as_str().unwrap();
        let expected = case["expected"].as_u64().unwrap() as usize;
        assert_eq!(char_count(input), expected, "{name}");
    }
}

#[test]
fn slugify_goldens() {
    for case in cases("slugify.json") {
        let name = case["name"].as_str().unwrap();
        let input = case["input"].as_str().unwrap();
        let expected = case["expected"].as_str().unwrap();
        let max_len = case.get("max_len").and_then(Value::as_u64).unwrap_or(60) as usize;
        assert_eq!(slugify(input, max_len), expected, "{name}");
    }
}

#[test]
fn clean_body_goldens() {
    for case in cases("clean_body.json") {
        let name = case["name"].as_str().unwrap();
        let input = case["input"].as_str().unwrap();
        let expected = case["expected"].as_str().unwrap();
        assert_eq!(clean_body(input), expected, "{name}");
    }
}

#[test]
fn is_plausible_title_goldens() {
    for case in cases("is_plausible_title.json") {
        let name = case["name"].as_str().unwrap();
        let input = case["input"].as_str().unwrap();
        let expected = case["expected"].as_bool().unwrap();
        assert_eq!(is_plausible_title(input), expected, "{name}");
    }
}

#[test]
fn extract_title_body_goldens() {
    for case in cases("extract_title_body.json") {
        let name = case["name"].as_str().unwrap();
        let input = case["input"].as_str().unwrap();
        let result = extract_title_body(input);
        match case.get("error").and_then(Value::as_str) {
            Some(msg) => {
                let err = result.expect_err(name);
                assert_eq!(err.to_string(), msg, "{name}");
            }
            None => {
                let (title, body) = result.expect(name);
                let expected: TitleBody = serde_json::from_value(case["expected"].clone()).unwrap();
                assert_eq!(title, expected.title, "{name} title");
                assert_eq!(body, expected.body, "{name} body");
            }
        }
    }
}

#[test]
fn extract_social_text_goldens() {
    for case in cases("extract_social_text.json") {
        let name = case["name"].as_str().unwrap();
        let input = case["input"].as_str().unwrap();
        let result = extract_social_text(input);
        match case.get("error").and_then(Value::as_str) {
            Some(msg) => {
                let err = result.expect_err(name);
                assert_eq!(err.to_string(), msg, "{name}");
            }
            None => {
                let got = result.expect(name);
                let expected = case["expected"].as_str().unwrap();
                assert_eq!(got, expected, "{name}");
            }
        }
    }
}
