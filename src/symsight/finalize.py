# Copyright (c) 2026, Nathaniel T. Berry
# Licensed under the Apache License, Version 2.0.
"""Promote drafts to the final directory (move or copy markdown only)."""

from __future__ import annotations

from pathlib import Path
from typing import Any

from symsight._native import FinalizeError
from symsight._native import finalize_draft as _finalize_draft
from symsight._native import list_final as _list_final

__all__ = ["FinalizeError", "finalize_draft", "list_final"]


def finalize_draft(
    draft_path: Path,
    *,
    final_dir: Path,
    brand: Any | None = None,
    copy: bool = False,
) -> Path:
    return Path(_finalize_draft(str(draft_path), str(final_dir), brand, copy))


def list_final(final_dir: Path) -> list[Path]:
    return [Path(p) for p in _list_final(str(final_dir))]
