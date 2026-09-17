# Copyright (c) 2026, Nathaniel Berry
# Licensed under the Apache License, Version 2.0.
"""Build system and user prompts from brand + format."""

from __future__ import annotations

from typing import Any

from symsight._native import length_rewrite_prompt as _length_rewrite_prompt
from symsight._native import system_prompt as _system_prompt
from symsight._native import user_prompt as _user_prompt


def system_prompt(req: Any) -> str:
    return str(_system_prompt(req))


def user_prompt(req: Any, today: str) -> str:
    return str(_user_prompt(req, today))


def length_rewrite_prompt(
    req: Any,
    *,
    title: str,
    body: str,
    current_count: int,
) -> str:
    return str(_length_rewrite_prompt(req, title, body, current_count))
