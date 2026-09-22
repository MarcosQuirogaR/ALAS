# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""US Standard Atmosphere (1976), read out of SUAVE's mission-analysis stack.

Every SUAVE mission segment attaches one of these to compute pressure,
temperature and gas properties at whatever altitude the segment solver is
currently integrating through -- a different reference implementation from
AeroSandbox's ISA (``gen_atmo.py``), so it gets its own fixture rather than
reusing that one's cases.

``compute_values`` takes a *geometric* altitude and converts it to
geopotential internally (``z / (1 + z / R_earth)``) before consulting the
break-point table, so the altitudes below are geometric, not the table's own
geopotential break values -- landing near a break geometrically does not land
exactly on it geopotentially, which is close enough to exercise each segment
without needing to solve the conversion's inverse. Boundary-precision
behaviour (which segment wins exactly at a break, and how large the
table-rounding jump there actually is) is instead pinned down by
``alas-atmo``'s own unit tests, which can reach the crate's private
geopotential-space entry point directly; this fixture only has to prove the
public, geometric-altitude API agrees with SUAVE end to end.

This module is separate from ``gen_atmo.py`` because it needs SUAVE on the
path (``.suave-venv``), not AeroSandbox.
"""

from __future__ import annotations

import _framework

# A spread of geometric altitudes: below the model's floor and above its
# ceiling (both exercise clamping), sea level, a point near each of the
# table's eight geopotential breaks (not exact, since the geometric input is
# what this fixture varies), and a point inside each segment.
ALTITUDES_M = [
    -100_000.0,  # well below the floor: clamps to -2 km geopotential
    -2000.0,  # near the floor itself
    -1000.0,  # inside the lowest segment
    0.0,  # sea level
    5000.0,  # inside the troposphere
    11_000.0,  # near the tropopause
    15_000.0,  # inside the first isothermal segment
    20_000.0,  # near the second breakpoint
    25_000.0,  # inside the third segment
    32_000.0,  # near the third breakpoint
    40_000.0,  # inside the fourth segment
    47_000.0,  # near the fourth breakpoint
    49_000.0,  # inside the second isothermal segment
    51_000.0,  # near the fifth breakpoint
    60_000.0,  # inside the sixth segment
    71_000.0,  # near the sixth breakpoint
    78_000.0,  # inside the seventh segment
    84_852.0,  # near the table's own ceiling
    90_000.0,  # above the ceiling: clamps
    200_000.0,  # well above the ceiling: clamps to the same value as 90000
]

# A handful of cases carry a nonzero temperature deviation, to pin down that
# it shifts temperature (and everything derived from it) without moving
# pressure -- one warmer than standard, one colder.
DEVIATIONS_K = {
    0.0: 8.0,
    20_000.0: -6.5,
    60_000.0: 15.0,
}


def main() -> None:
    _framework.add_suave_to_path()
    import SUAVE  # noqa: PLC0415 - needs the path set first

    atmosphere = SUAVE.Analyses.Atmospheric.US_Standard_1976()

    # Sanity check on the sea-level definition itself: if this drifts, either
    # the wrong SUAVE version is on the path or the break-point table has
    # changed, and that is worth failing loudly on rather than writing a
    # fixture nobody can trust.
    sea_level = atmosphere.compute_values(0.0, 0.0)
    if float(sea_level.pressure[0]) != 101_325.0:
        raise SystemExit(
            f"US1976 sea-level pressure read as {float(sea_level.pressure[0])!r}, expected 101325.0"
        )
    if float(sea_level.temperature[0]) != 288.15:
        raise SystemExit(
            f"US1976 sea-level temperature read as {float(sea_level.temperature[0])!r}, expected 288.15"
        )

    cases = []
    for altitude_m in ALTITUDES_M:
        temperature_deviation_k = DEVIATIONS_K.get(altitude_m, 0.0)
        values = atmosphere.compute_values(altitude_m, temperature_deviation_k)
        cases.append(
            {
                "altitude_m": altitude_m,
                "temperature_deviation_k": temperature_deviation_k,
                # `compute_values` always returns a column array, even for a
                # scalar input (`atleast_2d_col`); `[0]` reads the one row
                # back out, and `float()` turns the resulting 0-d/1-element
                # array into a plain number the same way printing it would.
                "pressure_pa": float(values.pressure[0]),
                "temperature_k": float(values.temperature[0]),
                "density_kg_m3": float(values.density[0]),
                "speed_of_sound_m_s": float(values.speed_of_sound[0]),
                "dynamic_viscosity_pa_s": float(values.dynamic_viscosity[0]),
                "kinematic_viscosity_m2_s": float(values.kinematic_viscosity[0]),
                "thermal_conductivity_w_m_k": float(values.thermal_conductivity[0]),
                "prandtl_number": float(values.prandtl_number[0]),
            }
        )

    _framework.write(
        "atmo",
        "us1976",
        {"cases": cases},
        description=(
            "SUAVE US_Standard_1976.compute_values: pressure, temperature and "
            "every derived gas property, at a spread of geometric altitudes "
            "covering every segment and both clamped extremes"
        ),
    )


if __name__ == "__main__":
    main()
