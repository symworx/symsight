# Copyright (c) 2026, Nathaniel T. Berry
# Licensed under the Apache License, Version 2.0.
"""Load and resolve brand YAML configs."""

from __future__ import annotations

from pathlib import Path

from symsight._native import BrandError
from symsight._native import list_brand_files as _list_brand_files
from symsight._native import list_brands as _list_brands_json
from symsight._native import load_brand_file as _load_brand_json
from symsight._native import resolve_brand as _resolve_brand_json
from symsight.models import Brand

__all__ = [
    "Brand",
    "BrandError",
    "list_brand_files",
    "list_brands",
    "load_brand_file",
    "resolve_brand",
]


def load_brand_file(path: Path) -> Brand:
    return Brand.model_validate_json(_load_brand_json(str(path)))


def list_brand_files(brands_dir: Path) -> list[Path]:
    return [Path(p) for p in _list_brand_files(str(brands_dir))]


def list_brands(brands_dir: Path) -> list[Brand]:
    return [Brand.model_validate_json(raw) for raw in _list_brands_json(str(brands_dir))]


def resolve_brand(
    *,
    brands_dir: Path,
    brand_id: str | None = None,
    brand_path: Path | None = None,
) -> Brand:
    raw = _resolve_brand_json(
        str(brands_dir),
        brand_id,
        str(brand_path) if brand_path is not None else None,
    )
    return Brand.model_validate_json(raw)
