# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""Golden generator for `alas.reporting.theme` (color palettes and themes).

Captures the light, grey, and dark palettes, default theme identifier, and
series colors from the reference Python implementation.
"""

from __future__ import annotations

import dataclasses
import _framework

_framework.add_alas_to_path()

import alas.reporting.theme as theme  # noqa: E402


def main():
    palettes_data = {}
    for name, pal in theme.PALETTES.items():
        palettes_data[name] = dataclasses.asdict(pal)

    resolved_tests = {
        "none": dataclasses.asdict(theme.get_palette(None)),
        "light": dataclasses.asdict(theme.get_palette("light")),
        "grey": dataclasses.asdict(theme.get_palette("grey")),
        "dark": dataclasses.asdict(theme.get_palette("dark")),
        "unknown": dataclasses.asdict(theme.get_palette("unrecognized_name")),
    }

    payload = {
        "palettes": palettes_data,
        "resolved": resolved_tests,
        "constants": {
            "default_theme": theme.DEFAULT_THEME,
            "baseline_color": theme.BASELINE_COLOR,
            "optimized_color": theme.OPTIMIZED_COLOR,
            "ghost_color": theme.GHOST_COLOR,
        },
    }

    _framework.write(
        "report",
        "theme",
        payload,
        description="Theme palettes, resolved fallback behavior, and plot series color constants.",
    )


if __name__ == "__main__":
    main()
