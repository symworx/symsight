// Copyright (c) 2026, Nathaniel Berry
// Licensed under the Apache License, Version 2.0.

//! Domain core for the symsight insight generator.
//!
//! Python remains the user-facing implementation until later PRs land
//! bindings. This crate currently owns brand models, YAML loading, textutil,
//! draft I/O, finalize, brandcheck, config, prompts, and generate.

pub mod brand;
pub mod brandcheck;
pub mod config;
pub mod draft_io;
pub mod error;
pub mod finalize;
pub mod generate;
pub mod llm;
pub mod models;
pub mod prompts;
pub mod textutil;

pub use brand::{list_brand_files, list_brands, load_brand_file, resolve_brand};
pub use brandcheck::{check_path, check_text, find_hits, iter_scan_files, scan_paths};
pub use config::{
    find_project_root, get_config, save_project_config, AppConfig, ConfigOverrides,
    DEFAULT_BASE_URL, DEFAULT_MODEL,
};
pub use draft_io::{
    draft_meta_json, list_drafts, parse_front_matter, read_draft, render_front_matter,
    save_draft_body, set_status, strip_disclaimer_from_body, unique_draft_path,
    write_draft_content, write_meta_json, write_new_draft, WriteNewDraft,
};
pub use error::{BrandError, ConfigError, FinalizeError, GenerateError, LlmError, TextError};
pub use finalize::{finalize_draft, list_final};
pub use generate::{generate_and_write, generate_content};
pub use llm::{
    redact_secrets, response_text, CompletionRequest, LlmClient, ScriptedClient, XaiClient,
};
pub use models::{
    ArticleFormatSpec, Brand, ContentFormat, Draft, DraftMeta, FormatSpecs, FrontValue,
    GenerateRequest, SocialFormatSpec, TypeSpec,
};
pub use prompts::{length_rewrite_prompt, safe_format, system_prompt, user_prompt};
pub use textutil::{
    char_count, clean_body, extract_social_text, extract_title_body, is_plausible_title,
    is_safe_path_component, slugify, word_count,
};

/// Workspace package version, single-sourced from the root `Cargo.toml`.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    /// Parse the git-workspace root so this assertion survives version bumps.
    /// Skips when the crate is tested outside the monorepo (crates.io sources
    /// do not include the workspace `Cargo.toml`).
    fn workspace_root_toml() -> Option<toml::Table> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
        if !path.is_file() {
            return None;
        }
        let text = std::fs::read_to_string(&path).ok()?;
        toml::from_str(&text).ok()
    }

    #[test]
    fn version_matches_workspace() {
        let Some(root) = workspace_root_toml() else {
            return;
        };
        let workspace = root
            .get("workspace")
            .and_then(|v| v.as_table())
            .expect("root Cargo.toml [workspace]");
        let pkg_ver = workspace
            .get("package")
            .and_then(|v| v.as_table())
            .and_then(|t| t.get("version"))
            .and_then(|v| v.as_str())
            .expect("[workspace.package] version");
        assert_eq!(crate::version(), pkg_ver);

        let core = workspace
            .get("dependencies")
            .and_then(|v| v.as_table())
            .and_then(|t| t.get("symsight-core"))
            .and_then(|v| v.as_table())
            .expect("[workspace.dependencies] symsight-core");
        assert_eq!(
            core.get("path").and_then(|v| v.as_str()),
            Some("crates/symsight-core"),
            "internal crate must be a workspace-relative path dep"
        );
        assert_eq!(
            core.get("version").and_then(|v| v.as_str()),
            Some(pkg_ver),
            "workspace.dependencies.symsight-core.version must stay in lockstep with workspace.package.version"
        );
    }
}
