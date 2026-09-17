# Development

How to build, test, and release **symsight**.

Branch model and release gates mirror [SymWorx](https://github.com/symworx/symworx) so both projects use the same muscle memory.

For agent/tooling guidelines, see [AGENTS.md](AGENTS.md). Contributors own all submitted code.

## Prerequisites

- Python 3.11+ (3.12 recommended; CI matrix is 3.11 + 3.12)
- [uv](https://docs.astral.sh/uv/)
- A SpaceXAI / xAI API key for live generation (`XAI_API_KEY`)
- Rust 1.85+ (stable) with `rustfmt` and `clippy` — required to install the package (`uv sync` builds `symsight._native` via maturin) and for Cargo tests. Domain logic is Rust. `rust-toolchain.toml` pins `stable` and the extra components.

## Common commands

```bash
# Install (runtime)
uv sync

# Install with lint/test tools
uv sync --extra dev

# Tests
uv run pytest

# TUI (Textual; uses the Rust core by default)
uv run symsight tui

# Lint
uv run ruff check src tests

# Optional typecheck
uv run mypy src

# Rust workspace (fmt, clippy, tests)
cargo +nightly fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Native clap binary (headless; `tui` still needs the Python package)
cargo run -p symsight-cli -- brands
cargo run -p symsight-cli -- check
cargo run -p symsight-cli -- tui   # prints a hint; use uv run symsight tui

# CLI / TUI
uv run symsight brands
uv run symsight generate --brand example-writer --type general --topic "deep work"
uv run symsight tui
```

Config:

```bash
cp .env.example .env
# optional: cp config/.symsight.toml.example .symsight.toml
```

## Code style

- Run `uv run ruff check src tests` before committing.
- For Rust changes, also run `cargo +nightly fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`. Canonical rustfmt is nightly (family CI); `rust-toolchain.toml` stays stable for build.
- Keep changes focused; one logical change per PR is preferred.
- Generation tests that hit the network should stay mocked (see `tests/test_generate_mocked.py`).
- Text helpers are locked by `tests/golden/*.json` (consumed by both `cargo test` and pytest). Regenerate only with `uv run python scripts/export_goldens.py` when you intend to change the Python contract — not after `textutil` becomes a Rust shim.

## Project layout

| Path | Role |
|------|------|
| `src/symsight/` | Python package (CLI + Textual TUI; shims over `symsight._native`) |
| `crates/symsight-core/` | Rust domain crate (default implementation) |
| `crates/symsight-cli/` | Native clap binary (`cargo run -p symsight-cli`) |
| `crates/symsight-py/` | PyO3 cdylib published as `symsight._native` |
| `config/brands/` | Brand YAML (example only in-repo) |
| `content/drafts/`, `content/final/` | Local drafts (usually not released) |
| `scripts/` | Thin entrypoints wrapping the library |
| `tests/` | Pytest suite |

Version is single-sourced from `[workspace.package].version` in the root `Cargo.toml`. Members use `version.workspace = true`. Internal path deps (`symsight-core`) live once under `[workspace.dependencies]` (workspace-relative path + version pin — Cargo strips the path on publish and forbids `version.workspace = true` there). `pyproject.toml` uses `dynamic = ["version"]` (maturin reads the cdylib crate). Release-meta and `./scripts/bump-version.sh` read the workspace version.

## Branch model

Same as SymWorx:

```
feature/* ──► develop ──► stage ──► release/vX.Y.Z ──► main ──► tag vX.Y.Z
                 │           │              │             │
              day-to-day   FF only      validation     publish
                 CI        (no CI)     (no bump)       on tag
```

Version bumps are a **`develop` chore**, not a step on `release/vX.Y.Z`. Same muscle memory as SymWorx: land the number on `develop`, then promote that SHA.

1. **Feature development** → merge to `develop`  
   Day-to-day CI (ruff + pytest + Rust fmt/clippy/test) runs on push/PR to `develop` only.

2. **Version bump** (manual, on `develop`, before the cycle)  
   When the next release is `X.Y.Z`:
   - `./scripts/bump-version.sh patch --changelog` (or `minor` / `set X.Y.Z`). The script rewrites `[workspace.package] version`, the `[workspace.dependencies]` `symsight-*` path+version pin (Cargo cannot inherit `version.workspace = true` there), and `Cargo.lock` member versions. Member crates stay on `version.workspace = true` and `symsight-core = { workspace = true }`.
   - Fill in `CHANGELOG.md` under `## [X.Y.Z]` (required later by release metadata).
   - PR into `develop`. CI must be green.

3. **Stage / early access** → fast-forward `develop` → `stage` when you want a promotion point.  
   Day-to-day CI does **not** run on `stage` (avoids double runs on FF). Pre-release tags (e.g. `v0.2.0-beta.1`) may be cut from here if needed.

4. **Release branch** (no bump)
   - Create `release/vX.Y.Z` from `stage` (or from `develop` if stage is not updated yet).
   - Branch name must already match `[workspace.package] version` and `CHANGELOG.md` must have `## [X.Y.Z]` (`release-meta` fails otherwise).
   - Open a PR from `release/vX.Y.Z` → `main`.

5. **Release**
   - Merge the PR to `main` when **Release** checks are green.
   - **Manually** create and push the annotated tag `vX.Y.Z` on the merge commit (tags are not auto-created in CI).
   - Tag push runs [`.github/workflows/release.yml`](.github/workflows/release.yml): full validation, then **GitHub Release** (manylinux sdist/wheel and a linux x86_64 GNU `symsight` binary) **and** `publish-crates` (`symsight-core` then `symsight-cli` to crates.io). The two publish jobs are independent: a crates.io failure does not block the GitHub Release. **PyPI is paused** until `publish-pypi` is re-enabled.
   - crates.io needs GitHub Environment `crates-io` with secret `CARGO_REGISTRY_TOKEN`. First publish of a version can also be done by hand (`cargo publish -p symsight-core` then `-p symsight-cli`); later tags use the workflow.

`./scripts/bump-version.sh` with no args is a **consistency check** (workspace pin and lockfile). Day-to-day `rust-checks` and Release `rust-checks` run it so a missed pin fails CI. It does not bump. A missing `## [X.Y.Z]` heading is reported but only **release-meta** requires it.

### Example (`v0.2.1` after a develop bump)

```bash
# Version + changelog already on develop (e.g. chore/bump-version merged).
git checkout stage && git pull
git merge --ff-only origin/develop
git push origin stage

git checkout -b release/v0.2.1
git push -u origin release/v0.2.1
# open PR → main, merge when green

git checkout main && git pull
git tag -a v0.2.1 -m "v0.2.1"
git push origin v0.2.1
```

### Versioning

- Semantic Versioning (SemVer).
- Pre-releases (e.g. `0.2.0-beta.1`, `0.2.0-rc.1`) may be tagged from `stage` or a release branch; mark them as prerelease in GitHub (`-` in the version).
- Bump on `develop` first; do not bump on `release/vX.Y.Z`.
- Final releases are cut from `release/vX.Y.Z` merged into `main`, then **manually** tagged.

## CI / release automation

| Workflow | Triggers | What it does |
|----------|----------|--------------|
| [`.github/workflows/ci.yml`](.github/workflows/ci.yml) | push/PR → `develop`; dispatch | `fmt` + `rust-checks` + `python-bindings` (same job ids as SymWorx) |
| [`.github/workflows/release.yml`](.github/workflows/release.yml) | PR → `main`; push `release/**` / tags `v*`; dispatch | Version + CHANGELOG gates, fmt, rust-checks, python-bindings, manylinux wheel, native binary; **GitHub Release + crates.io** only on tags |

Release metadata enforces:

- `release/vX.Y.Z` branch name matches `Cargo.toml` workspace version.
- Tag `vX.Y.Z` matches `Cargo.toml` workspace version.
- `CHANGELOG.md` contains `## [X.Y.Z]` for tags and `release/*` branches.

## Bootstrap remote branches (once)

If the remote only has `main` so far:

```bash
git checkout main
git pull
git checkout -b develop
git push -u origin develop
git checkout -b stage
git push -u origin stage
# optional: set default branch to develop in GitHub settings
```

### Repository rulesets (once the repo is public)

GitHub Free does not allow rulesets on private repositories. After making `symworx/symsight` public, apply the same rulesets as SymWorx / SymKit (org-admin bypass, so `git push --admin` still works):

```bash
./scripts/apply-github-rulesets.py
```

That creates `develop` (requires `fmt`, `rust-checks`, `python-bindings`, same job ids as SymWorx), `stage-main`, `release-branches`, `topic-no-force-push`, and `version-tags`. Re-running the script updates them in place.

## Related

- [CONTRIBUTING.md](CONTRIBUTING.md)
- [CHANGELOG.md](CHANGELOG.md)
- [README.md](README.md)
