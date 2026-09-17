// Copyright (c) 2026, Nathaniel Berry
// Licensed under the Apache License, Version 2.0.

//! Brandcheck parity with `src/symsight/brandcheck.py`.

use std::fs;
use std::path::PathBuf;

use symsight_core::{check_text, iter_scan_files, load_brand_file, scan_paths};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn tmp_dir() -> PathBuf {
    let base = std::env::temp_dir().join("symsight-core-brandcheck");
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

#[test]
fn fixture_forbidden_is_case_insensitive_substring() {
    let brand = load_brand_file(repo_root().join("tests/fixtures/fixture-co.yaml")).unwrap();
    let hits = check_text("Welcome to FixtureCorp services", &brand);
    assert!(hits.iter().any(|h| h.eq_ignore_ascii_case("fixturecorp")));
    assert!(check_text("Welcome to Fixture Co", &brand).is_empty());
}

#[test]
fn skips_target_and_dist_dirs() {
    let root = tmp_dir();
    fs::create_dir_all(root.join("target")).unwrap();
    fs::create_dir_all(root.join("dist")).unwrap();
    fs::create_dir_all(root.join("ok")).unwrap();
    fs::write(root.join("target/hidden.md"), "fixturecorp").unwrap();
    fs::write(root.join("dist/hidden.md"), "fixturecorp").unwrap();
    fs::write(root.join("ok/visible.md"), "fixturecorp").unwrap();
    let files = iter_scan_files(&[root.as_path()]);
    let names: Vec<String> = files
        .iter()
        .filter_map(|p| p.file_name()?.to_str().map(str::to_string))
        .collect();
    assert_eq!(names, ["visible.md"]);

    let brand = load_brand_file(repo_root().join("tests/fixtures/fixture-co.yaml")).unwrap();
    let problems = scan_paths(&[root.as_path()], &brand);
    assert_eq!(problems.len(), 1);
    assert!(problems[0].0.ends_with("visible.md"));
}
