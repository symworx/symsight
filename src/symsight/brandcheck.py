# Copyright (c) 2026, Nathaniel Berry
# Licensed under the Apache License, Version 2.0.
"""Forbidden-term scanning from active brand config."""

from __future__ import annotations

from pathlib import Path
from typing import Any

from symsight._native import check_path as _check_path
from symsight._native import check_text as _check_text
from symsight._native import find_hits as _find_hits
from symsight._native import iter_scan_files as _iter_scan_files
from symsight._native import scan_paths as _scan_paths


def find_hits(text: str, forbidden: list[str]) -> list[str]:
    return list(_find_hits(text, list(forbidden)))


def check_text(text: str, brand: Any) -> list[str]:
    return list(_check_text(text, brand))


def check_path(path: Path, brand: Any) -> list[str]:
    return list(_check_path(str(path), brand))


def iter_scan_files(roots: list[Path]) -> list[Path]:
    return [Path(p) for p in _iter_scan_files([str(r) for r in roots])]


def scan_paths(roots: list[Path], brand: Any) -> list[tuple[Path, list[str]]]:
    out: list[tuple[Path, list[str]]] = []
    for path, hits in _scan_paths([str(r) for r in roots], brand):
        out.append((Path(path), list(hits)))
    return out
