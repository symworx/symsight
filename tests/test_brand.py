# Copyright (c) 2026, Nathaniel Berry
# Licensed under the Apache License, Version 2.0.
"""Brand load and check tests."""

from __future__ import annotations

from pathlib import Path

import pytest

from symsight.brand import BrandError, load_brand_file, resolve_brand
from symsight.brandcheck import check_text
from symsight.models import ContentFormat, GenerateRequest
from symsight.prompts import system_prompt, user_prompt

FIXTURE = Path(__file__).parent / "fixtures" / "fixture-co.yaml"


def test_load_brand() -> None:
    brand = load_brand_file(FIXTURE)
    assert brand.id == "fixture-co"
    assert "general" in brand.types
    assert "fixturecorp" in brand.forbidden


def test_resolve_brand_by_id(tmp_path: Path) -> None:
    dest = tmp_path / "fixture-co.yaml"
    dest.write_text(FIXTURE.read_text(encoding="utf-8"), encoding="utf-8")
    brand = resolve_brand(brands_dir=tmp_path, brand_id="fixture-co")
    assert brand.full_name == "Fixture Co"


def test_resolve_missing() -> None:
    with pytest.raises(BrandError):
        resolve_brand(brands_dir=Path("/nonexistent"), brand_id="nope")


def test_forbidden_hits() -> None:
    brand = load_brand_file(FIXTURE)
    assert check_text("Welcome to FixtureCorp services", brand)
    assert not check_text("Welcome to Fixture Co", brand)


def test_scan_skips_target_and_dist(tmp_path: Path) -> None:
    from symsight.brandcheck import iter_scan_files

    (tmp_path / "target").mkdir()
    (tmp_path / "dist").mkdir()
    (tmp_path / "ok").mkdir()
    (tmp_path / "target" / "hidden.md").write_text("fixturecorp", encoding="utf-8")
    (tmp_path / "dist" / "hidden.md").write_text("fixturecorp", encoding="utf-8")
    (tmp_path / "ok" / "visible.md").write_text("ok", encoding="utf-8")
    names = {p.name for p in iter_scan_files([tmp_path])}
    assert names == {"visible.md"}


def test_prompts_use_brand_name_not_hardcoded() -> None:
    brand = load_brand_file(FIXTURE)
    req = GenerateRequest(brand=brand, type_id="general", topic="testing")
    sys_p = system_prompt(req)
    usr_p = user_prompt(req, "2026-08-05")
    assert "Fixture Co" in sys_p
    assert "testing" in usr_p
    # Ensure no accidental oakmon hardcoding in library prompts
    assert "oakmon" not in sys_p.lower()
    assert "oakmon" not in usr_p.lower()


def test_social_system_prompt() -> None:
    brand = load_brand_file(FIXTURE)
    req = GenerateRequest(
        brand=brand,
        type_id="social-tip",
        format=ContentFormat.SOCIAL,
        topic="hydration",
        max_chars=180,
    )
    sys_p = system_prompt(req)
    assert "180" in sys_p
    assert "post" in sys_p.lower()
