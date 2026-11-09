# Copyright (c) 2026, Nathaniel T. Berry
# Licensed under the Apache License, Version 2.0.
"""Menu and shortcuts for the project ``.symsight.toml`` file."""

from __future__ import annotations

import os
import select
import sys
import termios
import tty
from typing import TextIO

from symsight._native import apply_setup as _apply_setup
from symsight._native import create_project_config as _create_project_config
from symsight._native import delete_confirmed as _delete_confirmed
from symsight._native import delete_project_config as _delete_project_config
from symsight._native import edit_project_config as _edit_project_config
from symsight._native import generate_base_url as _generate_base_url
from symsight._native import list_model_ids as _list_model_ids
from symsight._native import preferred_editor as _preferred_editor
from symsight._native import project_config_path as _project_config_path
from symsight._native import setup_menu_header as _setup_menu_header
from symsight._native import setup_shortcut_text as _setup_shortcut_text
from symsight.config import AppConfig

ACTIONS: tuple[str, ...] = ("new", "edit", "delete", "quit")


def apply_setup(
    cfg: AppConfig,
    *,
    local: bool,
    base_url: str,
    model: str,
    api_key: str | None = None,
) -> dict[str, object]:
    """Save model settings. A remote key is written only to ``.env``."""
    return _apply_setup(cfg, local, base_url, model, api_key)


def list_model_ids(base_url: str) -> list[str]:
    """Ask a local server which model ids it is serving."""
    return list(_list_model_ids(base_url))


def project_config_path(root: os.PathLike[str] | str) -> str:
    """Path of ``.symsight.toml`` under ``root``."""
    return _project_config_path(os.fspath(root))


def create_project_config(root: os.PathLike[str] | str) -> str:
    """Write the starter file, or a message when one is already there."""
    return _create_project_config(os.fspath(root))


def edit_project_config(root: os.PathLike[str] | str, editor: str | None) -> str:
    """Open the project file in ``editor`` and return the edited line."""
    return _edit_project_config(os.fspath(root), editor)


def delete_project_config(root: os.PathLike[str] | str) -> str:
    """Remove ``.symsight.toml``. ``.env`` is left in place."""
    return _delete_project_config(os.fspath(root))


def delete_confirmed(answer: str) -> bool:
    """True for ``y``, ``Y``, ``yes``, or ``YES``."""
    return bool(_delete_confirmed(answer))


def preferred_editor() -> str | None:
    """``$VISUAL``, then ``$EDITOR``. Empty values are skipped."""
    return _preferred_editor(os.environ.get("VISUAL"), os.environ.get("EDITOR"))


def setup_shortcut_text(root: os.PathLike[str] | str) -> str:
    """Non-interactive ``symsight setup`` text."""
    return _setup_shortcut_text(os.fspath(root))


def setup_menu_header(root: os.PathLike[str] | str, *, arrows: bool) -> str:
    """Interactive menu header."""
    return _setup_menu_header(os.fspath(root), arrows)


def generate_base_url(configured: str, local: bool) -> str:
    """Base URL for one generate run. ``--local`` keeps a saved localhost URL."""
    return _generate_base_url(configured, local)


def stdin_is_tty() -> bool:
    return sys.stdin.isatty()


def stdout_is_tty() -> bool:
    return sys.stdout.isatty()


def decode_key(data: bytes) -> str:
    """Map one key sequence to up, down, accept, quit, interrupt, or ignore."""
    if data in (b"\r", b"\n"):
        return "accept"
    if data in (b"q", b"Q"):
        return "quit"
    if data == b"\x03":
        return "interrupt"
    if data in (b"\x1b[A", b"\x1bOA"):
        return "up"
    if data in (b"\x1b[B", b"\x1bOB"):
        return "down"
    if data == b"\x1b":
        return "quit"
    return "ignore"


def step(index: int, length: int, key: str) -> tuple[str, int]:
    """Next menu action. ``move`` keeps or changes the index. ``quit`` leaves."""
    if length == 0:
        return ("quit", 0)
    if key == "up":
        return ("move", length - 1 if index == 0 else index - 1)
    if key == "down":
        return ("move", 0 if index + 1 == length else index + 1)
    if key == "accept":
        return ("choose", index)
    if key in {"quit", "interrupt"}:
        return ("quit", index)
    return ("move", index)


def render_menu(items: tuple[str, ...] | list[str], index: int) -> str:
    """One screen of the arrow menu. The current row starts with ``>``."""
    lines: list[str] = []
    for i, item in enumerate(items):
        mark = ">" if i == index else " "
        lines.append(f"\r\x1b[2K{mark} {item}\n")
    return "".join(lines)


