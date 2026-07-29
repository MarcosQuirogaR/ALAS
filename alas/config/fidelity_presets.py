# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Analysis-fidelity preset registry for ALAS.

Each preset bundles the *resolution* knobs that trade analysis speed for
accuracy -- VLM polar-sweep point count and panel resolution
(:class:`~alas.config.analysis_config.AnalysisConfig`) -- under a short
display name. Scoped strictly to those three fields (``sweep_n_points``,
``spanwise_resolution``, ``chordwise_resolution``): it must not clobber the
physical/empirical assumption fields (tail efficiency, polar-fit windows,
etc.) that the user may have already tuned independently.

This is deliberately unrelated to the Performance tab's assumption presets
(see :mod:`alas.config.performance_presets`) -- fidelity is about *how
finely* the VLM analysis is resolved, not *what physical assumptions* (CLmax,
thrust lapse, OEI climb factors) the field-performance model uses.

Mirrors the ``AircraftPreset`` registry pattern in
:mod:`alas.config.presets`.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Dict, List

from .analysis_config import AnalysisConfig


@dataclass
class FidelityPreset:
    """A named analysis-fidelity preset (polar sweep point count + VLM panel resolution)."""

    name: str
    display_name: str
    description: str
    analysis: AnalysisConfig


_FIDELITY_PRESET_REGISTRY: Dict[str, FidelityPreset] = {}


def _register(preset: FidelityPreset) -> None:
    _FIDELITY_PRESET_REGISTRY[preset.name] = preset


def get_fidelity_preset(name: str) -> FidelityPreset:
    """Look up a fidelity preset by name. Raises KeyError if not found."""
    if name not in _FIDELITY_PRESET_REGISTRY:
        raise KeyError(
            f"Unknown fidelity preset '{name}'. Available: {sorted(_FIDELITY_PRESET_REGISTRY)}"
        )
    return _FIDELITY_PRESET_REGISTRY[name]


def available_fidelity_presets() -> List[str]:
    """Return list of all registered fidelity preset names in display order."""
    return list(_FIDELITY_PRESET_REGISTRY.keys())


def fidelity_preset_display_names() -> Dict[str, str]:
    """Return dict mapping preset name -> display name."""
    return {k: v.display_name for k, v in _FIDELITY_PRESET_REGISTRY.items()}


# ---------------------------------------------------------------------------
# Presets
# ---------------------------------------------------------------------------

_register(
    FidelityPreset(
        name="draft",
        display_name="Draft (fast)",
        description="Coarse polar sweep and panel resolution -- fastest feedback while iterating on requirements/geometry.",
        analysis=AnalysisConfig(
            sweep_n_points=7, spanwise_resolution=1, chordwise_resolution=1
        ),
    )
)

_register(
    FidelityPreset(
        name="standard",
        display_name="Standard (Recommended)",
        description="The default resolution -- a good balance of speed and accuracy for most runs.",
        analysis=AnalysisConfig(),  # sweep_n_points=15, spanwise/chordwise_resolution=1
    )
)

_register(
    FidelityPreset(
        name="high_fidelity",
        display_name="High Fidelity (slow)",
        description="Fine polar sweep and panel resolution for a final, high-confidence analysis. Slowest option.",
        analysis=AnalysisConfig(
            sweep_n_points=30, spanwise_resolution=3, chordwise_resolution=3
        ),
    )
)
