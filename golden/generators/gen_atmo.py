# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""International Standard Atmosphere, read out of AeroSandbox's closed form.

AeroSandbox's ``Atmosphere`` defaults to a CasADi B-spline fit of the 1976
COESA model ("differentiable"), and ALAS call sites normally retain that
default. This fixture deliberately exercises the separate ``method="isa"``
branch because the Rust crate translates that upstream API as well. Product
call-path evidence for the default belongs to ``gen_atmo_differentiable.py``;
using these closed-form values to validate an ALAS default call would test the
wrong branch even if both happened to be numerically close.

The case list walks every base altitude in AeroSandbox's ISA table (the layer
boundaries where a shifted index or an off-by-one in the table walk would
show up), a point inside each of the eight layers, a point below the first
layer's base (where the model extrapolates rather than clamping), and a point
well above the table's top. A handful of cases also carry a nonzero
``temperature_deviation`` to pin down that it shifts ``temperature()`` and
everything derived from it, without altering ``pressure()``.
"""

from __future__ import annotations

import _framework

# One point inside each of the eight ISA layers, plus every layer boundary
# and both a sub-surface and a well-above-the-table extrapolation. Ordered by
# altitude so a reader can see the layer walk directly.
ALTITUDES_M = [
    -5000.0,  # below the first layer's base: exercises the lower-bound branch
    0.0,  # layer 0 base
    5000.0,  # inside layer 0
    11000.0,  # layer 1 base
    15000.0,  # inside layer 1
    20000.0,  # layer 2 base
    25000.0,  # inside layer 2
    32000.0,  # layer 3 base
    40000.0,  # inside layer 3
    47000.0,  # layer 4 base
    49000.0,  # inside layer 4
    51000.0,  # layer 5 base
    60000.0,  # inside layer 5
    71000.0,  # layer 6 base
    78000.0,  # inside layer 6
    84852.0,  # layer 7 base, the table's last row
    90000.0,  # inside layer 7, above the class's documented valid range
    100000.0,  # well past the table, extrapolating the top layer further
]

# A characteristic length for the Knudsen number: small enough that the
# number moves visibly relative to the mean free path itself, distinguishing
# a bug in `knudsen` from one already caught by `mean_free_path`.
KNUDSEN_LENGTH_M = 0.1

# Altitudes exercised with a nonzero temperature deviation, to pin down that
# it shifts temperature (and everything derived from it) without touching
# pressure. One case is warmer than ISA, one colder.
DEVIATIONS_K = {
    0.0: 5.0,
    11000.0: -8.0,
    40000.0: 12.5,
}


def main() -> None:
    _framework.add_alas_to_path()
    from aerosandbox.atmosphere.atmosphere import Atmosphere

    # Sanity check on the sea-level definition itself: if this drifts, every
    # other case is being read from the wrong branch or the wrong AeroSandbox
    # version, and that is worth failing loudly on rather than writing a
    # fixture nobody can trust.
    sea_level = Atmosphere(altitude=0.0, method="isa")
    if sea_level.pressure() != 101325.0:
        raise SystemExit(f"ISA sea-level pressure read as {sea_level.pressure()!r}, expected 101325.0")
    if sea_level.temperature() != 288.15:
        raise SystemExit(f"ISA sea-level temperature read as {sea_level.temperature()!r}, expected 288.15")

    cases = []
    for altitude_m in ALTITUDES_M:
        temperature_deviation_k = DEVIATIONS_K.get(altitude_m, 0.0)
        atmo = Atmosphere(
            altitude=altitude_m,
            method="isa",
            temperature_deviation=temperature_deviation_k,
        )
        cases.append(
            {
                "altitude_m": altitude_m,
                "temperature_deviation_k": temperature_deviation_k,
                "knudsen_length_m": KNUDSEN_LENGTH_M,
                # `pressure_isa` builds its result through a chain of
                # `np.where` calls, which hands back a 0-d ndarray for a
                # scalar input rather than a plain float; `float()` reads the
                # number out the same way printing or comparing it would.
                "pressure_pa": float(atmo.pressure()),
                "temperature_k": float(atmo.temperature()),
                "density_kg_m3": float(atmo.density()),
                "speed_of_sound_m_s": float(atmo.speed_of_sound()),
                "dynamic_viscosity_pa_s": float(atmo.dynamic_viscosity()),
                "kinematic_viscosity_m2_s": float(atmo.kinematic_viscosity()),
                "ratio_of_specific_heats": float(atmo.ratio_of_specific_heats()),
                "mean_free_path_m": float(atmo.mean_free_path()),
                "knudsen_number": float(atmo.knudsen(KNUDSEN_LENGTH_M)),
                "density_altitude_m": float(atmo.density_altitude(method="approximate")),
            }
        )

    _framework.write(
        "atmo",
        "isa",
        {"cases": cases},
        description=(
            "AeroSandbox Atmosphere(method='isa'): pressure, temperature and "
            "every derived quantity, at each ISA table layer boundary, a "
            "point inside each layer, and both extrapolation directions"
        ),
    )


if __name__ == "__main__":
    main()
