# Copyright (c) 2026, Nathaniel T. Berry
# Licensed under the Apache License, Version 2.0.
"""Text helpers: counting, slugify, title/body extraction from model output."""

from __future__ import annotations

from symsight._native import (  # type: ignore[import-not-found]
    char_count,
    clean_body,
    extract_social_text,
    extract_title_body,
    is_plausible_title,
    slugify,
    word_count,
)

__all__ = [
    "char_count",
    "clean_body",
    "extract_social_text",
    "extract_title_body",
    "is_plausible_title",
    "slugify",
    "word_count",
]
