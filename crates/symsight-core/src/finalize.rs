// Copyright (c) 2026, Nathaniel T. Berry
// Licensed under the Apache License, Version 2.0.

//! Promote drafts to the final directory (`src/symsight/finalize.py`).

use std::fs;
use std::path::{Path, PathBuf};

use crate::brandcheck::check_text;
use crate::draft_io::set_status;
use crate::error::FinalizeError;
use crate::models::Brand;

pub fn finalize_draft(
    draft_path: &Path,
    final_dir: &Path,
    brand: Option<&Brand>,
    copy: bool,
) -> Result<PathBuf, FinalizeError> {
    let draft_path = draft_path
        .canonicalize()
        .map_err(|_| FinalizeError::NotFound(draft_path.to_path_buf()))?;
    if !draft_path.is_file() {
        return Err(FinalizeError::NotFound(draft_path));
    }

    let raw = fs::read_to_string(&draft_path)?;
    if let Some(brand) = brand {
        let hits = check_text(&raw, brand);
        if !hits.is_empty() {
            return Err(FinalizeError::BrandCheck {
                hits: hits.join(", "),
            });
        }
    }

    fs::create_dir_all(final_dir)?;
    let dest = final_dir.join(
        draft_path
            .file_name()
            .expect("file name after canonicalize"),
    );
    if dest.exists() {
        let dest_res = dest.canonicalize().unwrap_or_else(|_| dest.clone());
        if dest_res != draft_path {
            return Err(FinalizeError::DestinationExists(dest));
        }
    }

    set_status(&draft_path, "final")?;

    let meta_src = draft_path.with_extension("meta.json");
    let meta_dest = dest.with_extension("meta.json");

    if copy {
        fs::copy(&draft_path, &dest)?;
        if meta_src.is_file() {
            fs::copy(&meta_src, &meta_dest)?;
        }
    } else {
        let dest_res = dest.canonicalize().unwrap_or_else(|_| dest.clone());
        if dest_res == draft_path {
            return Ok(dest_res);
        }
        move_file(&draft_path, &dest)?;
        if meta_src.is_file() {
            move_file(&meta_src, &meta_dest)?;
        }
    }

    dest.canonicalize().map_err(FinalizeError::from)
}

fn move_file(src: &Path, dest: &Path) -> std::io::Result<()> {
    if fs::rename(src, dest).is_ok() {
        return Ok(());
    }
    fs::copy(src, dest)?;
    fs::remove_file(src)?;
    Ok(())
}

pub fn list_final(final_dir: &Path) -> Vec<PathBuf> {
    if !final_dir.is_dir() {
        return Vec::new();
    }
    let mut paths: Vec<PathBuf> = fs::read_dir(final_dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("md"))
        .collect();
    paths.sort();
    paths.reverse();
    paths
}
