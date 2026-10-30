# Copyright (c) 2026, Nathaniel T. Berry
# Licensed under the Apache License, Version 2.0.
"""Command-line interface for symsight."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from symsight.brand import BrandError, list_brands, resolve_brand
from symsight.brandcheck import scan_paths
from symsight.config import get_config
from symsight.finalize import FinalizeError, finalize_draft
from symsight.generate import GenerateError, generate_and_write
from symsight.image import ImageError, render_image
from symsight.models import ContentFormat, GenerateRequest


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="symsight",
        description="Brand-agnostic insight generator (articles & social posts)",
    )
    parser.add_argument(
        "--config-root",
        type=Path,
        default=None,
        help="Project root for config resolution (default: auto)",
    )
    sub = parser.add_subparsers(dest="command", required=False)

    # generate
    g = sub.add_parser("generate", help="Generate a draft")
    g.add_argument("--brand", default=None, help="Brand id (default: active_brand from config)")
    g.add_argument("--brand-file", type=Path, default=None, help="Path to brand YAML")
    g.add_argument("--type", dest="type_id", default=None, help="Content type id from brand")
    g.add_argument(
        "--format",
        choices=("article", "social"),
        default="article",
        help="Output format (default: article)",
    )
    g.add_argument("--topic", default=None, help="Topic / focus")
    g.add_argument("--min-words", type=int, default=None)
    g.add_argument("--max-words", type=int, default=None)
    g.add_argument("--max-chars", type=int, default=None, help="Social max characters")
    g.add_argument("--search", action="store_true", default=None, help="Force web_search on")
    g.add_argument("--no-search", action="store_true", help="Disable web_search")
    g.add_argument("--model", default=None, help="Override model id")
    g.add_argument(
        "--tone",
        "--voice",
        dest="tone",
        default=None,
        help="Tone id from the brand (alias: --voice). Default: brand default_tone",
    )
    g.add_argument("--drafts-dir", type=Path, default=None)

    # finalize
    f = sub.add_parser("finalize", help="Move/copy draft to final directory")
    f.add_argument("draft", type=Path, help="Path to draft markdown")
    f.add_argument("--copy", action="store_true", help="Copy instead of move")
    f.add_argument("--brand", default=None)
    f.add_argument("--brand-file", type=Path, default=None)
    f.add_argument("--final-dir", type=Path, default=None)
    f.add_argument("--skip-brand-check", action="store_true")

    # check
    c = sub.add_parser("check", help="Scan paths for forbidden brand terms")
    c.add_argument("paths", nargs="*", type=Path, help="Files or dirs (default: drafts + final)")
    c.add_argument("--brand", default=None)
    c.add_argument("--brand-file", type=Path, default=None)

    # brands
    b = sub.add_parser("brands", help="List configured brands")
    b.add_argument("--brands-dir", type=Path, default=None)

    # image
    image = sub.add_parser("image", help="Generate an image from the brand file's image.style")
    image.add_argument("--prompt", required=True, help="What the panel shows")
    image.add_argument("--out", type=Path, required=True, help="Destination image path")
    image.add_argument("--aspect", default=None, help="Override the brand aspect ratio")
    image.add_argument("--quality", default=None, help="Override the brand quality")
    image.add_argument("--model", default=None, help="Override the brand image model")
    image.add_argument("--brand", default=None, help="Brand id (default: active_brand from config)")
    image.add_argument("--brand-file", type=Path, default=None, help="Path to brand YAML")

    # tui
    t = sub.add_parser("tui", help="Launch the Textual TUI")
    t.add_argument("--drafts-dir", type=Path, default=None)
    t.add_argument("--final-dir", type=Path, default=None)
    t.add_argument("--brand", default=None)
    t.add_argument("--brands-dir", type=Path, default=None)

    return parser


def _resolve_use_search(args: argparse.Namespace) -> bool | None:
    if getattr(args, "no_search", False):
        return False
    if getattr(args, "search", None):
        return True
    return None


def cmd_generate(args: argparse.Namespace) -> int:
    overrides = {}
    if args.drafts_dir:
        overrides["drafts_dir"] = args.drafts_dir
    if args.brand:
        overrides["active_brand"] = args.brand
    if args.model:
        overrides["model"] = args.model
    cfg = get_config(root=args.config_root, overrides=overrides or None)

    try:
        brand = resolve_brand(
            brands_dir=cfg.brands_dir,
            brand_id=args.brand or cfg.active_brand,
            brand_path=args.brand_file,
        )
    except BrandError as exc:
        print(f"Brand error: {exc}", file=sys.stderr)
        return 1

    type_id = args.type_id
    if not type_id:
        if not brand.types:
            print("Brand has no types defined.", file=sys.stderr)
            return 1
        type_id = next(iter(brand.types))
        print(f"Using default type: {type_id}", file=sys.stderr)

    fmt = ContentFormat(args.format)
    req = GenerateRequest(
        brand=brand,
        type_id=type_id,
        format=fmt,
        topic=args.topic,
        min_words=args.min_words,
        max_words=args.max_words,
        max_chars=args.max_chars,
        use_search=_resolve_use_search(args),
        model=args.model,
        tone=args.tone,
    )
    try:
        selected = req.resolved_tone()
    except ValueError as exc:
        print(f"Generation failed: {exc}", file=sys.stderr)
        return 1
    if not (args.tone or "").strip() and selected is not None:
        print(f"Using default tone: {selected[0]}", file=sys.stderr)
    tone_label = f", tone {selected[0]}" if selected is not None else ""

    print(f"Generating {fmt.value} ({type_id}{tone_label}) for {brand.display_name}…")
    try:
        path = generate_and_write(req, cfg)
    except (GenerateError, RuntimeError, ValueError) as exc:
        print(f"Generation failed: {exc}", file=sys.stderr)
        return 1

    try:
        rel = path.relative_to(cfg.project_root)
    except ValueError:
        rel = path
    print(f"Draft written: {rel}")
    print(f"Title: {path.name}")
    print("Review the draft, then finalize with:")
    print(f"  symsight finalize {rel}")
    return 0


def cmd_finalize(args: argparse.Namespace) -> int:
    overrides = {}
    if args.final_dir:
        overrides["final_dir"] = args.final_dir
    if args.brand:
        overrides["active_brand"] = args.brand
    cfg = get_config(root=args.config_root, overrides=overrides or None)

    draft = args.draft
    if not draft.is_absolute():
        candidate = (Path.cwd() / draft).resolve()
        if not candidate.exists():
            candidate = (cfg.project_root / draft).resolve()
        draft = candidate

    brand = None
    if not args.skip_brand_check:
        try:
            brand = resolve_brand(
                brands_dir=cfg.brands_dir,
                brand_id=args.brand or cfg.active_brand,
                brand_path=args.brand_file,
            )
        except BrandError as exc:
            print(f"Brand error: {exc}", file=sys.stderr)
            return 1

    try:
        dest = finalize_draft(draft, final_dir=cfg.final_dir, brand=brand, copy=args.copy)
    except FinalizeError as exc:
        print(f"Finalize failed: {exc}", file=sys.stderr)
        return 1

    try:
        rel = dest.relative_to(cfg.project_root)
    except ValueError:
        rel = dest
    print(f"Final: {rel}")
    return 0


def cmd_check(args: argparse.Namespace) -> int:
    overrides = {}
    if args.brand:
        overrides["active_brand"] = args.brand
    cfg = get_config(root=args.config_root, overrides=overrides or None)

    try:
        brand = resolve_brand(
            brands_dir=cfg.brands_dir,
            brand_id=args.brand or cfg.active_brand,
            brand_path=args.brand_file,
        )
    except BrandError as exc:
        print(f"Brand error: {exc}", file=sys.stderr)
        return 1

    if not brand.forbidden:
        print(f"Brand {brand.id!r} has no forbidden terms; nothing to check.")
        return 0

    paths = list(args.paths) if args.paths else [cfg.drafts_dir, cfg.final_dir]
    problems = scan_paths(paths, brand)
    if problems:
        print(f"Brand check FAILED for {brand.display_name} — forbidden term(s) found.\n")
        for path, hits in problems:
            try:
                rel = path.relative_to(cfg.project_root)
            except ValueError:
                rel = path
            print(f"  • {rel}: {', '.join(hits)}")
        return 1

    print(f"Brand check passed for {brand.display_name}: no forbidden terms found.")
    return 0


def cmd_brands(args: argparse.Namespace) -> int:
    overrides = {}
    if args.brands_dir:
        overrides["brands_dir"] = args.brands_dir
    cfg = get_config(root=args.config_root, overrides=overrides or None)
    brands = list_brands(cfg.brands_dir)
    if not brands:
        print(f"No brands found in {cfg.brands_dir}")
        return 0
    for b in brands:
        types = ", ".join(b.types.keys()) or "(no types)"
        active = " *" if b.id == cfg.active_brand else ""
        print(f"{b.id}{active}: {b.display_name}  types=[{types}]")
    return 0


def cmd_image(args: argparse.Namespace) -> int:
    overrides = {}
    if args.brand:
        overrides["active_brand"] = args.brand
    cfg = get_config(root=args.config_root, overrides=overrides or None)

    try:
        brand = resolve_brand(
            brands_dir=cfg.brands_dir,
            brand_id=args.brand or cfg.active_brand,
            brand_path=args.brand_file,
        )
    except BrandError as exc:
        print(f"Brand error: {exc}", file=sys.stderr)
        return 1

    if not brand.image.style.strip():
        print(
            f"Image failed: Brand '{brand.id}' has no image.style. "
            "Add an image.style block to the brand file.",
            file=sys.stderr,
        )
        return 1

    try:
        key = cfg.require_api_key()
    except RuntimeError as exc:
        print(exc, file=sys.stderr)
        return 1

    dest = args.out if args.out.is_absolute() else cfg.project_root / args.out
    try:
        path = render_image(
            brand,
            args.prompt,
            dest,
            aspect=args.aspect,
            quality=args.quality,
            model=args.model,
            api_key=key,
            base_url=cfg.base_url,
        )
    except ImageError as exc:
        print(f"Image failed: {exc}", file=sys.stderr)
        return 1

    try:
        rel = path.relative_to(cfg.project_root)
    except ValueError:
        rel = path
    print(f"Image written: {rel}")
    return 0


def cmd_tui(args: argparse.Namespace) -> int:
    overrides = {}
    if args.drafts_dir:
        overrides["drafts_dir"] = args.drafts_dir
    if args.final_dir:
        overrides["final_dir"] = args.final_dir
    if args.brand:
        overrides["active_brand"] = args.brand
    if args.brands_dir:
        overrides["brands_dir"] = args.brands_dir
    cfg = get_config(root=args.config_root, overrides=overrides or None)

    from symsight.tui.app import run_tui

    run_tui(cfg)
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = _build_parser()
    args = parser.parse_args(argv)
    if not args.command:
        parser.print_help()
        return 0
    handlers = {
        "generate": cmd_generate,
        "finalize": cmd_finalize,
        "check": cmd_check,
        "brands": cmd_brands,
        "image": cmd_image,
        "tui": cmd_tui,
    }
    return handlers[args.command](args)


if __name__ == "__main__":
    sys.exit(main())
