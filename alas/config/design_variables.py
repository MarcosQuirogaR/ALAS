# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Design vector definition.

The optimizer searches a *design space*. In the original reference scripts this
space was a bare Python list addressed by magic indices (``x[10]``, ``x[4]`` ...),
which made the code fragile and opaque. Here the same 16 degrees of freedom are
expressed as a named, self-describing structure with a single source of truth for
their bounds, defaults, units and meaning.

Two views of the same data are kept in sync:

* :data:`DESIGN_VARIABLE_SPECS` -- the ordered metadata (name, bounds, default,
  unit, description). This is what the optimizer reads to build its bounds list
  and what the UI/docs read to present the design space.
* :class:`DesignVector` -- a dataclass with one attribute per variable, so the
  geometry builder can write ``dv.span_m`` instead of ``x[0]``.

``DesignVector.to_array`` / ``DesignVector.from_array`` bridge to/from the flat
NumPy vector that SciPy's optimizer works with.
"""

from __future__ import annotations

from dataclasses import dataclass, fields
from typing import List, Sequence

import numpy as np


@dataclass(frozen=True)
class DesignVariableSpec:
    """Metadata describing a single design degree of freedom."""

    name: str  # must match the matching DesignVector attribute
    default: float  # nominal value (the AVE reference design)
    lower: float  # optimizer lower bound
    upper: float  # optimizer upper bound
    unit: str  # physical unit, "-" if dimensionless
    description: str  # human-readable meaning
    # Decimal places for *display only* (the Design Space table's Initial/
    # Lower/Upper columns) -- these are full-precision floats internally
    # (e.g. an optimizer result or a preset's exact design vector), and
    # rendering that raw precision (sweep_deg == 34.00000000000001) reads as
    # noise rather than information. Chosen per variable's actual working
    # resolution: 2 for the geometry/angle variables a user reasons about in
    # whole-to-tenth units, 3 for the ~unit-scale multipliers, 4 for the
    # Hicks-Henne bump amplitudes -- whose entire useful range spans about
    # 0.005, so 2-3 decimals would round every value to the same couple of
    # displayed steps.
    decimals: int = 2


# --- Single source of truth for the design space ----------------------------
# Order here defines the order of the flat optimization vector.
DESIGN_VARIABLE_SPECS: List[DesignVariableSpec] = [
    DesignVariableSpec(
        "span_m", 71.75, 60.0, 80.0, "m", "Full wingspan (tip to tip)", decimals=2
    ),
    DesignVariableSpec(
        "root_chord_m", 16.50, 12.0, 19.0, "m", "Chord at the wing root", decimals=2
    ),
    DesignVariableSpec(
        "break_chord_m",
        7.80,
        6.0,
        10.0,
        "m",
        "Chord at the trailing-edge break (yehudi)",
        decimals=2,
    ),
    DesignVariableSpec(
        "tip_chord_m", 1.60, 1.0, 3.0, "m", "Chord at the wingtip", decimals=2
    ),
    DesignVariableSpec(
        "sweep_deg",
        34.00,
        25.0,
        45.0,
        "deg",
        "Inboard leading-edge sweep angle",
        decimals=2,
    ),
    DesignVariableSpec(
        "tip_twist_deg",
        0.00,
        -5.0,
        1.0,
        "deg",
        "Geometric washout at tip (negative = washout)",
        decimals=2,
    ),
    DesignVariableSpec(
        "wing_x_shift_m",
        0.00,
        -5.0,
        8.0,
        "m",
        "Longitudinal shift of the wing root for CG balance",
        decimals=2,
    ),
    DesignVariableSpec(
        "tail_scale",
        1.00,
        0.75,
        1.25,
        "-",
        "Uniform scale factor on the empennage",
        decimals=3,
    ),
    DesignVariableSpec(
        "fuselage_length_m",
        76.72,
        65.0,
        85.0,
        "m",
        "Overall fuselage length",
        decimals=2,
    ),
    DesignVariableSpec(
        "tail_x_shift_m",
        0.00,
        -2.0,
        3.0,
        "m",
        "Longitudinal shift of the empennage",
        decimals=2,
    ),
    DesignVariableSpec(
        "airfoil_thickness_scale",
        1.00,
        0.80,
        1.30,
        "-",
        "Multiplier on root/break airfoil thickness",
        decimals=3,
    ),
    DesignVariableSpec(
        "airfoil_camber_scale",
        1.00,
        0.7,
        1.4,
        "-",
        "Multiplier on root/break airfoil camber",
        decimals=3,
    ),
    DesignVariableSpec(
        "bump_upper_front",
        0.00,
        -0.005,
        0.002,
        "-",
        "Hicks-Henne bump, upper surface ~25% chord (suction)",
        decimals=4,
    ),
    DesignVariableSpec(
        "bump_upper_rear",
        0.00,
        -0.005,
        0.002,
        "-",
        "Hicks-Henne bump, upper surface ~75% chord (shock/recovery)",
        decimals=4,
    ),
    DesignVariableSpec(
        "bump_lower_mid",
        0.00,
        -0.005,
        0.003,
        "-",
        "Hicks-Henne bump, lower surface ~40% chord (belly volume)",
        decimals=4,
    ),
    DesignVariableSpec(
        "bump_lower_rear",
        0.00,
        -0.005,
        0.003,
        "-",
        "Hicks-Henne bump, lower surface ~85% chord (rear loading)",
        decimals=4,
    ),
]


@dataclass
class DesignVector:
    """Named container for one candidate design.

    Field order MUST match :data:`DESIGN_VARIABLE_SPECS`; this is asserted at
    import time by :func:`_validate_consistency`.
    """

    span_m: float = 71.75
    root_chord_m: float = 16.50
    break_chord_m: float = 7.80
    tip_chord_m: float = 1.60
    sweep_deg: float = 34.00
    tip_twist_deg: float = 0.00
    wing_x_shift_m: float = 0.00
    tail_scale: float = 1.00
    fuselage_length_m: float = 76.72
    tail_x_shift_m: float = 0.00
    airfoil_thickness_scale: float = 1.00
    airfoil_camber_scale: float = 1.00
    bump_upper_front: float = 0.00
    bump_upper_rear: float = 0.00
    bump_lower_mid: float = 0.00
    bump_lower_rear: float = 0.00

    # -- conversions ---------------------------------------------------------
    def to_array(self) -> np.ndarray:
        """Flatten to the ordered NumPy vector the optimizer operates on."""
        return np.array([getattr(self, f.name) for f in fields(self)], dtype=float)

    @classmethod
    def from_array(cls, array: Sequence[float]) -> "DesignVector":
        """Rebuild a named design vector from a flat optimizer array."""
        names = [f.name for f in fields(cls)]
        if len(array) != len(names):
            raise ValueError(
                f"Expected {len(names)} design variables, got {len(array)}."
            )
        return cls(**{name: float(value) for name, value in zip(names, array)})

    # -- bounds helpers ------------------------------------------------------
    @staticmethod
    def bounds() -> List[tuple]:
        """Return the (lower, upper) bounds list, in vector order, for SciPy."""
        return [(spec.lower, spec.upper) for spec in DESIGN_VARIABLE_SPECS]

    @staticmethod
    def default() -> "DesignVector":
        """The nominal AVE reference design (all specs at their default)."""
        return DesignVector(**{s.name: s.default for s in DESIGN_VARIABLE_SPECS})


def _validate_consistency() -> None:
    """Guard against the dataclass fields and the spec list drifting apart."""
    spec_names = [s.name for s in DESIGN_VARIABLE_SPECS]
    field_names = [f.name for f in fields(DesignVector)]
    if spec_names != field_names:
        raise RuntimeError(
            "DESIGN_VARIABLE_SPECS and DesignVector fields are out of sync:\n"
            f"  specs : {spec_names}\n"
            f"  fields: {field_names}"
        )


_validate_consistency()
