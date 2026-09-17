# Copyright (c) 2026, Nathaniel T. Berry
# Licensed under the Apache License, Version 2.0.
"""Draft front matter and finalize tests."""

from __future__ import annotations

from datetime import UTC, datetime
from pathlib import Path

from symsight.brand import load_brand_file
from symsight.draft_io import parse_front_matter, read_draft, write_new_draft
from symsight.finalize import finalize_draft
from symsight.models import ContentFormat, DraftMeta

FIXTURE = Path(__file__).parent / "fixtures" / "fixture-co.yaml"


def test_front_matter_roundtrip(tmp_path: Path) -> None:
    meta = DraftMeta(
        model="test-model",
        type="general",
        format="article",
        brand_id="fixture-co",
        word_count=12,
        generated_at=datetime.now(UTC).isoformat(),
    )
    path = write_new_draft(
        drafts_dir=tmp_path / "drafts",
        title="Hello World",
        body="This is the body of the draft with enough words here.",
        brand_id="fixture-co",
        brand_display="Fixture Co",
        type_id="general",
        fmt=ContentFormat.ARTICLE,
        topic="hello",
        disclaimer="Test disclaimer.",
        meta=meta,
    )
    assert path.is_file()
    draft = read_draft(path)
    assert draft.title == "Hello World"
    assert draft.front_matter["status"] == "draft"
    assert draft.front_matter["brand"] == "fixture-co"
    assert "body of the draft" in draft.body
    assert path.with_suffix(".meta.json").is_file()


def test_parse_front_matter() -> None:
    raw = '---\ntitle: "X"\nword_count: 3\nstatus: "draft"\n---\nHello'
    fm, body = parse_front_matter(raw)
    assert fm["title"] == "X"
    assert fm["word_count"] == 3
    assert body.strip() == "Hello"


def test_finalize_move(tmp_path: Path) -> None:
    brand = load_brand_file(FIXTURE)
    drafts = tmp_path / "drafts"
    final = tmp_path / "final"
    meta = DraftMeta(
        model="test-model",
        type="general",
        format="article",
        brand_id="fixture-co",
        word_count=5,
        generated_at=datetime.now(UTC).isoformat(),
    )
    path = write_new_draft(
        drafts_dir=drafts,
        title="Finalize Me",
        body="Short body for finalize.",
        brand_id="fixture-co",
        brand_display="Fixture Co",
        type_id="general",
        fmt=ContentFormat.ARTICLE,
        topic=None,
        disclaimer=None,
        meta=meta,
    )
    dest = finalize_draft(path, final_dir=final, brand=brand, copy=False)
    assert dest.is_file()
    assert dest.parent == final.resolve()
    assert not path.exists()
    moved = read_draft(dest)
    assert moved.front_matter.get("status") == "final"


def test_finalize_copy(tmp_path: Path) -> None:
    drafts = tmp_path / "drafts"
    final = tmp_path / "final"
    meta = DraftMeta(
        model="test-model",
        type="general",
        format="article",
        brand_id="fixture-co",
        word_count=5,
        generated_at=datetime.now(UTC).isoformat(),
    )
    path = write_new_draft(
        drafts_dir=drafts,
        title="Copy Me",
        body="Short body for copy finalize.",
        brand_id="fixture-co",
        brand_display="Fixture Co",
        type_id="general",
        fmt=ContentFormat.ARTICLE,
        topic=None,
        disclaimer=None,
        meta=meta,
    )
    dest = finalize_draft(path, final_dir=final, brand=None, copy=True)
    assert dest.is_file()
    assert path.exists()
