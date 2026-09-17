// Copyright (c) 2026, Nathaniel T. Berry
// Licensed under the Apache License, Version 2.0.

//! Forbidden-term scanning (`src/symsight/brandcheck.py`).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::Brand;

const SCAN_EXTENSIONS: &[&str] = &[
    ".html", ".md", ".css", ".js", ".json", ".txt", ".svg", ".py", ".yml", ".yaml", ".toml",
];

/// Directory names skipped while walking. Includes `target` and `dist` so Cargo
/// / pack artifacts are not scanned (CHANGELOG Unreleased).
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".venv",
    "venv",
    "__pycache__",
    "node_modules",
    ".grok",
    ".ruff_cache",
    ".mypy_cache",
    ".pytest_cache",
    "target",
    "dist",
];

pub fn find_hits(text: &str, forbidden: &[String]) -> Vec<String> {
    let lower = text.to_lowercase();
    forbidden
        .iter()
        .filter(|term| lower.contains(&term.to_lowercase()))
        .cloned()
        .collect()
}

pub fn check_text(text: &str, brand: &Brand) -> Vec<String> {
    find_hits(text, &brand.forbidden)
}

pub fn check_path(path: &Path, brand: &Brand) -> Vec<String> {
    match fs::read_to_string(path) {
        Ok(text) => check_text(&text, brand),
        Err(_) => Vec::new(),
    }
}

fn is_scan_file(path: &Path) -> bool {
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if name == "README" || name == "README.md" {
            return true;
        }
    }
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| {
            let dotted = format!(".{}", ext.to_ascii_lowercase());
            SCAN_EXTENSIONS.contains(&dotted.as_str())
        })
        .unwrap_or(false)
}

fn skipped(path: &Path) -> bool {
    path.components().any(|c| {
        c.as_os_str()
            .to_str()
            .is_some_and(|part| SKIP_DIRS.contains(&part))
    })
}

fn walk_files(root: &Path, out: &mut BTreeSet<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if skipped(&path) {
                continue;
            }
            walk_files(&path, out);
        } else if path.is_file() && !skipped(&path) && is_scan_file(&path) {
            out.insert(path);
        }
    }
}

pub fn iter_scan_files(roots: &[impl AsRef<Path>]) -> Vec<PathBuf> {
    let mut files = BTreeSet::new();
    for root in roots {
        let Ok(root) = root.as_ref().canonicalize() else {
            continue;
        };
        if root.is_file() {
            files.insert(root);
            continue;
        }
        if root.is_dir() {
            walk_files(&root, &mut files);
        }
    }
    files.into_iter().collect()
}

pub fn scan_paths(roots: &[impl AsRef<Path>], brand: &Brand) -> Vec<(PathBuf, Vec<String>)> {
    let mut problems = Vec::new();
    for path in iter_scan_files(roots) {
        let hits = check_path(&path, brand);
        if !hits.is_empty() {
            let mut unique = hits;
            unique.sort();
            unique.dedup();
            problems.push((path, unique));
        }
    }
    problems
}
