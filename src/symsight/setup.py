# Copyright (c) 2026, Nathaniel T. Berry
# Licensed under the Apache License, Version 2.0.
"""Project setup: model, base URL, and the remote API key file."""

from __future__ import annotations

from symsight._native import apply_setup as _apply_setup
from symsight._native import list_model_ids as _list_model_ids
from symsight.config import AppConfig


def apply_setup(
    cfg: AppConfig,
    *,
    local: bool,
    base_url: str,
    model: str,
    api_key: str | None = None,
) -> dict[str, object]:
    """Save model settings. A remote key is written only to ``.env``."""
    return _apply_setup(cfg, local, base_url, model, api_key)


def list_model_ids(base_url: str) -> list[str]:
    """Ask a local server which model ids it is serving."""
    return list(_list_model_ids(base_url))
