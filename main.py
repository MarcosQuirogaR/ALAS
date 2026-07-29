#!/usr/bin/env python
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
ALAS headless CLI launcher.

The desktop app is ``desktop/`` (a Go/Wails shell + React frontend), started
with ``wails dev``/``wails build`` -- it talks to the compute core through the
FastAPI sidecar (``python -m alas.sidecar.server``), not through this
script. This entry point is for headless/scripted runs only.

Examples
--------
    python main.py -c configs/example_config.yaml --plots   # headless run
    python main.py --no-optimize --plots   # headless: just analyze the nominal design
    python main.py --save-config my.yaml   # write a starting config to edit
"""

import sys

from alas.cli import main

if __name__ == "__main__":
    argv = sys.argv[1:]
    if not argv:  # no arguments -> show usage instead of a long default run
        argv = ["--help"]
    raise SystemExit(main(argv))
