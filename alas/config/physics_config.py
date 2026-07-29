# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Physics / drag-model configuration.

The semi-empirical drag buildup (Raymer component method + Korn wave-drag
estimate) relies on a handful of technology factors and form-factor parameters.
In the reference scripts these were inline literals (``* 1.10``, ``0.95``,
``np.radians(32)`` ...). They are gathered here so the fidelity assumptions are
explicit and adjustable.
"""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class DragModelConfig:
    """Tunable coefficients for the semi-empirical drag buildup."""

    max_thickness_chordwise_loc: float = field(
        default=0.35,
        metadata={
            "label": "Max-thickness chordwise location",
            "unit": "x/c",
            "help": "Chordwise location of maximum airfoil thickness (as a fraction of chord), used in the wing form factor.",
        },
    )

    interference_factor_wing: float = field(
        default=1.0,
        metadata={
            "label": "Wing interference factor (Q)",
            "help": "How much the wing-fuselage junction disturbs the local flow, multiplying the wing's parasite drag. 1.0 = no interference.",
        },
    )
    interference_factor_fuselage: float = field(
        default=1.25,
        metadata={
            "label": "Fuselage interference factor (Q)",
            "help": "How much neighbouring components (wing, tail) disturb the fuselage's flow, multiplying its parasite drag.",
        },
    )

    viscous_margin: float = field(
        default=1.10,
        metadata={
            "label": "Viscous drag margin",
            "help": "Lumped multiplier applied to the total parasite drag, accounting for excrescences, gaps and roughness "
            "not captured component-by-component. 1.10 = +10% margin.",
        },
    )

    interference_factor_nacelle: float = field(
        default=1.3,
        metadata={
            "label": "Nacelle/pylon interference factor (Q)",
            "help": "How much the engine nacelle and pylon disturb the local flow. Typical value 1.3 for podded under-wing installations (Raymer).",
        },
    )

    korn_technology_factor: float = field(
        default=0.95,
        metadata={
            "label": "Korn technology factor (kappa)",
            "help": "Airfoil-technology factor in the Korn wave-drag equation; ~0.95 for modern supercritical sections, "
            "lower for older/less efficient sections.",
        },
    )
    wave_drag_onset_mach: float = field(
        default=0.6,
        metadata={
            "label": "Wave-drag onset Mach",
            "help": "Below this Mach number, transonic wave drag is assumed zero (not yet computed).",
        },
    )
    wave_drag_coefficient: float = field(
        default=20.0,
        metadata={
            "label": "Wave-drag rise coefficient",
            "help": "Leading constant in the Korn wave-drag rise: CD_wave = coefficient * (M - M_drag_divergence)^4.",
        },
    )
