# Copyright (c) 2026, Nathaniel Berry
# Licensed under the Apache License, Version 2.0.
"""TUI import / construct smoke — no live Textual run, no live HTTP."""

from __future__ import annotations

from pathlib import Path

from symsight.config import AppConfig
from symsight.tui.app import GenerateScreen, SettingsScreen, SymsightApp


def _cfg(tmp_path: Path) -> AppConfig:
    return AppConfig(
        xai_api_key="test-key",
        project_root=tmp_path,
        drafts_dir=tmp_path / "drafts",
        final_dir=tmp_path / "final",
        brands_dir=tmp_path,
        active_brand="fixture-co",
    ).resolve_paths()


def test_symsight_app_constructs(tmp_path: Path) -> None:
    cfg = _cfg(tmp_path)
    app = SymsightApp(cfg)
    assert app.cfg.drafts_dir == cfg.drafts_dir
    assert app.current_path is None
    assert app.brand is None


def test_settings_uses_pydantic_model_copy(tmp_path: Path) -> None:
    cfg = _cfg(tmp_path)
    updated = cfg.model_copy(update={"active_brand": "other"})
    assert updated.active_brand == "other"
    assert cfg.active_brand == "fixture-co"
    screen = SettingsScreen(cfg)
    assert screen.cfg.active_brand == "fixture-co"


def test_generate_screen_imports(tmp_path: Path) -> None:
    from symsight.brand import load_brand_file

    fixture = Path(__file__).parent / "fixtures" / "fixture-co.yaml"
    brand = load_brand_file(fixture)
    screen = GenerateScreen(_cfg(tmp_path), brand)
    assert screen.brand.id == "fixture-co"
