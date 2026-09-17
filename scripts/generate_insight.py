#!/usr/bin/env python3
# Copyright (c) 2026, Nathaniel Berry
# Licensed under the Apache License, Version 2.0.
"""Thin wrapper: generate a draft (delegates to symsight CLI)."""

from __future__ import annotations

import sys

from symsight.cli import main

if __name__ == "__main__":
    # Prepend subcommand if user invoked like oakmon-web scripts
    argv = sys.argv[1:]
    if not argv or argv[0] not in {
        "generate",
        "finalize",
        "check",
        "brands",
        "tui",
        "-h",
        "--help",
    }:
        argv = ["generate", *argv]
    sys.exit(main(argv))
