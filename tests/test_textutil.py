# Copyright (c) 2026, Nathaniel T. Berry
# Licensed under the Apache License, Version 2.0.
"""Tests for title/body extraction and counters."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from symsight.textutil import (
    char_count,
    clean_body,
    extract_social_text,
    extract_title_body,
    is_plausible_title,
    slugify,
    word_count,
)


def test_word_count_basic() -> None:
    assert word_count("Hello world") == 2
    # hyphen splits tokens (same behavior as source oakmon scripts)
    assert word_count("one-two three") == 3
    assert word_count("don't stop") == 2


def test_char_count() -> None:
    assert char_count("abc") == 3


def test_slugify() -> None:
    assert slugify("Hello World!") == "hello-world"
    assert slugify("") == "insight"


def test_extract_title_body_clean() -> None:
    raw = "TITLE: Markets Hold Steady\n---\nParagraph one.\n\nParagraph two."
    title, body = extract_title_body(raw)
    assert title == "Markets Hold Steady"
    assert "Paragraph one" in body
    assert "Paragraph two" in body


def test_extract_title_body_with_preamble() -> None:
    raw = (
        "I will write a careful essay.\n"
        "TITLE: Asset Allocation Basics\n"
        "---\n"
        "Diversification matters for most investors over time."
    )
    title, body = extract_title_body(raw)
    assert title == "Asset Allocation Basics"
    assert "Diversification" in body


def test_clean_body_strips_citations() -> None:
    body = "Fact one [[1]](https://example.com) and more [2](https://x.test)."
    cleaned = clean_body(body)
    assert "[[" not in cleaned
    assert "http" not in cleaned


def test_is_plausible_title() -> None:
    assert is_plausible_title("Markets This Week")
    assert not is_plausible_title("I will revise the essay now carefully")


def test_extract_social_text() -> None:
    raw = "Here is a post:\n\nKeep it short and useful."
    assert "Keep it short" in extract_social_text(raw)


def test_extract_empty_raises() -> None:
    with pytest.raises(ValueError):
        extract_title_body("")


GOLDEN = Path(__file__).parent / "golden"


def _cases(name: str) -> list[dict[str, object]]:
    payload = json.loads((GOLDEN / name).read_text(encoding="utf-8"))
    return list(payload["cases"])


@pytest.mark.parametrize("case", _cases("word_count.json"), ids=lambda c: str(c["name"]))
def test_word_count_golden(case: dict[str, object]) -> None:
    assert word_count(str(case["input"])) == case["expected"]


@pytest.mark.parametrize("case", _cases("char_count.json"), ids=lambda c: str(c["name"]))
def test_char_count_golden(case: dict[str, object]) -> None:
    assert char_count(str(case["input"])) == case["expected"]


@pytest.mark.parametrize("case", _cases("slugify.json"), ids=lambda c: str(c["name"]))
def test_slugify_golden(case: dict[str, object]) -> None:
    max_len = int(case["max_len"]) if "max_len" in case else 60
    assert slugify(str(case["input"]), max_len=max_len) == case["expected"]


@pytest.mark.parametrize("case", _cases("clean_body.json"), ids=lambda c: str(c["name"]))
def test_clean_body_golden(case: dict[str, object]) -> None:
    assert clean_body(str(case["input"])) == case["expected"]


@pytest.mark.parametrize("case", _cases("is_plausible_title.json"), ids=lambda c: str(c["name"]))
def test_is_plausible_title_golden(case: dict[str, object]) -> None:
    assert is_plausible_title(str(case["input"])) is case["expected"]


@pytest.mark.parametrize("case", _cases("extract_title_body.json"), ids=lambda c: str(c["name"]))
def test_extract_title_body_golden(case: dict[str, object]) -> None:
    if case.get("error"):
        with pytest.raises(ValueError, match=str(case["error"])):
            extract_title_body(str(case["input"]))
        return
    title, body = extract_title_body(str(case["input"]))
    expected = case["expected"]
    assert isinstance(expected, dict)
    assert title == expected["title"]
    assert body == expected["body"]


@pytest.mark.parametrize("case", _cases("extract_social_text.json"), ids=lambda c: str(c["name"]))
def test_extract_social_golden(case: dict[str, object]) -> None:
    if case.get("error"):
        with pytest.raises(ValueError, match=str(case["error"])):
            extract_social_text(str(case["input"]))
        return
    assert extract_social_text(str(case["input"])) == case["expected"]
