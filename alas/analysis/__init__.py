# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""Final high-fidelity analysis of optimized designs."""

from .full_analysis import (
    AnalysisReport,
    DesignPoint,
    FullAnalysis,
    PolarFit,
)

__all__ = [
    "FullAnalysis",
    "AnalysisReport",
    "DesignPoint",
    "PolarFit",
]
