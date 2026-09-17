// Copyright (c) 2026, Nathaniel T. Berry
// Licensed under the Apache License, Version 2.0.

//! Config merge parity with `src/symsight/config.py`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use symsight_core::{
    find_project_root, get_config, save_project_config, AppConfig, ConfigError, ConfigOverrides,
    DEFAULT_MODEL,
};

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn tmp_dir() -> PathBuf {
    let base = std::env::temp_dir().join("symsight-core-config");
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

struct EnvGuard {
    saved: Vec<(String, Option<String>)>,
}

impl EnvGuard {
    fn apply(pairs: &[(&str, Option<&str>)]) -> Self {
        let mut saved = Vec::new();
        for (key, val) in pairs {
            saved.push(((*key).to_string(), std::env::var(key).ok()));
            match val {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
        Self { saved }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, val) in &self.saved {
            match val {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }
}

fn isolated_home(home: &Path) -> EnvGuard {
    EnvGuard::apply(&[
        ("HOME", Some(home.to_str().unwrap())),
        ("XAI_API_KEY", None),
        ("SYMSIGHT_XAI_API_KEY", None),
        ("xai_api_key", None),
        ("SYMSIGHT_MODEL", None),
        ("model", None),
        ("SYMSIGHT_ACTIVE_BRAND", None),
        ("active_brand", None),
        ("SYMSIGHT_BASE_URL", None),
        ("base_url", None),
        ("SYMSIGHT_BRANDS_DIR", None),
        ("SYMSIGHT_DRAFTS_DIR", None),
        ("SYMSIGHT_FINAL_DIR", None),
    ])
}

#[test]
fn find_project_root_markers() {
    let root = tmp_dir();
    let nested = root.join("a/b");
    fs::create_dir_all(&nested).unwrap();
    assert_eq!(find_project_root(Some(&nested)), nested);

    fs::write(root.join("Cargo.toml"), "[package]\nname=\"x\"\n").unwrap();
    assert_eq!(
        find_project_root(Some(&nested)),
        nested,
        "Cargo.toml must not be a root marker"
    );

    fs::write(root.join("pyproject.toml"), "[project]\nname=\"x\"\n").unwrap();
    assert_eq!(find_project_root(Some(&nested)), root);

    let other = tmp_dir();
    let deep = other.join("x");
    fs::create_dir_all(&deep).unwrap();
    fs::write(other.join(".symsight.toml"), "model = \"m\"\n").unwrap();
    assert_eq!(find_project_root(Some(&deep)), other);

    let brands = tmp_dir();
    fs::create_dir_all(brands.join("config/brands")).unwrap();
    assert_eq!(
        find_project_root(Some(&brands.join("config/brands"))),
        brands
    );
}

#[test]
fn file_beats_env_and_override_beats_file() {
    let _lock = ENV_LOCK.lock().unwrap();
    let root = tmp_dir();
    let home = root.join("home");
    fs::create_dir_all(&home).unwrap();
    let _env = isolated_home(&home);
    let _model = EnvGuard::apply(&[("SYMSIGHT_MODEL", Some("from-env"))]);
    fs::write(root.join(".symsight.toml"), "model = \"from-file\"\n").unwrap();
    let cfg = get_config(Some(&root), None);
    assert_eq!(cfg.model, "from-file");

    let cfg = get_config(
        Some(&root),
        Some(&ConfigOverrides {
            model: Some("from-cli".into()),
            ..ConfigOverrides::default()
        }),
    );
    assert_eq!(cfg.model, "from-cli");
}

#[test]
fn env_fills_keys_absent_from_file() {
    let _lock = ENV_LOCK.lock().unwrap();
    let root = tmp_dir();
    let home = root.join("home");
    fs::create_dir_all(&home).unwrap();
    let _env = isolated_home(&home);
    let _model = EnvGuard::apply(&[("SYMSIGHT_MODEL", Some("from-env"))]);
    fs::write(
        root.join(".symsight.toml"),
        "active_brand = \"from-file\"\n",
    )
    .unwrap();
    let cfg = get_config(Some(&root), None);
    assert_eq!(cfg.model, "from-env");
    assert_eq!(cfg.active_brand, "from-file");
}

#[test]
fn project_toml_beats_user_toml() {
    let _lock = ENV_LOCK.lock().unwrap();
    let root = tmp_dir();
    let home = root.join("home");
    fs::create_dir_all(home.join(".config/symsight")).unwrap();
    let _env = isolated_home(&home);
    fs::write(
        home.join(".config/symsight/config.toml"),
        "active_brand = \"user-brand\"\nmodel = \"user-model\"\n",
    )
    .unwrap();
    fs::write(
        root.join(".symsight.toml"),
        "active_brand = \"project-brand\"\n",
    )
    .unwrap();
    let cfg = get_config(Some(&root), None);
    assert_eq!(cfg.active_brand, "project-brand");
    assert_eq!(cfg.model, "user-model");
}

#[test]
fn save_project_config_preserves_unknown_keys() {
    let _lock = ENV_LOCK.lock().unwrap();
    let root = tmp_dir();
    let home = root.join("home");
    fs::create_dir_all(&home).unwrap();
    let _env = isolated_home(&home);
    let cfg = AppConfig {
        xai_api_key: String::new(),
        model: DEFAULT_MODEL.into(),
        base_url: "https://api.x.ai/v1".into(),
        active_brand: "fixture-co".into(),
        brands_dir: root.join("config/brands"),
        drafts_dir: root.join("content/drafts"),
        final_dir: root.join("content/final"),
        project_root: root.clone(),
    }
    .resolve_paths();
    let out = root.join(".symsight.toml");
    fs::write(&out, "keep_me = \"yes\"\n").unwrap();
    save_project_config(&cfg, Some(&out)).unwrap();
    let text = fs::read_to_string(&out).unwrap();
    assert_eq!(
        text,
        "# symsight project config\n\n\
         keep_me = \"yes\"\n\
         active_brand = \"fixture-co\"\n\
         brands_dir = \"config/brands\"\n\
         drafts_dir = \"content/drafts\"\n\
         final_dir = \"content/final\"\n\
         model = \"grok-4.5\"\n\
         base_url = \"https://api.x.ai/v1\"\n"
    );
}

#[test]
fn require_api_key_reads_project_dotenv_and_errors_clearly() {
    let _lock = ENV_LOCK.lock().unwrap();
    let root = tmp_dir();
    let home = root.join("home");
    fs::create_dir_all(&home).unwrap();
    let _env = isolated_home(&home);
    let cfg = get_config(Some(&root), None);
    let err = cfg.require_api_key().unwrap_err();
    assert_eq!(err, ConfigError::MissingApiKey);
    assert_eq!(
        err.to_string(),
        "XAI_API_KEY is not set. Export it or add it to a git-ignored .env file.\n  export XAI_API_KEY=...\n  # see .env.example"
    );

    fs::write(root.join(".env"), "XAI_API_KEY=from-dotenv\n").unwrap();
    assert_eq!(cfg.require_api_key().unwrap(), "from-dotenv");
}

#[test]
fn debug_redacts_api_key() {
    let cfg = AppConfig {
        xai_api_key: "xai-TESTSECRETNOTREAL".into(),
        model: DEFAULT_MODEL.into(),
        base_url: "https://api.x.ai/v1".into(),
        active_brand: "example-writer".into(),
        brands_dir: PathBuf::from("./config/brands"),
        drafts_dir: PathBuf::from("./content/drafts"),
        final_dir: PathBuf::from("./content/final"),
        project_root: PathBuf::from("."),
    };
    let dumped = format!("{cfg:?}");
    assert!(!dumped.contains("TESTSECRETNOTREAL"), "{dumped}");
    assert!(dumped.contains("[redacted]"), "{dumped}");

    let ov = ConfigOverrides {
        xai_api_key: Some("xai-TESTSECRETNOTREAL".into()),
        ..ConfigOverrides::default()
    };
    let ov_dumped = format!("{ov:?}");
    assert!(!ov_dumped.contains("TESTSECRETNOTREAL"), "{ov_dumped}");
}

#[test]
fn defaults_when_nothing_is_set() {
    let _lock = ENV_LOCK.lock().unwrap();
    let root = tmp_dir();
    let home = root.join("home");
    fs::create_dir_all(&home).unwrap();
    let _env = isolated_home(&home);
    let cfg = get_config(Some(&root), None);
    assert_eq!(cfg.model, DEFAULT_MODEL);
    assert_eq!(cfg.active_brand, "example-writer");
    assert_eq!(cfg.brands_dir, root.join("./config/brands"));
}
