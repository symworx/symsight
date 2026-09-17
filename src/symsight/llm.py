# Copyright (c) 2026, Nathaniel Berry
# Licensed under the Apache License, Version 2.0.
"""xAI / SpaceXAI client helpers (Rust core)."""

from __future__ import annotations

from typing import Any

from symsight._native import make_client as _make_client
from symsight._native import response_text as _response_text


def make_client(*, api_key: str, base_url: str | None = None) -> Any:
    return _make_client(api_key, base_url)


def response_text(response: object) -> str:
    return str(_response_text(response))


def create_completion(*args: Any, **kwargs: Any) -> str:
    raise RuntimeError(
        "create_completion is not used; generate() talks to the Rust LlmClient directly"
    )
