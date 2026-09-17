# Copyright (c) 2026, Nathaniel Berry
# Licensed under the Apache License, Version 2.0.
"""CLI entrypoint behavior."""

from __future__ import annotations

import pytest

from symsight.cli import main


def test_no_subcommand_prints_help(capsys: pytest.CaptureFixture[str]) -> None:
    assert main([]) == 0
    out = capsys.readouterr().out
    assert "generate" in out
    assert "tui" in out


def test_brands_lists_example_writer() -> None:
    assert main(["brands"]) == 0
