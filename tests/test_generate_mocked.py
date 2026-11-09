# Copyright (c) 2026, Nathaniel T. Berry
# Licensed under the Apache License, Version 2.0.
"""Generation orchestration with mocked LLM."""

from __future__ import annotations

import json
from pathlib import Path
from types import SimpleNamespace

from symsight.brand import load_brand_file
from symsight.config import AppConfig
from symsight.generate import generate_and_write
from symsight.models import ContentFormat, GenerateRequest

FIXTURE = Path(__file__).parent / "fixtures" / "fixture-co.yaml"


class FakeClient:
    def __init__(self, text: str) -> None:
        self.text = text
        self.calls = 0
        self.kwargs: dict[str, object] = {}

    @property
    def responses(self) -> FakeClient:
        return self

    def create(self, **kwargs: object) -> SimpleNamespace:
        self.calls += 1
        self.kwargs = kwargs
        return SimpleNamespace(output_text=self.text, output=[])


def test_generate_article_mocked(tmp_path: Path) -> None:
    brand = load_brand_file(FIXTURE)
    body_words = " ".join(["word"] * 60)
    raw = f"TITLE: Test Title\n---\n{body_words}"
    client = FakeClient(raw)
    cfg = AppConfig(
        xai_api_key="test-key",
        project_root=tmp_path,
        drafts_dir=tmp_path / "drafts",
        final_dir=tmp_path / "final",
        brands_dir=tmp_path,
    ).resolve_paths()
    req = GenerateRequest(
        brand=brand,
        type_id="general",
        format=ContentFormat.ARTICLE,
        topic="testing",
        min_words=50,
        max_words=100,
        use_search=False,
    )
    path = generate_and_write(req, cfg, client=client)
    assert path.is_file()
    text = path.read_text(encoding="utf-8")
    assert "Test Title" in text
    assert "word" in text


def test_generate_social_mocked(tmp_path: Path) -> None:
    brand = load_brand_file(FIXTURE)
    raw = "A short tip about testing carefully."
    client = FakeClient(raw)
    cfg = AppConfig(
        xai_api_key="test-key",
        project_root=tmp_path,
        drafts_dir=tmp_path / "drafts",
        final_dir=tmp_path / "final",
        brands_dir=tmp_path,
    ).resolve_paths()
    req = GenerateRequest(
        brand=brand,
        type_id="social-tip",
        format=ContentFormat.SOCIAL,
        topic="testing",
        max_chars=200,
        use_search=False,
    )
    path = generate_and_write(req, cfg, client=client)
    assert path.is_file()
    assert "short tip" in path.read_text(encoding="utf-8")


def test_generate_attaches_context_file(tmp_path: Path) -> None:
    notes = tmp_path / "notes.md"
    notes.write_text("A concrete note about widgets.\n", encoding="utf-8")
    brand = load_brand_file(FIXTURE)
    body_words = " ".join(["word"] * 60)
    raw = f"TITLE: Test Title\n---\n{body_words}"
    client = FakeClient(raw)
    cfg = AppConfig(
        xai_api_key="test-key",
        project_root=tmp_path,
        drafts_dir=tmp_path / "drafts",
        final_dir=tmp_path / "final",
        brands_dir=tmp_path,
    ).resolve_paths()
    req = GenerateRequest(
        brand=brand,
        type_id="general",
        format=ContentFormat.ARTICLE,
        topic="testing",
        min_words=50,
        max_words=100,
        use_search=False,
        context_files=[str(notes)],
    )
    path = generate_and_write(req, cfg, client=client)
    meta = json.loads(path.with_suffix(".meta.json").read_text(encoding="utf-8"))
    assert meta["context_files"] == ["notes.md"]
    user = client.kwargs["input"][1]["content"]
    assert isinstance(user, str)
    assert "A concrete note about widgets." in user
