#!/usr/bin/env python3
# Copyright (c) 2026, Nathaniel Berry
# Licensed under the Apache License, Version 2.0.
"""Export textutil golden vectors from the *current* Python implementation.

The JSON under tests/golden/ is the compatibility contract for the Rust port.
Regenerate only when you intend to change that contract — never after
src/symsight/textutil.py has become a shim over the Rust core.

    uv run python scripts/export_goldens.py
"""

from __future__ import annotations

import json
from pathlib import Path

from symsight.textutil import (
    char_count,
    clean_body,
    extract_social_text,
    extract_title_body,
    is_plausible_title,
    slugify,
    word_count,
)

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "tests" / "golden"

LONG_SLUG = "The Quick Brown Fox Jumps Over The Lazy Dog And Then Some Extra Words Here"


def _dump(name: str, payload: object) -> None:
    path = OUT / name
    path.write_text(
        json.dumps(payload, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    print(f"wrote {path.relative_to(ROOT)}")


def _word_count() -> dict[str, object]:
    inputs = [
        ("empty", ""),
        ("hello", "Hello world"),
        ("hyphen", "one-two three"),
        ("apostrophe", "don't stop"),
        ("underscore", "foo_bar"),
        ("accents", "café au lait"),
        ("punctuation", "..."),
        ("padded", "  spaced  out  "),
        ("digits", "123 456"),
        ("comma", "Hello, world!"),
        ("well_known", "it's well-known"),
        ("newline", "one\ntwo"),
    ]
    return {
        "cases": [
            {"name": name, "input": text, "expected": word_count(text)} for name, text in inputs
        ]
    }


def _char_count() -> dict[str, object]:
    inputs = [
        ("empty", ""),
        ("abc", "abc"),
        ("accents", "café"),
        ("newline", "a\nb"),
    ]
    return {
        "cases": [
            {"name": name, "input": text, "expected": char_count(text)} for name, text in inputs
        ]
    }


def _slugify() -> dict[str, object]:
    cases: list[dict[str, object]] = []
    specs: list[tuple[str, str, int | None]] = [
        ("hello", "Hello World!", None),
        ("empty", "", None),
        ("whitespace", "  ", None),
        ("punctuation", "Hello, World!!!", None),
        ("unicode", "Café Time", None),
        ("hyphens", "---Hello---", None),
        ("already", "already-slug", None),
        ("default_max", LONG_SLUG, None),
        ("max_40", LONG_SLUG, 40),
        ("max_60", LONG_SLUG, 60),
        ("type_slug_40", "short-duration-bond-funds-and-more-extra", 40),
    ]
    for name, text, max_len in specs:
        if max_len is None:
            expected = slugify(text)
            cases.append({"name": name, "input": text, "expected": expected})
        else:
            expected = slugify(text, max_len=max_len)
            cases.append(
                {"name": name, "input": text, "max_len": max_len, "expected": expected}
            )
    return {"cases": cases}


def _clean_body() -> dict[str, object]:
    inputs = [
        ("plain", "Just a paragraph."),
        (
            "citations",
            "Fact one [[1]](https://example.com) and more [2](https://x.test).",
        ),
        ("bare_footnote", "See [[3]] for details."),
        ("leading_rule", "---\n\nBody after rule."),
        ("leading_dashes", "---- leftover"),
        ("trailing_spaces", "line one   \nline two"),
        ("double_space", "too  many   spaces."),
        ("space_before_period", "end ."),
    ]
    return {
        "cases": [
            {"name": name, "input": text, "expected": clean_body(text)} for name, text in inputs
        ]
    }


def _plausible() -> dict[str, object]:
    inputs = [
        ("good", "Markets This Week"),
        ("process", "I will revise the essay now carefully"),
        ("empty", ""),
        ("too_long", "A" * 121),
        ("http", "See http://example.com today"),
        ("https", "See https://example.com today"),
        ("footnote", "Title with [[1]] marker"),
        ("two_periods_long", "A. " * 30 + "end"),
        ("okay_comma", "Okay, here is a title that is not actually a title"),
        ("web_search", "web_search found three sources"),
        ("as_an_ai", "As an AI I would write this title"),
        ("short_ok", "Rates"),
    ]
    return {
        "cases": [
            {"name": name, "input": text, "expected": is_plausible_title(text)}
            for name, text in inputs
        ]
    }


def _extract_pair(
    name: str, text: str
) -> dict[str, object]:
    try:
        title, body = extract_title_body(text)
        return {"name": name, "input": text, "expected": {"title": title, "body": body}, "error": None}
    except ValueError as exc:
        return {"name": name, "input": text, "expected": None, "error": str(exc)}


def _extract_title_body() -> dict[str, object]:
    cases = [
        _extract_pair(
            "clean",
            "TITLE: Markets Hold Steady\n---\nParagraph one.\n\nParagraph two.",
        ),
        _extract_pair(
            "preamble",
            "I will write a careful essay.\n"
            "TITLE: Asset Allocation Basics\n"
            "---\n"
            "Diversification matters for most investors over time.",
        ),
        _extract_pair(
            "fenced",
            "```markdown\nTITLE: Fenced Title\n---\nFenced body paragraph.\n```",
        ),
        _extract_pair(
            "multiple_titles",
            "TITLE: First Draft Title\n---\nscratch\nTITLE: Final Chosen Title\n---\n"
            "The real body that should be kept.",
        ),
        _extract_pair(
            "implausible_then_good",
            "TITLE: I will revise the essay now carefully\n---\nnope\n"
            "TITLE: A Clear Market Note\n---\nUseful body text here.",
        ),
        _extract_pair(
            "only_implausible",
            "TITLE: I will revise the essay now carefully\n---\nBody still kept.",
        ),
        _extract_pair("heading_fallback", "# Heading Line\n\nBody under heading."),
        _extract_pair(
            "implausible_first_line",
            "I will write something now.\n\nActual body paragraph stays.",
        ),
        _extract_pair("empty", ""),
        _extract_pair("title_empty_body", "TITLE: Only A Title\n---\n"),
        _extract_pair(
            "quoted_title",
            'TITLE: "Quoted Title"\n---\nBody after quotes.',
        ),
        _extract_pair(
            "nested_title_token",
            "TITLE: TITLE: Nested Token Title\n---\nBody after nested token.",
        ),
        _extract_pair(
            "inline_title",
            "Preamble TITLE: Mid Line Title\n---\nBody after inline title.",
        ),
    ]
    return {"cases": cases}


def _extract_social() -> dict[str, object]:
    cases: list[dict[str, object]] = []
    specs = [
        ("here_is", "Here is a post:\n\nKeep it short and useful."),
        ("heres", "Here's a post:\n\nA compact tip for savers."),
        (
            "title_paragraph",
            "TITLE: ignore me\n\nThe actual social post text.",
        ),
        ("fenced", "```\nPost inside fences about rates.\n```"),
        ("empty", ""),
        ("single", "One short tip about rebalancing."),
        (
            "multi",
            "First paragraph is the post.\n\nSecond paragraph is discarded.",
        ),
    ]
    for name, text in specs:
        try:
            cases.append(
                {"name": name, "input": text, "expected": extract_social_text(text), "error": None}
            )
        except ValueError as exc:
            cases.append({"name": name, "input": text, "expected": None, "error": str(exc)})
    return {"cases": cases}


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    _dump("word_count.json", _word_count())
    _dump("char_count.json", _char_count())
    _dump("slugify.json", _slugify())
    _dump("clean_body.json", _clean_body())
    _dump("is_plausible_title.json", _plausible())
    _dump("extract_title_body.json", _extract_title_body())
    _dump("extract_social_text.json", _extract_social())


if __name__ == "__main__":
    main()
