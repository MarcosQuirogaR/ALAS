# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-mass::torenbeek``: AeroSandbox's Torenbeek wing/fuselage weight
correlations, scoped to the two entry points `alas/physics/mass.py` (a
separate, not-yet-ported module) calls: ``mass_wing`` and
``mass_fuselage_simple``, plus ``mass_wing``'s three private helpers
(``mass_wing_high_lift_devices``, ``mass_wing_basic_structure``,
``mass_wing_spoilers_and_speedbrakes``), which each get their own case set
so a disagreement in the composed ``mass_wing`` total can be localized to one
helper.

Geometry is the same three wings and the main fuselage
``gen_geom_asb_wing.py``/``gen_geom_asb_fuselage.py`` already build (shaped
like ``aircraft_builder.py``'s main wing, hstab, vstab and fuselage), rebuilt
here from the same literal values rather than imported, for the reason those
two generators already record: this generator exercises AeroSandbox's own
classes directly and has no other reason to depend on ``alas-config``'s
Python counterpart.

The scalar parameters (load factor, TOGW, airspeeds, flap angle, strut
location, main-gear mounting) are spread around the literal values
`alas/physics/mass.py` actually passes (``DesignRequirements``/
``MassModelConfig`` defaults, recorded next to each case below), plus one
case each exercising ``strut_y_location`` and ``main_gear_mounted_to_wing``
away from what that call site ever passes, since a future caller might.
"""

from __future__ import annotations

import _framework
from aerosandbox.geometry.airfoil.airfoil import Airfoil
from aerosandbox.geometry.fuselage import Fuselage, FuselageXSec
from aerosandbox.geometry.wing import Wing, WingXSec
from aerosandbox.library.weights.torenbeek_weights import (
    mass_fuselage_simple,
    mass_wing,
    mass_wing_basic_structure,
    mass_wing_high_lift_devices,
    mass_wing_spoilers_and_speedbrakes,
)


def _build_main_wing() -> Wing:
    # alas.config.design_variables.DesignVector defaults.
    span_m = 71.75
    root_chord_m = 16.50
    break_chord_m = 7.80
    tip_chord_m = 1.60
    sweep_deg = 34.00
    tip_twist_deg = 0.00

    # alas.config.geometry_config.WingConfig defaults.
    root_z_m = -2.1
    break_z_m = -0.3
    tip_z_m = 2.5
    root_twist_deg = 4.0
    break_twist_deg = 2.0
    break_span_fraction = 0.35
    outboard_sweep_decrement_deg = 2.0

    import math

    semi_span = span_m / 2
    y_break = break_span_fraction * semi_span
    sweep_in = math.radians(sweep_deg)
    sweep_out = math.radians(sweep_deg - outboard_sweep_decrement_deg)
    dx_break = y_break * math.tan(sweep_in)
    dx_tip = dx_break + (semi_span - y_break) * math.tan(sweep_out)

    root_section = Airfoil("naca4412")
    tip_airfoil = Airfoil("naca2410")

    return Wing(
        name="Main Wing",
        symmetric=True,
        xsecs=[
            WingXSec(
                xyz_le=[0, 0, root_z_m],
                chord=root_chord_m,
                twist=root_twist_deg,
                airfoil=root_section,
            ),
            WingXSec(
                xyz_le=[dx_break, y_break, break_z_m],
                chord=break_chord_m,
                twist=break_twist_deg,
                airfoil=root_section,
            ),
            WingXSec(
                xyz_le=[dx_tip, semi_span, tip_z_m],
                chord=tip_chord_m,
                twist=tip_twist_deg,
                airfoil=tip_airfoil,
            ),
        ],
    )


def _build_hstab() -> Wing:
    # alas.config.geometry_config.EmpennageConfig defaults, tail_scale=1.0.
    hstab_root_chord_m = 8.0
    hstab_tip_chord_m = 2.2
    hstab_root_twist_deg = -2.0
    hstab_tip_twist_deg = -2.0
    hstab_tip_le_m = (7.5, 11.0, 1.0)

    tail_airfoil = Airfoil("naca0012")

    return Wing(
        name="Horizontal Stabilizer",
        symmetric=True,
        xsecs=[
            WingXSec(
                xyz_le=[0, 0, 0],
                chord=hstab_root_chord_m,
                twist=hstab_root_twist_deg,
                airfoil=tail_airfoil,
            ),
            WingXSec(
                xyz_le=list(hstab_tip_le_m),
                chord=hstab_tip_chord_m,
                twist=hstab_tip_twist_deg,
                airfoil=tail_airfoil,
            ),
        ],
    )


def _build_vstab() -> Wing:
    # alas.config.geometry_config.EmpennageConfig defaults, tail_scale=1.0.
    vstab_root_chord_m = 9.5
    vstab_tip_chord_m = 3.2
    vstab_tip_le_m = (9.0, 0.0, 9.8)

    tail_airfoil = Airfoil("naca0012")

    return Wing(
        name="Vertical Stabilizer",
        symmetric=False,
        xsecs=[
            WingXSec(
                xyz_le=[0, 0, 0],
                chord=vstab_root_chord_m,
                twist=0.0,
                airfoil=tail_airfoil,
            ),
            WingXSec(
                xyz_le=list(vstab_tip_le_m),
                chord=vstab_tip_chord_m,
                twist=0.0,
                airfoil=tail_airfoil,
            ),
        ],
    )


# alas.config.geometry_config.FuselageConfig defaults.
_DIAMETER_M = 6.2
_NOSE_Z_M = -0.5
_CABIN_START_X_M = 6.0
_CABIN_Z_M = 0.2
_TAILCONE_LENGTH_M = 14.0
_TAIL_Z_M = 1.8

# alas.config.design_variables.DesignVector default.
_FUSELAGE_LENGTH_M = 76.72

_RADIUS = _DIAMETER_M / 2
_CABIN_END = _FUSELAGE_LENGTH_M - _TAILCONE_LENGTH_M


def _fuselage_stations():
    import aerosandbox.numpy as anp

    stations = []

    x_nose = anp.sinspace(0, 1, 10)
    for xi in x_nose[:-1]:
        x_val = xi * _CABIN_START_X_M
        z_val = _CABIN_Z_M + (_NOSE_Z_M - _CABIN_Z_M) * (1 - xi) ** 2
        r_val = _RADIUS * (1 - (1 - xi) ** 2) ** 0.5
        stations.append((float(x_val), float(z_val), float(r_val)))

    stations.append((_CABIN_START_X_M, _CABIN_Z_M, _RADIUS))
    stations.append((_CABIN_END, _CABIN_Z_M, _RADIUS))

    x_tail = anp.linspace(0, 1, 10)
    for xi in x_tail[1:]:
        x_val = _CABIN_END + xi * _TAILCONE_LENGTH_M
        z_val = _CABIN_Z_M + (_TAIL_Z_M - _CABIN_Z_M) * xi**1.5
        r_val = _RADIUS * (1 - xi**1.5)
        stations.append((float(x_val), float(z_val), float(r_val)))

    return stations


def _build_main_fuselage() -> Fuselage:
    xsecs = [FuselageXSec(xyz_c=[x, 0, z], radius=r) for x, z, r in _fuselage_stations()]
    return Fuselage(name="Fuselage", xsecs=xsecs)


def _build_ovoid_fuselage(height_m: float) -> Fuselage:
    xsecs = []
    for x, z, r in _fuselage_stations():
        local_width = r * 2
        local_height = r * 2 * (height_m / _DIAMETER_M)
        xsecs.append(
            FuselageXSec(xyz_c=[x, 0, z], width=local_width, height=local_height, shape=2.0)
        )
    return Fuselage(name="Fuselage", xsecs=xsecs)


def main() -> None:
    wings = {
        "main_wing": _build_main_wing(),
        "hstab": _build_hstab(),
        "vstab": _build_vstab(),
    }
    fuselages = {
        "main_fuselage": _build_main_fuselage(),
        "ovoid_fuselage": _build_ovoid_fuselage(height_m=7.5),
    }

    # alas.config.requirements.DesignRequirements / MassModelConfig defaults,
    # and the literal spread mass.py's own call sites exercise -- see the
    # module doc.
    high_lift_cases = [
        {"wing": "main_wing", "max_airspeed_for_flaps": 92.6, "flap_deflection_angle": 30.0},
        {"wing": "main_wing", "max_airspeed_for_flaps": 92.6, "flap_deflection_angle": 15.0},
        {"wing": "hstab", "max_airspeed_for_flaps": 0.0, "flap_deflection_angle": 0.0},
        {"wing": "vstab", "max_airspeed_for_flaps": 0.0, "flap_deflection_angle": 0.0},
    ]
    high_lift_results = []
    for case in high_lift_cases:
        value = mass_wing_high_lift_devices(
            wing=wings[case["wing"]],
            max_airspeed_for_flaps=case["max_airspeed_for_flaps"],
            flap_deflection_angle=case["flap_deflection_angle"],
        )
        high_lift_results.append({**case, "mass_high_lift_devices": float(value)})

    basic_structure_cases = [
        {
            "wing": "main_wing",
            "design_mass_TOGW": 285_000.0,
            "ultimate_load_factor": 3.75,
            "suspended_mass": 285_000.0 * 0.85,
            "never_exceed_airspeed": 190.0,
            "main_gear_mounted_to_wing": False,
            "strut_y_location": None,
        },
        {
            "wing": "main_wing",
            "design_mass_TOGW": 285_000.0,
            "ultimate_load_factor": 3.75,
            "suspended_mass": 285_000.0 * 0.85,
            "never_exceed_airspeed": 190.0,
            "main_gear_mounted_to_wing": True,
            "strut_y_location": None,
        },
        {
            "wing": "main_wing",
            "design_mass_TOGW": 285_000.0,
            "ultimate_load_factor": 3.75,
            "suspended_mass": 285_000.0 * 0.85,
            "never_exceed_airspeed": 190.0,
            "main_gear_mounted_to_wing": True,
            "strut_y_location": 6.0,
        },
        {
            "wing": "hstab",
            "design_mass_TOGW": 285_000.0,
            "ultimate_load_factor": 3.75,
            "suspended_mass": 0.0,
            "never_exceed_airspeed": 190.0,
            "main_gear_mounted_to_wing": False,
            "strut_y_location": None,
        },
        {
            "wing": "vstab",
            "design_mass_TOGW": 285_000.0,
            "ultimate_load_factor": 3.75,
            "suspended_mass": 0.0,
            "never_exceed_airspeed": 190.0,
            "main_gear_mounted_to_wing": False,
            "strut_y_location": None,
        },
    ]
    basic_structure_results = []
    for case in basic_structure_cases:
        value = mass_wing_basic_structure(
            wing=wings[case["wing"]],
            design_mass_TOGW=case["design_mass_TOGW"],
            ultimate_load_factor=case["ultimate_load_factor"],
            suspended_mass=case["suspended_mass"],
            never_exceed_airspeed=case["never_exceed_airspeed"],
            main_gear_mounted_to_wing=case["main_gear_mounted_to_wing"],
            strut_y_location=case["strut_y_location"],
        )
        basic_structure_results.append({**case, "mass_wing_basic": float(value)})

    spoilers_cases = [
        {"wing": "main_wing", "mass_basic_wing": 18_000.0},
        {"wing": "hstab", "mass_basic_wing": 900.0},
    ]
    spoilers_results = []
    for case in spoilers_cases:
        value = mass_wing_spoilers_and_speedbrakes(
            wing=wings[case["wing"]], mass_basic_wing=case["mass_basic_wing"]
        )
        spoilers_results.append({**case, "mass_spoilers_and_speedbrakes": float(value)})

    # The full `mass_wing` composition, at parameter combinations shaped
    # like `mass.py`'s three call sites (main wing, hstab, vstab) plus one
    # away from them (strut-braced, wing-mounted gear).
    mass_wing_cases = [
        {
            "wing": "main_wing",
            "design_mass_TOGW": 285_000.0,
            "ultimate_load_factor": 3.75,
            "suspended_mass": 285_000.0 * 0.85,
            "never_exceed_airspeed": 190.0,
            "max_airspeed_for_flaps": 92.6,
            "main_gear_mounted_to_wing": False,
            "flap_deflection_angle": 30.0,
            "strut_y_location": None,
        },
        {
            "wing": "hstab",
            "design_mass_TOGW": 285_000.0,
            "ultimate_load_factor": 3.75,
            "suspended_mass": 0.0,
            "never_exceed_airspeed": 190.0,
            "max_airspeed_for_flaps": 0.0,
            "main_gear_mounted_to_wing": False,
            "flap_deflection_angle": 0.0,
            "strut_y_location": None,
        },
        {
            "wing": "vstab",
            "design_mass_TOGW": 285_000.0,
            "ultimate_load_factor": 3.75,
            "suspended_mass": 0.0,
            "never_exceed_airspeed": 190.0,
            "max_airspeed_for_flaps": 0.0,
            "main_gear_mounted_to_wing": False,
            "flap_deflection_angle": 0.0,
            "strut_y_location": None,
        },
        {
            "wing": "main_wing",
            "design_mass_TOGW": 220_000.0,
            "ultimate_load_factor": 3.75,
            "suspended_mass": 220_000.0 * 0.8,
            "never_exceed_airspeed": 210.0,
            "max_airspeed_for_flaps": 85.0,
            "main_gear_mounted_to_wing": True,
            "flap_deflection_angle": 20.0,
            "strut_y_location": 8.0,
        },
    ]
    mass_wing_results = []
    for case in mass_wing_cases:
        value = mass_wing(
            wing=wings[case["wing"]],
            design_mass_TOGW=case["design_mass_TOGW"],
            ultimate_load_factor=case["ultimate_load_factor"],
            suspended_mass=case["suspended_mass"],
            never_exceed_airspeed=case["never_exceed_airspeed"],
            max_airspeed_for_flaps=case["max_airspeed_for_flaps"],
            main_gear_mounted_to_wing=case["main_gear_mounted_to_wing"],
            flap_deflection_angle=case["flap_deflection_angle"],
            strut_y_location=case["strut_y_location"],
        )
        mass_wing_results.append({**case, "mass_wing_total": float(value)})

    fuselage_cases = [
        {"fuselage": "main_fuselage", "never_exceed_airspeed": 190.0, "wing_to_tail_distance": 28.0},
        {"fuselage": "main_fuselage", "never_exceed_airspeed": 150.0, "wing_to_tail_distance": 22.5},
        {"fuselage": "ovoid_fuselage", "never_exceed_airspeed": 190.0, "wing_to_tail_distance": 28.0},
    ]
    fuselage_results = []
    for case in fuselage_cases:
        value = mass_fuselage_simple(
            fuselage=fuselages[case["fuselage"]],
            never_exceed_airspeed=case["never_exceed_airspeed"],
            wing_to_tail_distance=case["wing_to_tail_distance"],
        )
        fuselage_results.append({**case, "mass_fuselage": float(value)})

    _framework.write(
        "mass",
        "torenbeek",
        {
            "mass_wing_high_lift_devices": high_lift_results,
            "mass_wing_basic_structure": basic_structure_results,
            "mass_wing_spoilers_and_speedbrakes": spoilers_results,
            "mass_wing": mass_wing_results,
            "mass_fuselage_simple": fuselage_results,
        },
        description=(
            "aerosandbox.library.weights.torenbeek_weights: mass_wing (and its "
            "three private helpers) and mass_fuselage_simple, on wings/fuselages "
            "shaped like aircraft_builder.py's main wing, hstab, vstab and "
            "fuselage, across a spread of load factor, TOGW, airspeeds, flap "
            "angle, strut location and main-gear-mounting parameters"
        ),
    )


if __name__ == "__main__":
    main()