def run_setup(root: os.PathLike[str] | str) -> None:
    """Arrow menu on a terminal. Otherwise a line prompt, or the shortcut list."""
    editor = preferred_editor()
    interactive = stdin_is_tty()
    if interactive and stdout_is_tty() and _raw_mode_works():
        _run_arrows(root, editor)
        return
    run_lines(root, interactive, sys.stdin, sys.stdout, editor)


def run_lines(
    root: os.PathLike[str] | str,
    interactive: bool,
    reader: TextIO,
    out: TextIO,
    editor: str | None,
) -> None:
    """Line prompt. With ``interactive`` false, print the shortcuts and return."""
    if not interactive:
        out.write(setup_shortcut_text(root))
        return
    out.write(setup_menu_header(root, arrows=False))
    try:
        while True:
            out.writelines(f"{action}\n" for action in ACTIONS)
            choice = _prompt(reader, out, "> ").lower()
            if choice in {"quit", "q", ""}:
                return
            if choice not in {"new", "edit", "delete"}:
                out.write(f"choose new, edit, delete, or quit (got '{choice}')\n")
                continue
            _dispatch(root, choice, editor, reader, out)
    except KeyboardInterrupt:
        return


def _run_arrows(root: os.PathLike[str] | str, editor: str | None) -> None:
    sys.stdout.write(setup_menu_header(root, arrows=True))
    sys.stdout.flush()
    try:
        while True:
            choice = select_index(sys.stdout, ACTIONS)
            if choice is None or choice == "quit":
                return
            _dispatch(root, choice, editor, sys.stdin, sys.stdout)
    except KeyboardInterrupt:
        return


def select_index(out: TextIO, items: tuple[str, ...] | list[str]) -> str | None:
    """Arrow-key choice. ``None`` means quit. Restores the terminal before returning."""
    if not items:
        return None
    fd = sys.stdin.fileno()
    old = termios.tcgetattr(fd)
    try:
        tty.setraw(fd)
        index = 0
        _draw(out, items, index, first=True)
        while True:
            kind, index = step(index, len(items), _read_menu_key())
            if kind == "move":
                _draw(out, items, index, first=False)
            elif kind == "choose":
                return items[index]
            else:
                return None
    finally:
        termios.tcsetattr(fd, termios.TCSADRAIN, old)


def _dispatch(
    root: os.PathLike[str] | str,
    choice: str,
    editor: str | None,
    reader: TextIO,
    out: TextIO,
) -> None:
    try:
        if choice == "new":
            message = create_project_config(root)
        elif choice == "edit":
            message = edit_project_config(root, editor)
        elif choice == "delete":
            path = project_config_path(root)
            if not _confirm_delete(path, reader, out):
                out.write(">>> delete cancelled\n")
                return
            message = delete_project_config(root)
        else:
            out.write(f"choose new, edit, delete, or quit (got '{choice}')\n")
            return
    except (RuntimeError, OSError) as exc:
        out.write(f"error: {exc}\n")
        return
    if message:
        out.write(f"{message}\n")


def _confirm_delete(path: str, reader: TextIO, out: TextIO) -> bool:
    out.write(f"Delete {path}? [y/N] ")
    out.flush()
    return delete_confirmed(reader.readline())


def _prompt(reader: TextIO, out: TextIO, label: str) -> str:
    out.write(label)
    out.flush()
    line = reader.readline()
    if not line:
        return ""
    return line.strip()


def _draw(out: TextIO, items: tuple[str, ...] | list[str], index: int, *, first: bool) -> None:
    if not first:
        out.write(f"\x1b[{len(items)}A")
    out.write(render_menu(items, index))
    out.flush()


def _read_menu_key() -> str:
    first = _read_byte()
    if first != 0x1B:
        return decode_key(bytes([first]))
    seq = bytearray([0x1B])
    if _wait_stdin(0.08):
        second = _read_byte()
        seq.append(second)
        if second in (ord("["), ord("O")) and _wait_stdin(0.08):
            seq.append(_read_byte())
    return decode_key(bytes(seq))


def _read_byte() -> int:
    chunk = sys.stdin.buffer.read(1)
    if not chunk:
        raise EOFError("stdin closed")
    return chunk[0]


def _wait_stdin(timeout: float) -> bool:
    ready, _, _ = select.select([sys.stdin], [], [], timeout)
    return bool(ready)


def _raw_mode_works() -> bool:
    try:
        termios.tcgetattr(sys.stdin.fileno())
    except (OSError, ValueError, termios.error):
        return False
    return True
