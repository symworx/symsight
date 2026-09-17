// Copyright (c) 2026, Nathaniel Berry
// Licensed under the Apache License, Version 2.0.

//! Flag-compatible clap CLI. `uv run symsight` remains the Python entrypoint
//! until a later flip PR; this binary is opt-in via `cargo run -p symsight-cli`.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum};
use symsight_core::{
    finalize_draft, generate_and_write, get_config, list_brands, resolve_brand, scan_paths,
    ConfigOverrides, ContentFormat, GenerateRequest,
};

#[derive(Debug, Parser)]
#[command(
    name = "symsight",
    version,
    about = "Brand-agnostic insight generator (articles & social posts)"
)]
pub struct Cli {
    /// Project root for config resolution (default: auto)
    #[arg(long, global = true)]
    pub config_root: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum FormatArg {
    Article,
    Social,
}

impl From<FormatArg> for ContentFormat {
    fn from(value: FormatArg) -> Self {
        match value {
            FormatArg::Article => ContentFormat::Article,
            FormatArg::Social => ContentFormat::Social,
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Generate a draft
    Generate {
        /// Brand id (default: active_brand from config)
        #[arg(long)]
        brand: Option<String>,
        /// Path to brand YAML
        #[arg(long)]
        brand_file: Option<PathBuf>,
        /// Content type id from brand
        #[arg(long = "type")]
        type_id: Option<String>,
        /// Output format
        #[arg(long, value_enum, default_value = "article")]
        format: FormatArg,
        /// Topic / focus
        #[arg(long)]
        topic: Option<String>,
        #[arg(long)]
        min_words: Option<i64>,
        #[arg(long)]
        max_words: Option<i64>,
        /// Social max characters
        #[arg(long)]
        max_chars: Option<i64>,
        /// Force web_search on
        #[arg(long)]
        search: bool,
        /// Disable web_search (wins if both flags are passed)
        #[arg(long)]
        no_search: bool,
        /// Override model id
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        drafts_dir: Option<PathBuf>,
    },
    /// Move/copy draft to final directory
    Finalize {
        /// Path to draft markdown
        draft: PathBuf,
        /// Copy instead of move
        #[arg(long)]
        copy: bool,
        #[arg(long)]
        brand: Option<String>,
        #[arg(long)]
        brand_file: Option<PathBuf>,
        #[arg(long)]
        final_dir: Option<PathBuf>,
        #[arg(long)]
        skip_brand_check: bool,
    },
    /// Scan paths for forbidden brand terms
    Check {
        /// Files or dirs (default: drafts + final)
        paths: Vec<PathBuf>,
        #[arg(long)]
        brand: Option<String>,
        #[arg(long)]
        brand_file: Option<PathBuf>,
    },
    /// List configured brands
    Brands {
        #[arg(long)]
        brands_dir: Option<PathBuf>,
    },
    /// Launch the Textual TUI
    Tui {
        #[arg(long)]
        drafts_dir: Option<PathBuf>,
        #[arg(long)]
        final_dir: Option<PathBuf>,
        #[arg(long)]
        brand: Option<String>,
        #[arg(long)]
        brands_dir: Option<PathBuf>,
    },
}

pub fn resolve_use_search(search: bool, no_search: bool) -> Option<bool> {
    if no_search {
        Some(false)
    } else if search {
        Some(true)
    } else {
        None
    }
}

pub fn execute(cli: Cli) -> i32 {
    match cli.command {
        Command::Generate { .. } => cmd_generate(&cli),
        Command::Finalize { .. } => cmd_finalize(&cli),
        Command::Check { .. } => cmd_check(&cli),
        Command::Brands { .. } => cmd_brands(&cli),
        Command::Tui { .. } => cmd_tui(),
    }
}

fn cmd_generate(cli: &Cli) -> i32 {
    let Command::Generate {
        brand,
        brand_file,
        type_id,
        format,
        topic,
        min_words,
        max_words,
        max_chars,
        search,
        no_search,
        model,
        drafts_dir,
    } = &cli.command
    else {
        unreachable!();
    };

    let mut overrides = ConfigOverrides::default();
    if let Some(dir) = drafts_dir {
        overrides.drafts_dir = Some(dir.clone());
    }
    if let Some(b) = brand {
        overrides.active_brand = Some(b.clone());
    }
    if let Some(m) = model {
        overrides.model = Some(m.clone());
    }
    let cfg = get_config(cli.config_root.as_deref(), Some(&overrides));

    let brand = match resolve_brand(
        &cfg.brands_dir,
        brand.as_deref().or(Some(cfg.active_brand.as_str())),
        brand_file.as_deref(),
    ) {
        Ok(b) => b,
        Err(exc) => {
            eprintln!("Brand error: {exc}");
            return 1;
        }
    };

    let type_id = if let Some(t) = type_id.clone() {
        t
    } else {
        let Some(first) = brand.types.keys().next().cloned() else {
            eprintln!("Brand has no types defined.");
            return 1;
        };
        eprintln!("Using default type: {first}");
        first
    };

    let fmt = ContentFormat::from(*format);
    let req = GenerateRequest {
        brand: brand.clone(),
        type_id: type_id.clone(),
        format: fmt,
        topic: topic.clone(),
        min_words: *min_words,
        max_words: *max_words,
        max_chars: *max_chars,
        use_search: resolve_use_search(*search, *no_search),
        model: model.clone(),
    };

    println!(
        "Generating {} ({type_id}) for {}…",
        fmt.as_str(),
        brand.display_name
    );
    let path = match generate_and_write(&req, &cfg, None, None) {
        Ok(p) => p,
        Err(exc) => {
            eprintln!("Generation failed: {exc}");
            return 1;
        }
    };

    let rel = relative_to(&path, &cfg.project_root);
    println!("Draft written: {}", rel.display());
    println!(
        "Title: {}",
        path.file_name().unwrap_or_default().to_string_lossy()
    );
    println!("Review the draft, then finalize with:");
    println!("  symsight finalize {}", rel.display());
    0
}

fn cmd_finalize(cli: &Cli) -> i32 {
    let Command::Finalize {
        draft,
        copy,
        brand,
        brand_file,
        final_dir,
        skip_brand_check,
    } = &cli.command
    else {
        unreachable!();
    };

    let mut overrides = ConfigOverrides::default();
    if let Some(dir) = final_dir {
        overrides.final_dir = Some(dir.clone());
    }
    if let Some(b) = brand {
        overrides.active_brand = Some(b.clone());
    }
    let cfg = get_config(cli.config_root.as_deref(), Some(&overrides));

    let mut draft_path = draft.clone();
    if !draft_path.is_absolute() {
        let cwd = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(&draft_path);
        draft_path = if cwd.exists() {
            cwd
        } else {
            cfg.project_root.join(draft)
        };
    }

    let brand = if *skip_brand_check {
        None
    } else {
        match resolve_brand(
            &cfg.brands_dir,
            brand.as_deref().or(Some(cfg.active_brand.as_str())),
            brand_file.as_deref(),
        ) {
            Ok(b) => Some(b),
            Err(exc) => {
                eprintln!("Brand error: {exc}");
                return 1;
            }
        }
    };

    let dest = match finalize_draft(&draft_path, &cfg.final_dir, brand.as_ref(), *copy) {
        Ok(p) => p,
        Err(exc) => {
            eprintln!("Finalize failed: {exc}");
            return 1;
        }
    };
    let rel = relative_to(&dest, &cfg.project_root);
    println!("Final: {}", rel.display());
    0
}

fn cmd_check(cli: &Cli) -> i32 {
    let Command::Check {
        paths,
        brand,
        brand_file,
    } = &cli.command
    else {
        unreachable!();
    };

    let mut overrides = ConfigOverrides::default();
    if let Some(b) = brand {
        overrides.active_brand = Some(b.clone());
    }
    let cfg = get_config(cli.config_root.as_deref(), Some(&overrides));

    let brand = match resolve_brand(
        &cfg.brands_dir,
        brand.as_deref().or(Some(cfg.active_brand.as_str())),
        brand_file.as_deref(),
    ) {
        Ok(b) => b,
        Err(exc) => {
            eprintln!("Brand error: {exc}");
            return 1;
        }
    };

    if brand.forbidden.is_empty() {
        println!(
            "Brand '{}' has no forbidden terms; nothing to check.",
            brand.id
        );
        return 0;
    }

    let default_paths = [cfg.drafts_dir.clone(), cfg.final_dir.clone()];
    let scan: Vec<&Path> = if paths.is_empty() {
        default_paths.iter().map(PathBuf::as_path).collect()
    } else {
        paths.iter().map(PathBuf::as_path).collect()
    };
    let problems = scan_paths(&scan, &brand);
    if !problems.is_empty() {
        println!(
            "Brand check FAILED for {} — forbidden term(s) found.\n",
            brand.display_name
        );
        for (path, hits) in problems {
            let rel = relative_to(&path, &cfg.project_root);
            println!("  • {}: {}", rel.display(), hits.join(", "));
        }
        return 1;
    }
    println!(
        "Brand check passed for {}: no forbidden terms found.",
        brand.display_name
    );
    0
}

fn cmd_brands(cli: &Cli) -> i32 {
    let Command::Brands { brands_dir } = &cli.command else {
        unreachable!();
    };
    let mut overrides = ConfigOverrides::default();
    if let Some(dir) = brands_dir {
        overrides.brands_dir = Some(dir.clone());
    }
    let cfg = get_config(cli.config_root.as_deref(), Some(&overrides));
    let brands = list_brands(&cfg.brands_dir);
    if brands.is_empty() {
        println!("No brands found in {}", cfg.brands_dir.display());
        return 0;
    }
    for b in brands {
        let types = if b.types.is_empty() {
            "(no types)".to_string()
        } else {
            b.types.keys().cloned().collect::<Vec<_>>().join(", ")
        };
        let active = if b.id == cfg.active_brand { " *" } else { "" };
        println!("{}{active}: {}  types=[{types}]", b.id, b.display_name);
    }
    0
}

fn cmd_tui() -> i32 {
    let _ = writeln!(
        io::stderr(),
        "The native binary does not include the Textual TUI yet.\nUse: uv run symsight tui"
    );
    1
}

fn relative_to(path: &Path, root: &Path) -> PathBuf {
    path.strip_prefix(root)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| path.to_path_buf())
}
