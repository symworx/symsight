# Copyright (c) 2026, Nathaniel Berry
# Licensed under the Apache License, Version 2.0.
"""Redact API keys and reject path-traversal names."""

from __future__ import annotations

from pathlib import Path

import pytest

from symsight.brand import BrandError, resolve_brand
from symsight.config import AppConfig
from symsight.draft_io import unique_draft_path


def test_appconfig_repr_hides_api_key() -> None:
    cfg = AppConfig(xai_api_key="xai-TESTSECRETNOTREAL")
    dumped = repr(cfg) + str(cfg)
    assert "TESTSECRETNOTREAL" not in dumped


def test_unique_draft_path_rejects_traversal(tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="unsafe draft stem"):
        unique_draft_path(tmp_path, "../etc/passwd")


def test_resolve_brand_rejects_traversal(tmp_path: Path) -> None:
    with pytest.raises(BrandError, match="Unsafe brand id"):
        resolve_brand(brands_dir=tmp_path, brand_id="../x")
