# Copyright (c) 2026, Nathaniel Berry
# Licensed under the Apache License, Version 2.0.
"""Generation orchestration: prompt → LLM → validate → write draft."""

from __future__ import annotations

from pathlib import Path
from typing import Any

from symsight._native import GenerateError
from symsight._native import generate_and_write as _generate_and_write
from symsight._native import generate_content as _generate_content
from symsight.models import DraftMeta

__all__ = ["GenerateError", "generate_and_write", "generate_content"]


def generate_content(
    req: Any,
    cfg: Any,
    *,
    client: object | None = None,
) -> tuple[str, str, DraftMeta]:
    title, body, meta = _generate_content(req, cfg, client)
    return str(title), str(body), DraftMeta.model_validate(meta)


def generate_and_write(
    req: Any,
    cfg: Any,
    *,
    drafts_dir: Path | None = None,
    client: object | None = None,
) -> Path:
    return Path(
        _generate_and_write(
            req,
            cfg,
            str(drafts_dir) if drafts_dir is not None else None,
            client,
        )
    )
