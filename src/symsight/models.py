# Copyright (c) 2026, Nathaniel T. Berry
# Licensed under the Apache License, Version 2.0.
"""Pydantic models for brands, drafts, and generation requests."""

from __future__ import annotations

from enum import Enum
from typing import Any

from pydantic import BaseModel, Field, field_validator


class ContentFormat(str, Enum):
    ARTICLE = "article"
    SOCIAL = "social"


class TypeSpec(BaseModel):
    """One content type under a brand (e.g. general, news)."""

    description: str = ""
    default_use_search: bool = False
    user_template: str


class ArticleFormatSpec(BaseModel):
    default_min_words: int = 200
    default_max_words: int = 500


class SocialFormatSpec(BaseModel):
    default_max_chars: int = 200


class FormatSpecs(BaseModel):
    article: ArticleFormatSpec = Field(default_factory=ArticleFormatSpec)
    social: SocialFormatSpec = Field(default_factory=SocialFormatSpec)


class Brand(BaseModel):
    """Brand identity and prompt templates — loaded only from YAML config."""

    id: str
    display_name: str
    voice: str
    full_name: str
    short_name: str = ""
    forbidden: list[str] = Field(default_factory=list)
    disclaimer: str = ""
    types: dict[str, TypeSpec] = Field(default_factory=dict)
    formats: FormatSpecs = Field(default_factory=FormatSpecs)

    @field_validator("short_name", mode="before")
    @classmethod
    def default_short_name(cls, v: Any, info: Any) -> str:
        if v:
            return str(v)
        # filled later if empty via model_validator-style post — keep simple
        return str(v or "")

    def model_post_init(self, context: Any, /) -> None:
        if not self.short_name:
            object.__setattr__(
                self,
                "short_name",
                self.full_name.split()[0] if self.full_name else self.id,
            )


class GenerateRequest(BaseModel):
    brand: Brand
    type_id: str
    format: ContentFormat = ContentFormat.ARTICLE
    topic: str | None = None
    min_words: int | None = None
    max_words: int | None = None
    max_chars: int | None = None
    use_search: bool | None = None
    model: str | None = None

    def resolved_type(self) -> TypeSpec:
        if self.type_id not in self.brand.types:
            known = ", ".join(sorted(self.brand.types)) or "(none)"
            raise ValueError(f"Unknown type {self.type_id!r} for brand {self.brand.id!r}. Known: {known}")
        return self.brand.types[self.type_id]

    def resolved_min_words(self) -> int:
        if self.min_words is not None:
            return self.min_words
        return self.brand.formats.article.default_min_words

    def resolved_max_words(self) -> int:
        if self.max_words is not None:
            return self.max_words
        return self.brand.formats.article.default_max_words

    def resolved_max_chars(self) -> int:
        if self.max_chars is not None:
            return self.max_chars
        return self.brand.formats.social.default_max_chars

    def resolved_use_search(self) -> bool:
        if self.use_search is not None:
            return self.use_search
        return self.resolved_type().default_use_search


class DraftMeta(BaseModel):
    """Sidecar / generation metadata."""

    model: str
    type: str
    format: str
    topic: str | None = None
    brand_id: str
    used_web_search: bool = False
    word_count: int | None = None
    char_count: int | None = None
    generated_at: str
    title: str | None = None
    path: str | None = None


class Draft(BaseModel):
    """In-memory draft representation."""

    path: Any = None  # Path | None — avoid circular import issues in annotations
    title: str
    body: str
    front_matter: dict[str, Any] = Field(default_factory=dict)
    meta: DraftMeta | None = None

    @property
    def status(self) -> str:
        return str(self.front_matter.get("status", "draft"))
