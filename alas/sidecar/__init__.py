# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Headless JSON/WebSocket API bridging :class:`~alas.pipeline.DesignPipeline`
to the desktop app -- a Go/Wails shell + React frontend (``desktop/``).

This package is the only part of ALAS that imports FastAPI/uvicorn. It
adds no physics of its own -- it is a thin JSON boundary over the same
``DesignPipeline``, ``ALASConfig``, and ``reporting`` modules the CLI
uses directly.
"""
