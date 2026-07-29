#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
`wails build`'s single pre-build hook (`desktop/wails.json`'s `preBuildHooks`)
-- runs both freeze/bundle steps in sequence: the main FastAPI sidecar
(`scripts/build_sidecar.py`) and the SUAVE mission-analysis runtime
(`scripts/build_suave_env.py`). Kept as one script (rather than shell-chaining
two commands in `wails.json`) so it works identically regardless of which
shell Wails' preBuildHooks runner uses on a given OS.
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import build_sidecar
import build_suave_env


def main() -> None:
    build_sidecar.main()
    build_suave_env.main()


if __name__ == "__main__":
    main()
