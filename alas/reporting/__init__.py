# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""Reporting layer: data export, console summaries, and visualization."""

__all__ = [
    "report_to_dict",
    "export_json",
    "export_airfoil_dat",
    "format_summary",
    "print_summary",
    # Visualization is imported lazily (see alas.reporting.visualization) so
    # headless runs do not require matplotlib.
]

_DESIGN_REPORT_EXPORTS = frozenset(__all__)


def __getattr__(name: str):
    # PEP 562 lazy exports, same reasoning as the top-level package __init__:
    # design_report pulls in analysis.full_analysis (~1s), which importers of
    # sibling submodules (reporting.theme, the sidecar's hot path) don't need.
    if name in _DESIGN_REPORT_EXPORTS:
        from . import design_report

        return getattr(design_report, name)
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
