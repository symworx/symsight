# Copyright (c) 2026, Nathaniel T. Berry
# Licensed under the Apache License, Version 2.0.
"""Application settings: paths, model, active brand."""

from __future__ import annotations

import os
from pathlib import Path
from typing import Any

from pydantic import AliasChoices, Field
from pydantic_settings import BaseSettings, SettingsConfigDict

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover — py<3.11
    import tomli as tomllib  # type: ignore


DEFAULT_MODEL = "grok-4.5"
DEFAULT_BASE_URL = "https://api.x.ai/v1"


def find_project_root(start: Path | None = None) -> Path:
    """Walk up looking for pyproject.toml or .symsight.toml."""
    cur = (start or Path.cwd()).resolve()
    for path in [cur, *cur.parents]:
        if (path / "pyproject.toml").exists() or (path / ".symsight.toml").exists():
            return path
        if (path / "config" / "brands").is_dir():
            return path
    return cur


def _load_toml(path: Path) -> dict[str, Any]:
    if not path.is_file():
        return {}
    with path.open("rb") as f:
        data = tomllib.load(f)
    return dict(data) if data else {}


def load_config_file(root: Path | None = None) -> dict[str, Any]:
    """Merge project .symsight.toml over user config if present."""
    root = root or find_project_root()
    user = Path.home() / ".config" / "symsight" / "config.toml"
    merged: dict[str, Any] = {}
    merged.update(_load_toml(user))
    merged.update(_load_toml(root / ".symsight.toml"))
    return merged


class AppConfig(BaseSettings):
    """Runtime configuration.

    Most fields accept ``SYMSIGHT_*`` env vars. API key accepts ``XAI_API_KEY``
    or ``SYMSIGHT_XAI_API_KEY``.
    """

    model_config = SettingsConfigDict(
        env_file=".env",
        env_file_encoding="utf-8",
        extra="ignore",
        populate_by_name=True,
    )

    xai_api_key: str = Field(
        default="",
        repr=False,
        validation_alias=AliasChoices("XAI_API_KEY", "SYMSIGHT_XAI_API_KEY", "xai_api_key"),
    )
    model: str = Field(default=DEFAULT_MODEL, validation_alias=AliasChoices("SYMSIGHT_MODEL", "model"))
    base_url: str = Field(
        default=DEFAULT_BASE_URL,
        validation_alias=AliasChoices("SYMSIGHT_BASE_URL", "base_url"),
    )
    active_brand: str = Field(
        default="example-writer",
        validation_alias=AliasChoices("SYMSIGHT_ACTIVE_BRAND", "active_brand"),
    )
    brands_dir: Path = Field(
        default=Path("./config/brands"),
        validation_alias=AliasChoices("SYMSIGHT_BRANDS_DIR", "brands_dir"),
    )
    drafts_dir: Path = Field(
        default=Path("./content/drafts"),
        validation_alias=AliasChoices("SYMSIGHT_DRAFTS_DIR", "drafts_dir"),
    )
    final_dir: Path = Field(
        default=Path("./content/final"),
        validation_alias=AliasChoices("SYMSIGHT_FINAL_DIR", "final_dir"),
    )
    project_root: Path = Field(default_factory=find_project_root)

    def resolve_paths(self) -> AppConfig:
        """Resolve relative paths against project_root."""
        root = self.project_root.resolve()

        def rel(p: Path) -> Path:
            if p.is_absolute():
                return p
            return (root / p).resolve()

        return self.model_copy(
            update={
                "project_root": root,
                "brands_dir": rel(self.brands_dir),
                "drafts_dir": rel(self.drafts_dir),
                "final_dir": rel(self.final_dir),
            }
        )

    def require_api_key(self) -> str:
        key = (self.xai_api_key or os.environ.get("XAI_API_KEY", "")).strip()
        if not key:
            # try dotenv from project root
            try:
                from dotenv import load_dotenv

                load_dotenv(self.project_root / ".env")
            except ImportError:
                pass
            key = (os.environ.get("XAI_API_KEY", "") or self.xai_api_key).strip()
        if not key:
            raise RuntimeError(
                "XAI_API_KEY is not set. Export it or add it to a git-ignored .env file.\n"
                "  export XAI_API_KEY=...\n"
                "  # see .env.example"
            )
        return key


def get_config(
    *,
    root: Path | None = None,
    overrides: dict[str, Any] | None = None,
) -> AppConfig:
    """Build AppConfig from files + env + optional overrides."""
    root = (root or find_project_root()).resolve()
    file_data = load_config_file(root)

    # Map toml keys → settings fields
    data: dict[str, Any] = {"project_root": root}
    for key in (
        "active_brand",
        "brands_dir",
        "drafts_dir",
        "final_dir",
        "model",
        "base_url",
    ):
        if key in file_data and file_data[key] is not None:
            data[key] = file_data[key]

    if overrides:
        data.update({k: v for k, v in overrides.items() if v is not None})

    cfg = AppConfig(**data)
    # XAI_API_KEY from env (pydantic-settings) — also pull bare env
    if not cfg.xai_api_key:
        cfg = cfg.model_copy(update={"xai_api_key": os.environ.get("XAI_API_KEY", "")})
    return cfg.resolve_paths()


def save_project_config(cfg: AppConfig, path: Path | None = None) -> Path:
    """Write path/brand settings to project .symsight.toml (partial update)."""
    root = cfg.project_root
    out = path or (root / ".symsight.toml")
    existing = _load_toml(out)

    def as_str(p: Path) -> str:
        try:
            return str(p.relative_to(root))
        except ValueError:
            return str(p)

    existing.update(
        {
            "active_brand": cfg.active_brand,
            "brands_dir": as_str(cfg.brands_dir),
            "drafts_dir": as_str(cfg.drafts_dir),
            "final_dir": as_str(cfg.final_dir),
            "model": cfg.model,
            "base_url": cfg.base_url,
        }
    )

    lines = ["# symsight project config", ""]
    for key, val in existing.items():
        if isinstance(val, bool):
            lines.append(f"{key} = {'true' if val else 'false'}")
        elif isinstance(val, (int, float)):
            lines.append(f"{key} = {val}")
        else:
            s = str(val).replace("\\", "\\\\").replace('"', '\\"')
            lines.append(f'{key} = "{s}"')
    lines.append("")
    out.write_text("\n".join(lines), encoding="utf-8")
    return out
