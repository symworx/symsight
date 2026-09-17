// Copyright (c) 2026, Nathaniel T. Berry
// Licensed under the Apache License, Version 2.0.

//! Load and resolve brand YAML configs (`src/symsight/brand.py`).

use std::fs;
use std::path::{Path, PathBuf};

#[allow(deprecated)]
use serde_yml::{from_str, from_value, Value};

use crate::error::BrandError;
use crate::models::Brand;
use crate::textutil::is_safe_path_component;

/// Load a single brand YAML file.
pub fn load_brand_file(path: impl AsRef<Path>) -> Result<Brand, BrandError> {
    let path = path.as_ref();
    if !path.is_file() {
        return Err(BrandError::NotFound(path.to_path_buf()));
    }
    let text = fs::read_to_string(path).map_err(|err| BrandError::Invalid {
        path: path.to_path_buf(),
        message: err.to_string(),
    })?;
    let mut raw: Value = from_str(&text).map_err(|err| BrandError::Invalid {
        path: path.to_path_buf(),
        message: err.to_string(),
    })?;
    let mapping = raw
        .as_mapping_mut()
        .ok_or_else(|| BrandError::NotAMapping(path.to_path_buf()))?;
    if !mapping.contains_key("id") {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("brand")
            .to_string();
        mapping.insert("id", Value::String(stem));
    }
    let mut brand: Brand = from_value(raw).map_err(|err| BrandError::Invalid {
        path: path.to_path_buf(),
        message: err.to_string(),
    })?;
    brand.finalize_short_name();
    Ok(brand)
}

/// YAML then leftover YML files, each group sorted by path (same as Python).
pub fn list_brand_files(brands_dir: impl AsRef<Path>) -> Vec<PathBuf> {
    let brands_dir = brands_dir.as_ref();
    if !brands_dir.is_dir() {
        return Vec::new();
    }
    let mut yaml = Vec::new();
    let mut yml = Vec::new();
    let entries = match fs::read_dir(brands_dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("yaml") => yaml.push(path),
            Some("yml") => yml.push(path),
            _ => {}
        }
    }
    yaml.sort();
    yml.sort();
    yaml.extend(yml);
    yaml
}

/// Load every readable brand under `brands_dir`; skip files that fail to parse.
pub fn list_brands(brands_dir: impl AsRef<Path>) -> Vec<Brand> {
    list_brand_files(brands_dir)
        .into_iter()
        .filter_map(|path| load_brand_file(path).ok())
        .collect()
}

/// Resolve a brand by explicit path or by id under `brands_dir`.
pub fn resolve_brand(
    brands_dir: impl AsRef<Path>,
    brand_id: Option<&str>,
    brand_path: Option<&Path>,
) -> Result<Brand, BrandError> {
    if let Some(path) = brand_path {
        return load_brand_file(path);
    }
    let Some(brand_id) = brand_id.filter(|id| !id.is_empty()) else {
        return Err(BrandError::NotSpecified);
    };
    if !is_safe_path_component(brand_id) {
        return Err(BrandError::UnsafeId(brand_id.to_string()));
    }
    let brands_dir = brands_dir.as_ref();
    for ext in [".yaml", ".yml"] {
        let candidate = brands_dir.join(format!("{brand_id}{ext}"));
        if candidate.is_file() {
            return load_brand_file(candidate);
        }
    }
    for path in list_brand_files(brands_dir) {
        if let Ok(brand) = load_brand_file(&path) {
            if brand.id == brand_id {
                return Ok(brand);
            }
        }
    }
    let known = list_brand_files(brands_dir);
    let available = if known.is_empty() {
        "(none)".to_string()
    } else {
        known
            .iter()
            .filter_map(|p| p.file_stem()?.to_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    Err(BrandError::Unresolved {
        brand_id: brand_id.to_string(),
        brands_dir: brands_dir.to_path_buf(),
        available,
    })
}
