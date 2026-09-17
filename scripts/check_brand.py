#!/usr/bin/env python3
# Copyright (c) 2026, Nathaniel T. Berry
# Licensed under the Apache License, Version 2.0.
"""Thin wrapper: brand forbidden-term check."""

from __future__ import annotations

import sys

from symsight.cli import main

if __name__ == "__main__":
    argv = sys.argv[1:]
    if not argv or argv[0] not in {"check", "-h", "--help"}:
        argv = ["check", *argv]
    sys.exit(main(argv))
