# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Shared color palette for ALAS's plots.

Deliberately front-end-free: :mod:`alas.reporting.visualization` is
documented as headless-safe and decoupled from any front-end (the CLI/
pipeline reuse its figure factories to export print-friendly report PNGs, and
the sidecar's ``figures.py``/``figures_extra.py`` reuse them to serve SVG to
the desktop app), so the data those factories need -- which color is "dark",
which is "light" -- cannot live behind any UI-specific import. This module is
pure ``dataclasses`` + string formatting; the desktop app's ``theme.css`` is a
CSS port of the same palette values for its own chrome.

Every ``figure_*`` factory in ``visualization.py`` takes a trailing
``theme: str | None = None`` kwarg. ``None`` (the default used by every
existing headless caller, e.g. ``pipeline.py``'s report-PNG export) resolves
to the ``"light"`` palette -- the same white-background/dark-text look those
functions already rendered before this module existed, byte-for-byte. Sidecar
call sites pass the user's active theme name explicitly.
"""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class Palette:
    name: str
    bg: str  # figure/axes background
    panel: str  # frontend panel/input background (chrome only)
    border: str  # frontend widget border (chrome only)
    spine: str  # matplotlib axes spine color
    tick: str  # matplotlib tick/label color
    title: str  # matplotlib title / frontend primary text color
    accent: str  # frontend selection/highlight color


# NOTE: only the chrome fields (panel/border/accent) are tuned for the modern
# look below. The chart-affecting fields (bg/spine/tick/title) are left
# untouched so headless report PNGs (which use the "light" palette) render
# byte-for-byte identically -- see module docstring.
PALETTES: dict[str, Palette] = {
    "light": Palette(
        name="light",
        bg="#ffffff",
        panel="#f4f5f7",
        border="#dfe2e8",
        spine="#333333",
        tick="#333333",
        title="#000000",
        accent="#2563eb",
    ),
    "grey": Palette(
        name="grey",
        bg="#3a3a3a",
        panel="#41454c",
        border="#565b63",
        spine="#777777",
        tick="#dddddd",
        title="#ffffff",
        accent="#6aa2ff",
    ),
    "dark": Palette(
        name="dark",
        bg="#1e1e1e",
        panel="#262a31",
        border="#363b44",
        spine="#555555",
        tick="#cccccc",
        title="#ffffff",
        accent="#4f8cff",
    ),
}

DEFAULT_THEME = "dark"

# Cross-tab series colors so a comparison plot looks the same regardless of
# which tab/figure draws it: baseline is always blue, optimized always red.
BASELINE_COLOR = "tab:blue"
OPTIMIZED_COLOR = "tab:red"
# Muted/dashed trace for a prior run overlaid for comparison (Run History).
GHOST_COLOR = "#999999"


def get_palette(theme: str | None) -> Palette:
    """Resolve a theme name to its :class:`Palette`.

    ``None`` -- the default on every ``figure_*`` factory, used by every
    existing headless/report call site -- resolves to ``"light"``, so
    omitting the argument reproduces today's white-background behavior
    exactly. Unknown names fall back to ``"light"`` rather than raising, so a
    stale persisted theme name from a future settings format never crashes a
    plot.
    """
    return PALETTES.get(theme or "light", PALETTES["light"])
