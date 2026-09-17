# Copyright (c) 2026, Nathaniel Berry
# Licensed under the Apache License, Version 2.0.
"""Domain implementation is the native Rust extension."""

from __future__ import annotations

import symsight._native as native

from symsight.brand import list_brands
from symsight.config import find_project_root


def test_native_extension_loads() -> None:
    assert hasattr(native, "generate_and_write")
    assert hasattr(native, "load_brand_file")


def test_brands_round_trip_via_shim() -> None:
    brands = list_brands(find_project_root() / "config" / "brands")
    ids = {b.id for b in brands}
    assert "example-writer" in ids
