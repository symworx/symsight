# symsight

`SymSight` is an insight generator meant to produce **blogs/articles** or **social posts** with a reproducible [uv](https://docs.astral.sh/uv/) environment, a Rust core (`symsight._native`), library + thin CLIs, and a **Textual** TUI.

Voice, firm name, disclaimers, forbidden terms, and content-type prompts live in **YAML brand files**.

## Setup

```bash
# Rust 1.85+ is required: uv sync compiles the native extension.
uv sync
cp .env.example .env   # set XAI_API_KEY from https://console.x.ai
```

The domain core is Rust (`symsight._native`). The Python package is the `uv run symsight` CLI and Textual TUI.

Headless native binary (no Python) after a GitHub Release, from crates.io, or locally:

```bash
cargo install symsight-cli   # installs the `symsight` binary
cargo run -p symsight-cli -- brands
```

Optional project config:

```bash
cp config/.symsight.toml.example .symsight.toml
```

## Brands

Brands are YAML files under `config/brands/` (configurable).

Ship only a generic example:

- `config/brands/example-writer.yaml`

Add your own (e.g. `config/brands/my-firm.yaml`) with:

- `voice`, `full_name`, `forbidden`, `disclaimer`
- `types` with `user_template` strings (`{today}`, `{topic}`, `{min_words}`, `{max_words}`, `{max_chars}`, `{full_name}`)
- `formats.article` / `formats.social` defaults

```bash
uv run symsight brands
```

## CLI

```bash
# Article (default type = first type on brand if omitted)
uv run symsight generate --brand example-writer --type general --topic "deep work"
uv run symsight generate --type news --topic "semiconductors" --search

# Social (≤ max chars, default 200 from brand)
uv run symsight generate --format social --type social-tip --topic "one rebalancing tip" --max-chars 200

# Finalize: move draft → final dir (use --copy to keep draft)
uv run symsight finalize content/drafts/<file>.md

# Forbidden-term scan (uses active brand)
uv run symsight check

# TUI
uv run symsight tui
```

Thin scripts (same behavior):

```bash
uv run python scripts/generate_insight.py --type general --topic "…"
uv run python scripts/finalize_insight.py content/drafts/….md
uv run python scripts/check_brand.py
uv run python scripts/tui.py
```

## TUI keys

| Key | Action |
|-----|--------|
| `g` | Generate |
| `s` | Save editor |
| `f` | Finalize (move to final dir) |
| `r` | Refresh draft list |
| `,` | Settings (drafts/final/brand paths) |
| `?` | Help |
| `q` | Quit |

## Examples

Runnable sample brands and a mini workspace (finance/business-oriented, generic):

```bash
# See examples/README.md for full walkthrough
uv run symsight generate \
  --brand-file examples/brands/sample-business.yaml \
  --type market-brief \
  --topic "what rising rates mean for short-duration bond funds"
```

## Layout

```
config/brands/          # default brand YAML (example-writer)
content/drafts/         # working drafts (gitignored bodies)
content/final/          # finalized markdown
examples/               # sample brands, mini-project, static drafts
src/symsight/           # Python package (shims + Textual TUI)
crates/                 # Rust core, clap CLI, PyO3 bindings
scripts/                # thin entrypoints
tests/
```

## Development

```bash
uv sync --extra dev
uv run pytest
uv run ruff check src tests
```

**SymWorx org standard** (GitHub Flow): feature PRs → **`worx`**, then a manual tag `vX.Y.Z`.  
Details: [DEVELOPMENT.md](DEVELOPMENT.md) · [CHANGELOG.md](CHANGELOG.md) · [CONTRIBUTING.md](CONTRIBUTING.md) · [SECURITY.md](SECURITY.md).

## License

Apache License 2.0 — see [LICENSE](LICENSE) and [NOTICE.md](NOTICE.md).

Copyright (c) 2026, Nathaniel T. Berry.

## Notes

- Generation uses SpaceXAI / xAI (`XAI_API_KEY`, `https://api.x.ai/v1`, default model `grok-4.5`).
- Domain logic is Rust (`symsight._native`). The Textual TUI remains Python.
- Finalize is **markdown only** (move/copy).
