# Contributing to symsight

Thanks for contributing.

## Philosophy

Symsight keeps generation **brand-driven** (YAML), **reproducible** (`uv`), and **local-first** for drafts. Prefer small, testable changes over large rewrites.

## AI-assisted contributions

AI tools are fine. You remain responsible for the result: explain the change, keep quality high, and own the code after merge.

## Getting started

1. Fork/clone the repository.
2. Set up the environment (see [DEVELOPMENT.md](DEVELOPMENT.md)).
3. Branch from **`worx`** (`git checkout -b feature/your-feature-name`).
4. Make changes; run `uv run pytest`, `cargo test --workspace`, and `uv run ruff check src tests`. Need Rust 1.85+ (`uv sync` builds the native extension).
5. Open a PR into **`worx`**.

## Pull requests

- Keep PRs focused (one logical change when practical).
- Include tests when adding or changing behavior.
- Update docs / `CHANGELOG.md` under **Unreleased** when user-visible.
- Stay engaged with review comments.

## Release path

GitHub Flow, same as SymWorx. Feature PRs go to **`worx`**. A release is a version bump + changelog on `worx`, then a **manual** tag `vX.Y.Z`:

```text
feature/*  ──PR──►  worx  ──tag──►  vX.Y.Z
```

Until GitHub renames the default branch, PRs still target **`develop`**. Details: [DEVELOPMENT.md](DEVELOPMENT.md#branch-model).

## Security

To report a security issue, see [SECURITY.md](SECURITY.md) (not public issues).
