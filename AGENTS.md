# AGENTS.md — symsight

Instructions for agentic development tools working in this repository.

All changes must be owned by the human contributor, who reviews and maintains the code.

## Project overview

**symsight** generates brand-voiced articles and social posts via SpaceXAI / xAI. Config lives in YAML brands; output is markdown drafts under `content/`.

| Path | Role |
|------|------|
| `src/symsight/` | Library (CLI, TUI, Pydantic models; shims over `symsight._native`) |
| `config/brands/` | Brand YAML (ship example only) |
| `tests/` | Pytest (mock the LLM for unit tests) |
| `.github/workflows/` | CI + release (GitHub Flow; tags on `worx`) |

## Branch model (do not invent a different one)

GitHub Flow, same as SymWorx:

1. Feature work → **`worx`** (day-to-day CI).
2. **Manual** version bump on `worx` when a release is next (`./scripts/bump-version.sh`, changelog `## [X.Y.Z]`).
3. Optional freeze: `release/vX.Y.Z` off `worx`, PR it back.
4. **Manually** tag `vX.Y.Z` on `worx` (no auto-tag job). Tag runs publish (GitHub Release; crates.io; PyPI paused).

Until GitHub renames the default branch, PRs still target **`develop`**. See [DEVELOPMENT.md](DEVELOPMENT.md).

## Working style

- Prefer incremental, working changes.
- Do not commit secrets (`.env`), personal brand files, or real draft content unless the user asks.
- Keep generation tests mocked; do not call the live API in CI.
- Match existing patterns in `src/symsight/` (Pydantic models, brand YAML, thin CLIs).

## Common commands

```bash
uv sync --extra dev
uv run pytest
uv run ruff check src tests
cargo test --workspace
uv run symsight --help
```
