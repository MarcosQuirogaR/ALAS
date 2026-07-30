# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Lazy module proxies for the sidecar's heavy imports.

``reporting.visualization`` imports aerosandbox (and with it casadi/scipy,
~11s cold) at module level, and both figure registries used to import it at
*their* module level -- so the whole chain ran before the sidecar could even
announce its port. The proxy defers that cost to the first attribute access
(i.e. the first figure actually rendered), and ``server.py``'s background
warm-up thread pays it right after startup so that first render isn't slow
either.

Deliberately minimal: attribute access only, which is all the figure
registries do (``viz.figure_*`` calls).
"""

from __future__ import annotations

import importlib
from typing import Any


class LazyModule:
    """Importlib-backed stand-in that resolves on first attribute access."""

    def __init__(self, module_name: str) -> None:
        self._module_name = module_name
        self._module: Any = None

    def __getattr__(self, attr: str) -> Any:
        if self._module is None:
            self._module = importlib.import_module(self._module_name)
        return getattr(self._module, attr)


viz = LazyModule("alas.reporting.visualization")
