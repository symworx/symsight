# Copyright (c) 2026, Nathaniel T. Berry
# Licensed under the Apache License, Version 2.0.
"""Project-file menu. Arrow keys are decoded without a live terminal."""

from __future__ import annotations

from io import StringIO
from pathlib import Path

from symsight.setup import decode_key, render_menu, run_lines, step


def test_decode_key_matches_the_menu_bytes() -> None:
    assert decode_key(b"\r") == "accept"
    assert decode_key(b"\n") == "accept"
    assert decode_key(b"q") == "quit"
    assert decode_key(b"Q") == "quit"
    assert decode_key(b"\x03") == "interrupt"
    assert decode_key(b"\x1b[A") == "up"
    assert decode_key(b"\x1bOA") == "up"
    assert decode_key(b"\x1b[B") == "down"
    assert decode_key(b"\x1bOB") == "down"
    assert decode_key(b"\x1b") == "quit"
    assert decode_key(b"x") == "ignore"


def test_step_wraps_and_quits() -> None:
    assert step(0, 4, "up") == ("move", 3)
    assert step(3, 4, "down") == ("move", 0)
    assert step(1, 4, "accept") == ("choose", 1)
    assert step(1, 4, "quit") == ("quit", 1)
    assert step(1, 4, "interrupt") == ("quit", 1)
    assert step(2, 4, "ignore") == ("move", 2)


def test_render_menu_marks_the_selection() -> None:
    assert render_menu(("new", "edit"), 1) == "\r\x1b[2K  new\n\r\x1b[2K> edit\n"


def test_line_menu_prints_shortcuts_when_not_interactive(tmp_path: Path) -> None:
    out = StringIO()
    run_lines(tmp_path, False, StringIO("new\n"), out, None)
    text = out.getvalue()
    assert "new:    symsight create" in text
    assert "edit:   symsight edit" in text
    assert "delete: symsight delete" in text
    assert "> " not in text
    assert not (tmp_path / ".symsight.toml").exists()


def test_line_menu_new_then_quit(tmp_path: Path) -> None:
    out = StringIO()
    run_lines(tmp_path, True, StringIO("new\nquit\n"), out, None)
    text = out.getvalue()
    assert "Shortcuts: symsight create, symsight edit, symsight delete" in text
    assert "created:" in text
    body = (tmp_path / ".symsight.toml").read_text(encoding="utf-8")
    assert 'base_url = "https://api.x.ai/v1"' in body
    assert not (tmp_path / ".env").exists()


def test_line_menu_unknown_then_delete_cancel(tmp_path: Path) -> None:
    (tmp_path / ".symsight.toml").write_text("kept = true\n", encoding="utf-8")
    out = StringIO()
    run_lines(tmp_path, True, StringIO("create\ndelete\nn\nq\n"), out, None)
    text = out.getvalue()
    assert "choose new, edit, delete, or quit (got 'create')" in text
    assert ">>> delete cancelled" in text
    assert (tmp_path / ".symsight.toml").read_text(encoding="utf-8") == "kept = true\n"
