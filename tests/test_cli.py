# Copyright (c) 2026, Nathaniel T. Berry
# Licensed under the Apache License, Version 2.0.
"""CLI entrypoint behavior."""

from __future__ import annotations

from pathlib import Path

import pytest

from symsight.cli import main


def test_no_subcommand_prints_help(capsys: pytest.CaptureFixture[str]) -> None:
    assert main([]) == 0
    out = capsys.readouterr().out
    assert "generate" in out
    assert "tui" in out


def test_brands_lists_example_writer() -> None:
    assert main(["brands"]) == 0


def test_generate_help_mentions_tone(capsys: pytest.CaptureFixture[str]) -> None:
    assert main(["generate", "--help"]) == 0
    out = capsys.readouterr().out
    assert "--tone" in out
    assert "--voice" in out


def test_unknown_voice_alias_is_rejected(
    capsys: pytest.CaptureFixture[str], tmp_path: Path
) -> None:
    fixture = Path(__file__).parent / "fixtures" / "fixture-co.yaml"
    code = main(
        [
            "--config-root",
            str(tmp_path),
            "generate",
            "--brand-file",
            str(fixture),
            "--voice",
            "briefing",
            "--topic",
            "widgets",
        ]
    )
    assert code == 1
    err = capsys.readouterr().err
    assert "Unknown tone 'briefing'" in err
