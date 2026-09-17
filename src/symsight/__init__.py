# Copyright (c) 2026, Nathaniel T. Berry
# Licensed under the Apache License, Version 2.0.

"""symsight — brand-agnostic insight generator."""

from __future__ import annotations

try:
    from importlib.metadata import version as _pkg_version

    __version__ = _pkg_version("symsight")
except Exception:  # noqa: BLE001
    try:
        from symsight._native import __version__ as __version__
    except Exception:  # noqa: BLE001
        __version__ = "0.1.0"
