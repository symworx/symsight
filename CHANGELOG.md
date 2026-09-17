# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.3] - 2026-09-17

### Changed

- Copyright holder updated to Nathaniel T. Berry
- Repository URLs updated from `github.com/csymd` to `github.com/symworx`

## [0.2.2] - 2026-08-28

### Added

- None

### Changed 

- Address ignored conflicts in changelog

## [0.2.1] - 2026-08-28

### Added

- Tag releases publish `symsight-core` then `symsight-cli` to crates.io (`publish-crates` job)

### Changed

- Internal crates depend on `symsight-core` from `[workspace.dependencies]` (workspace-relative path); `bump-version.sh` keeps that pin, the lockfile, and changelog version links in lockstep with `[workspace.package] version`

## [0.2.0] - 2026-08-28

### Added

- `scripts/bump-version.sh` for lockstep Cargo workspace version bumps
- `scripts/apply-github-rulesets.py` to install the family branch/tag rulesets once the repo is public

### Changed

- Day-to-day CI runs on `develop` only; Release validation runs on PRs into `main`, pushes to `release/**`, and tags `v*` (no second full run on push to `main`)
- Canonical rustfmt in CI is nightly (stable toolchain still builds)
- CI job ids match family develop checks: `fmt`, `rust-checks`, `python-bindings`
- Package, SECURITY, and changelog URLs use `github.com/symworx/symsight`
- `uv run symsight` with no subcommand prints help (was an argparse error)

### Removed

- Legacy Python domain fallback (`SYMSIGHT_IMPL=python`, `--extra legacy`, `src/symsight/_py/`)

### Security

- Redact `XAI_API_KEY` from `AppConfig` Debug/repr and LLM HTTP error bodies
- Reject path separators / `..` in draft stems and `--brand` ids
- Pin GitHub Actions to commit SHAs; checkout does not persist credentials
- Bump `pyo3` to 0.29.2 (GHSA-36hh-v3qg-5jq4, GHSA-chgr-c6px-7xpp, GHSA-pph8-gcv7-4qj5)
- Bump `serde_yml` to 0.0.13 (drops `libyml`; GHSA-gfxp-f68g-8x78, GHSA-hhw4-xg65-fp2x)

## [0.1.0] - 2026-08-16

### Added

- Initial release of **symsight**: brand-driven articles and social posts from YAML brand files
- CLI (`generate`, `finalize`, `check`, `brands`, `tui`) and Textual TUI
- Rust domain core (`symsight-core`) with Python bindings (`symsight._native` via PyO3 / maturin)
- Native clap binary (`cargo run -p symsight-cli`); GitHub Release also ships `symsight-v*-x86_64-unknown-linux-gnu`
- GitHub Release artifacts: manylinux wheel + sdist (PyPI publish remains paused)
- Example brand, mini-project workspace, `SECURITY.md`, `CODE_OF_CONDUCT.md`, and `NOTICE.md`

### Changed

- Default implementation is Rust; `SYMSIGHT_IMPL=python` is a one-release fallback (`uv sync --extra legacy`)
- Package version is sourced from the Cargo workspace (`pyproject.toml` is `dynamic = ["version"]`)
- Brandcheck skip dirs include `target` and `dist` so Cargo / pack artifacts are not scanned

### Notes

- `SymSight` is 0.x / Beta (`Development Status :: 4 - Beta`). The API may change until 1.0.
- Generation tests stay mocked; CI does not call the live xAI API.

---

## Version Links

[0.2.3]: https://github.com/symworx/symsight/releases/tag/v0.2.3
[0.2.2]: https://github.com/symworx/symsight/releases/tag/v0.2.2
[0.2.1]: https://github.com/symworx/symsight/releases/tag/v0.2.1
[0.2.0]: https://github.com/symworx/symsight/releases/tag/v0.2.0
[0.1.0]: https://github.com/symworx/symsight/releases/tag/v0.1.0
