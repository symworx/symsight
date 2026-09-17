// Copyright (c) 2026, Nathaniel Berry
// Licensed under the Apache License, Version 2.0.

//! Error types matching the Python package's exception wording.

use std::path::PathBuf;

use thiserror::Error;

/// Brand load / resolve failure (`symsight.brand.BrandError`).
#[derive(Debug, Error)]
pub enum BrandError {
    #[error("Brand file not found: {0}")]
    NotFound(PathBuf),
    #[error("Brand file must be a mapping: {0}")]
    NotAMapping(PathBuf),
    #[error("Invalid brand file {path}: {message}")]
    Invalid { path: PathBuf, message: String },
    #[error("No brand specified (set active_brand, --brand, or --brand-file)")]
    NotSpecified,
    #[error("Unsafe brand id {0:?}: must be a single path segment")]
    UnsafeId(String),
    #[error("Brand '{brand_id}' not found in {brands_dir}. Available: {available}")]
    Unresolved {
        brand_id: String,
        brands_dir: PathBuf,
        available: String,
    },
}

/// Generation / request resolution failure (`symsight.generate.GenerateError` and
/// `ValueError` from `GenerateRequest.resolved_type`).
#[derive(Debug, Error)]
pub enum GenerateError {
    #[error("Unknown type '{type_id}' for brand '{brand_id}'. Known: {known}")]
    UnknownType {
        type_id: String,
        brand_id: String,
        known: String,
    },
    #[error("Empty response from model")]
    Empty,
    #[error("Generated text contains forbidden brand variant(s): {hits}. Re-run generation.")]
    Forbidden { hits: String },
    #[error("{0}")]
    Message(String),
}

/// HTTP / client failure from `XaiClient` (`RuntimeError` / request errors).
#[derive(Debug, Error)]
pub enum LlmError {
    #[error("LLM request failed: {0}")]
    Http(String),
}

/// Finalize failure (`symsight.finalize.FinalizeError`).
#[derive(Debug, Error)]
pub enum FinalizeError {
    #[error("Draft not found: {0}")]
    NotFound(PathBuf),
    #[error("Brand check failed on draft ({hits}). Fix before finalizing.")]
    BrandCheck { hits: String },
    #[error("Destination already exists: {0}")]
    DestinationExists(PathBuf),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// Missing API key (`RuntimeError` from `AppConfig.require_api_key`).
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error(
        "XAI_API_KEY is not set. Export it or add it to a git-ignored .env file.\n  export XAI_API_KEY=...\n  # see .env.example"
    )]
    MissingApiKey,
}

/// Title / body / social parse failure (`ValueError` in `symsight.textutil`).
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TextError {
    #[error("Could not parse TITLE/body from model output")]
    EmptyTitleBody,
    #[error("Parsed empty body from model output")]
    EmptyBody,
    #[error("Empty social text from model output")]
    EmptySocial,
}
