// Copyright (c) 2026, Nathaniel T. Berry
// Licensed under the Apache License, Version 2.0.

//! Flag-compatible clap CLI. `uv run symsight` remains the Python entrypoint
//! until a later flip PR; this binary is opt-in via `cargo run -p symsight-cli`.

use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum};
use symsight_core::{
    apply_setup, finalize_draft, generate_and_write, get_config, list_brands, list_model_ids,
    prepare_image_request, render_image_with_xai, resolve_brand, scan_paths, AppConfig,
    ConfigOverrides, ContentFormat, GenerateRequest, ImageOverrides, SetupRequest,
    DEFAULT_BASE_URL, DEFAULT_LOCAL_BASE_URL, DEFAULT_MODEL,
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
        /// Tone id from the brand (alias: --voice). Default: brand default_tone
        #[arg(long, visible_alias = "voice")]
        tone: Option<String>,
        /// File to read into the prompt (repeatable). Must sit inside the project root.
        #[arg(long = "context")]
        context: Vec<PathBuf>,
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
    /// Generate an image from the brand file's image.style
    Image {
        /// What the panel shows
        #[arg(long)]
        prompt: String,
        /// Destination image path
        #[arg(long)]
        out: PathBuf,
        /// Override the brand aspect ratio
        #[arg(long)]
        aspect: Option<String>,
        /// Override the brand quality (low, medium, auto)
        #[arg(long)]
        quality: Option<String>,
        /// Override the brand image model
        #[arg(long)]
        model: Option<String>,
        /// Brand id (default: active_brand from config)
        #[arg(long)]
        brand: Option<String>,
        /// Path to brand YAML
        #[arg(long)]
        brand_file: Option<PathBuf>,
    },
    /// Save the model and base URL for this project
    Setup {
        /// Use a localhost OpenAI-compatible model
        #[arg(long)]
        local: bool,
        /// Use the hosted xAI API
        #[arg(long)]
        remote: bool,
        /// API base URL (default depends on --local / --remote)
        #[arg(long)]
        base_url: Option<String>,
        /// Model id
        #[arg(long)]
        model: Option<String>,
        /// Remote XAI_API_KEY. Prefer `uv run symsight setup`; this flag lands in shell history.
        #[arg(long)]
        api_key: Option<String>,
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
        Command::Image { .. } => cmd_image(&cli),
        Command::Setup { .. } => cmd_setup(&cli),
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
        tone,
        context,
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
        tone: tone.clone(),
        context_files: context.clone(),
    };

    let requested = tone.as_deref().map(str::trim).filter(|id| !id.is_empty());
    let tone_label = match req.resolved_tone() {
        Ok(Some((id, _))) => {
            if requested.is_none() {
                eprintln!("Using default tone: {id}");
            }
            format!(", tone {id}")
        }
        Ok(None) => String::new(),
        Err(exc) => {
            eprintln!("Generation failed: {exc}");
            return 1;
        }
    };

    println!(
        "Generating {} ({type_id}{tone_label}) for {}…",
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

fn cmd_image(cli: &Cli) -> i32 {
    let Command::Image {
        prompt,
        out,
        aspect,
        quality,
        model,
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
    if let Err(exc) = prepare_image_request(
        &brand,
        prompt,
        aspect.as_deref(),
        quality.as_deref(),
        model.as_deref(),
    ) {
        eprintln!("Image failed: {exc}");
        return 1;
    }
    let key = match cfg.require_api_key() {
        Ok(key) => key,
        Err(exc) => {
            eprintln!("{exc}");
            return 1;
        }
    };
    let dest = if out.is_absolute() {
        out.clone()
    } else {
        cfg.project_root.join(out)
    };
    let path = match render_image_with_xai(
        &brand,
        prompt,
        &dest,
        ImageOverrides {
            aspect: aspect.as_deref(),
            quality: quality.as_deref(),
            model: model.as_deref(),
        },
        &key,
        &cfg.base_url,
    ) {
        Ok(path) => path,
        Err(exc) => {
            eprintln!("Image failed: {exc}");
            return 1;
        }
    };
    let rel = relative_to(&path, &cfg.project_root);
    println!("Image written: {}", rel.display());
    0
}

fn cmd_setup(cli: &Cli) -> i32 {
    let Command::Setup {
        local,
        remote,
        base_url,
        model,
        api_key,
    } = &cli.command
    else {
        unreachable!();
    };
    if *local && *remote {
        eprintln!("pass only one of --local or --remote");
        return 1;
    }
    let choice = match setup_choice(*local, *remote) {
        Ok(choice) => choice,
        Err(msg) => {
            eprintln!("{msg}");
            return 1;
        }
    };
    let cfg = get_config(cli.config_root.as_deref(), None);
    let prepared = match prepare_setup(&cfg, choice, base_url, model, api_key) {
        Ok(prepared) => prepared,
        Err(msg) => {
            eprintln!("{msg}");
            return 1;
        }
    };
    match apply_setup(&cfg, &prepared.request) {
        Ok(report) => {
            print_setup_report(&report, prepared.server_down);
            0
        }
        Err(err) => {
            eprintln!("{err}");
            1
        }
    }
}

enum SetupChoice {
    Local,
    Remote,
}

struct PreparedSetup {
    request: SetupRequest,
    server_down: bool,
}

fn setup_choice(local: bool, remote: bool) -> Result<SetupChoice, String> {
    if local {
        return Ok(SetupChoice::Local);
    }
    if remote {
        return Ok(SetupChoice::Remote);
    }
    if !io::stdin().is_terminal() {
        return Err("pass --remote or --local".into());
    }
    let answer = prompt_line("Write with a remote xAI model or a local model? [remote]: ")?;
    match answer.to_ascii_lowercase().as_str() {
        "" | "remote" | "r" => Ok(SetupChoice::Remote),
        "local" | "l" => Ok(SetupChoice::Local),
        _ => Err("answer remote or local".into()),
    }
}

fn prepare_setup(
    cfg: &AppConfig,
    choice: SetupChoice,
    base_url: &Option<String>,
    model: &Option<String>,
    api_key: &Option<String>,
) -> Result<PreparedSetup, String> {
    match choice {
        SetupChoice::Local => prepare_local(base_url, model, api_key),
        SetupChoice::Remote => prepare_remote(cfg, base_url, model, api_key),
    }
}

fn prepare_local(
    base_url: &Option<String>,
    model: &Option<String>,
    api_key: &Option<String>,
) -> Result<PreparedSetup, String> {
    if api_key.as_deref().is_some_and(|key| !key.trim().is_empty()) {
        return Err("local setup does not take an API key".into());
    }
    let base_url = base_url
        .clone()
        .unwrap_or_else(|| DEFAULT_LOCAL_BASE_URL.to_string());
    let (model, server_down) = match model.clone() {
        Some(model) => (model, false),
        None => pick_local_model(&base_url)?,
    };
    Ok(PreparedSetup {
        request: SetupRequest {
            local: true,
            base_url,
            model,
            api_key: None,
        },
        server_down,
    })
}

fn pick_local_model(base_url: &str) -> Result<(String, bool), String> {
    if !io::stdin().is_terminal() {
        return Err("pass --model".into());
    }
    match list_model_ids(base_url) {
        Ok(ids) if !ids.is_empty() => {
            for (index, id) in ids.iter().enumerate() {
                println!("  {}. {id}", index + 1);
            }
            let default = ids[0].clone();
            let answer = prompt_line(&format!("Model [{default}]: "))?;
            if answer.is_empty() {
                return Ok((default, false));
            }
            if let Ok(n) = answer.parse::<usize>() {
                if (1..=ids.len()).contains(&n) {
                    return Ok((ids[n - 1].clone(), false));
                }
            }
            Ok((answer, false))
        }
        Ok(_) => {
            eprintln!("No models reported at {base_url}.");
            let answer = prompt_line("Model id: ")?;
            if answer.is_empty() {
                return Err("pass --model".into());
            }
            Ok((answer, false))
        }
        Err(err) => {
            eprintln!("Could not list models at {base_url}: {err}");
            let answer = prompt_line("Model id: ")?;
            if answer.is_empty() {
                return Err("pass --model".into());
            }
            Ok((answer, true))
        }
    }
}

fn prepare_remote(
    cfg: &AppConfig,
    base_url: &Option<String>,
    model: &Option<String>,
    api_key: &Option<String>,
) -> Result<PreparedSetup, String> {
    let base_url = base_url
        .clone()
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    let model = model.clone().unwrap_or_else(|| DEFAULT_MODEL.to_string());
    let api_key = resolve_remote_key(cfg, api_key)?;
    Ok(PreparedSetup {
        request: SetupRequest {
            local: false,
            base_url,
            model,
            api_key,
        },
        server_down: false,
    })
}

fn resolve_remote_key(cfg: &AppConfig, flag: &Option<String>) -> Result<Option<String>, String> {
    if let Some(key) = flag {
        if key.trim().is_empty() {
            return Err("API key is empty".into());
        }
        return Ok(Some(key.clone()));
    }
    if !cfg.xai_api_key.trim().is_empty() {
        return Ok(None);
    }
    Err("set XAI_API_KEY or pass --api-key. For a hidden prompt, run: uv run symsight setup".into())
}

fn print_setup_report(report: &symsight_core::SetupReport, server_down: bool) {
    println!("Saved {}", report.config_path.display());
    println!("Model: {}", report.model);
    println!("Base URL: {}", report.base_url);
    if report.local {
        println!("Web search is unavailable on a local model. Images still use xAI.");
    }
    if server_down {
        println!("Generate will fail until the server is running.");
    }
    println!("symsight generate --type general --topic \"your topic\"");
}

fn prompt_line(question: &str) -> Result<String, String> {
    print!("{question}");
    io::stdout().flush().map_err(|err| err.to_string())?;
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|err| err.to_string())?;
    Ok(line.trim().to_string())
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
