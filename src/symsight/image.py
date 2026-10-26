# Copyright (c) 2026, Nathaniel T. Berry
# Licensed under the Apache License, Version 2.0.
"""Generate an image from the active brand's `image` block."""

from __future__ import annotations

from pathlib import Path

from symsight._native import ImageError
from symsight._native import render_image as _render_image
from symsight.models import Brand

__all__ = ["ImageError", "render_image"]


def render_image(
    brand: Brand,
    scene: str,
    dest: Path,
    *,
    aspect: str | None = None,
    quality: str | None = None,
    model: str | None = None,
    api_key: str,
    base_url: str,
) -> Path:
    raw = _render_image(
        brand,
        scene,
        str(dest),
        aspect,
        quality,
        model,
        api_key,
        base_url,
    )
    return Path(raw)
