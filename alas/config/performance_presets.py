# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Performance-assumption preset registry for ALAS.

Each preset bundles the *physical/empirical* field-performance assumptions on
:class:`~alas.config.performance_config.PerformanceConfig` -- high-lift
CLmax, thrust lapse, OEI climb factors, balanced-field-length factor -- under
a named technology/high-lift profile. This is deliberately independent of
:mod:`alas.config.fidelity_presets` (which controls analysis *resolution*,
not physical assumptions) and does not touch ``matching_chart_resolution``
(a plotting-resolution knob, not a physical assumption).

Mirrors the ``AircraftPreset`` registry pattern in
:mod:`alas.config.presets`.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Dict, List

from .performance_config import PerformanceConfig


@dataclass
class PerformancePreset:
    """A named bundle of field-performance / high-lift assumptions."""

    name: str
    display_name: str
    description: str
    settings: PerformanceConfig


_PERFORMANCE_PRESET_REGISTRY: Dict[str, PerformancePreset] = {}


def _register(preset: PerformancePreset) -> None:
    _PERFORMANCE_PRESET_REGISTRY[preset.name] = preset


def get_performance_preset(name: str) -> PerformancePreset:
    """Look up a performance preset by name. Raises KeyError if not found."""
    if name not in _PERFORMANCE_PRESET_REGISTRY:
        raise KeyError(
            f"Unknown performance preset '{name}'. Available: {sorted(_PERFORMANCE_PRESET_REGISTRY)}"
        )
    return _PERFORMANCE_PRESET_REGISTRY[name]


def available_performance_presets() -> List[str]:
    """Return list of all registered performance preset names in display order."""
    return list(_PERFORMANCE_PRESET_REGISTRY.keys())


def performance_preset_display_names() -> Dict[str, str]:
    """Return dict mapping preset name -> display name."""
    return {k: v.display_name for k, v in _PERFORMANCE_PRESET_REGISTRY.items()}


# ---------------------------------------------------------------------------
# Presets
# ---------------------------------------------------------------------------
# Only the physical/empirical fields are varied; ws_min_pa/ws_max_pa (chart
# axis limits) and matching_chart_resolution (plot resolution) are left at
# PerformanceConfig()'s defaults for every preset.

_register(
    PerformancePreset(
        name="conservative_simple_flaps",
        display_name="Conservative (simple flaps)",
        description=(
            "Older-generation / regional-jet high-lift system (single/double-slotted flaps, no slats). "
            "Lower CLmax, gentler thrust lapse."
        ),
        settings=PerformanceConfig(
            cl_max_to=1.60,
            cl_max_land=2.20,
            thrust_lapse=0.20,
            oei_gradient=0.024,
            k_land=0.58,
            oei_climb_cl=1.0,
            oei_climb_delta_cd=0.020,
            bfl_factor=1.15,
        ),
    )
)

_register(
    PerformancePreset(
        name="standard_narrowbody",
        display_name="Standard Narrow-body (Recommended)",
        description="Typical modern narrow-body twin -- the PerformanceConfig defaults.",
        settings=PerformanceConfig(),  # cl_max_to=1.80, cl_max_land=2.60, thrust_lapse=0.235, ...
    )
)

_register(
    PerformancePreset(
        name="modern_narrowbody",
        display_name="Modern Narrow-body (slats + Fowler flaps)",
        description=(
            "Modern single-aisle twin with leading-edge slats and single/double-slotted Fowler flaps "
            "(e.g. A320/A220 family) -- higher CLmax than the generic 'standard narrow-body' bucket, "
            "which undersells this common, well-documented high-lift system and overestimates V-speeds "
            "by ~15-20 kt."
        ),
        settings=PerformanceConfig(cl_max_to=2.10, cl_max_land=2.90),
    )
)

_register(
    PerformancePreset(
        name="advanced_highlift_widebody",
        display_name="Advanced High-Lift (widebody)",
        description=(
            "Modern widebody with triple-slotted flaps + slats and high-bypass engines. "
            "Higher CLmax, steeper OEI climb requirement (tri/quad-class margin)."
        ),
        settings=PerformanceConfig(
            cl_max_to=2.10,
            cl_max_land=2.95,
            thrust_lapse=0.26,
            oei_gradient=0.027,
            k_land=0.63,
            oei_climb_cl=1.35,
            oei_climb_delta_cd=0.030,
            bfl_factor=1.18,
        ),
    )
)
