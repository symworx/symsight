# Examples

Loose layouts you can copy and run. Nothing here is required for installing
`symsight`; the main app defaults still live under `config/` and `content/`.

## Prerequisites

From the **repository root**:

```bash
uv sync
cp .env.example .env   # set XAI_API_KEY
```

## Layout

```
examples/
  brands/                 # extra brand YAMLs (not the install default)
    sample-business.yaml  # educational finance / business voice
  mini-project/           # self-contained workspace you can cd into
    .symsight.toml
    brands/               # copy or symlink your brands here
    drafts/
    final/
  sample-drafts/          # static sample markdown (no API call)
    article-outline.md
    social-post.md
```

## 1. Use a sample brand from the repo root

```bash
# List brands (default dir is config/brands)
uv run symsight brands

# Point at the examples brand file
uv run symsight generate \
  --brand-file examples/brands/sample-business.yaml \
  --type market-brief \
  --tone briefing \
  --topic "what rising rates mean for short-duration bond funds" \
  --min-words 200 \
  --max-words 400
```

Social post:

```bash
uv run symsight generate \
  --brand-file examples/brands/sample-business.yaml \
  --format social \
  --type social-tip \
  --topic "one diversification reminder for long-term investors" \
  --max-chars 200
```

Drafts land under `content/drafts/` unless you override with config or
`--drafts-dir`.

## 2. Run inside the mini-project workspace

```bash
cd examples/mini-project

# Brand is already wired via .symsight.toml → brands/sample-business.yaml
# (relative paths are from this directory)

# From repo root you can also set config root:
cd ../..
uv run symsight --config-root examples/mini-project brands
uv run symsight --config-root examples/mini-project generate \
  --type market-brief \
  --topic "how businesses budget for software subscriptions"
```

Or copy the mini-project elsewhere and treat it as a personal workspace:

```bash
cp -R examples/mini-project ~/symsight-workspace
cd ~/symsight-workspace
# edit brands/, set XAI_API_KEY in env or parent .env
```

## 3. Static samples (no API)

Open `sample-drafts/` for the expected draft shape (front matter + body +
disclaimer pattern). Use these as a review checklist when the model output
looks off.

## Notes

- Brands here are **generic and educational** — not a real firm, not advice.
- Keep real client brands and generated drafts out of public git history.
- Forbidden-term scan:

  ```bash
  uv run symsight check --brand-file examples/brands/sample-business.yaml examples/sample-drafts
  ```
