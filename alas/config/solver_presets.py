# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Solver preset registry for ALAS.

Each preset bundles a complete :class:`SolverSettings` (strategy, population,
iteration budget, worker count, tolerance) under a short display name, so the
user can pick a speed/thoroughness tradeoff from a dropdown instead of hand
-tuning SciPy's ``differential_evolution`` knobs. Mirrors the
``AircraftPreset`` registry pattern in :mod:`alas.config.presets`.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Dict, List

from .optimizer_config import SolverSettings


@dataclass
class SolverPreset:
    """A named solver-configuration preset."""

    name: str
    display_name: str
    description: str
    settings: SolverSettings


_SOLVER_PRESET_REGISTRY: Dict[str, SolverPreset] = {}


def _register(preset: SolverPreset) -> None:
    _SOLVER_PRESET_REGISTRY[preset.name] = preset


def get_solver_preset(name: str) -> SolverPreset:
    """Look up a solver preset by name. Raises KeyError if not found."""
    if name not in _SOLVER_PRESET_REGISTRY:
        raise KeyError(
            f"Unknown solver preset '{name}'. Available: {sorted(_SOLVER_PRESET_REGISTRY)}"
        )
    return _SOLVER_PRESET_REGISTRY[name]


def available_solver_presets() -> List[str]:
    """Return list of all registered solver preset names in display order."""
    return list(_SOLVER_PRESET_REGISTRY.keys())


def solver_preset_display_names() -> Dict[str, str]:
    """Return dict mapping preset name -> display name."""
    return {k: v.display_name for k, v in _SOLVER_PRESET_REGISTRY.items()}


# ---------------------------------------------------------------------------
# Presets
# ---------------------------------------------------------------------------

_register(
    SolverPreset(
        name="quick_draft",
        display_name="Quick Draft",
        description=(
            "Fast, rough pass -- small population and few generations. Good for "
            "iterating on requirements/geometry before committing to a full run."
        ),
        settings=SolverSettings(
            strategy="best1bin",
            max_iterations=8,
            population_size=4,
            tolerance=0.02,
            workers=4,
            display_progress=True,
        ),
    )
)

_register(
    SolverPreset(
        name="balanced",
        display_name="Balanced (Recommended)",
        description="The default tradeoff -- good convergence in a reasonable wall-clock time.",
        settings=SolverSettings(),  # tracks OptimizerConfig's own defaults, incl. workers=4
    )
)

_register(
    SolverPreset(
        name="thorough",
        display_name="Thorough",
        description="Larger population and more generations for tighter convergence on a final design.",
        settings=SolverSettings(
            strategy="best1bin",
            max_iterations=30,
            population_size=10,
            tolerance=0.005,
            workers=4,
            display_progress=True,
        ),
    )
)

_register(
    SolverPreset(
        name="exhaustive",
        display_name="Exhaustive",
        description=(
            "Widest search -- large population, many generations, tight tolerance. "
            "Slowest option; use for a final high-confidence optimization."
        ),
        settings=SolverSettings(
            strategy="best1bin",
            max_iterations=60,
            population_size=15,
            tolerance=0.002,
            workers=4,
            display_progress=True,
        ),
    )
)
