# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-geom::asb::wing``: AeroSandbox's ``Wing``/``WingXSec``, scoped
surface.

Covers the methods `docs/PORTING.md` names for this row --
``.translate()``, ``.subdivide_sections()``, ``.area()``, ``.span()``,
``.mean_aerodynamic_chord()``, ``.aerodynamic_center()``, ``.taper_ratio()``
-- built on geometry shaped like ``alas/geometry/aircraft_builder.py``'s
``_build_main_wing``/``_build_hstab``/``_build_vstab``, with the literal
default values ``alas.config.design_variables.DesignVector`` and
``alas.config.geometry_config.GeometryConfig`` carry (recorded next to each
number below rather than imported, since this generator exercises AeroSandbox's
``Wing`` directly and has no other reason to depend on ``alas-config``'s
Python counterpart).

Cases:

* ``main_wing``: 3 xsecs (root/break/tip), symmetric -- shaped like the main
  wing. Root and break share one ``Airfoil`` object (as
  ``_build_main_wing`` does, passing the same ``root_section`` to both), and
  the tip is a distinct section, so ``subdivide_sections`` exercises both the
  airfoil-identity short-circuit (root/break) and the blend path
  (break/tip).

* ``hstab``: 2 xsecs (root/tip), symmetric -- shaped like the horizontal
  stabiliser. Both ends share one ``Airfoil`` object.

* ``vstab``: 2 xsecs (root/tip), *not* symmetric -- shaped like the vertical
  stabiliser. Both ends share one ``Airfoil`` object.

For each wing: ``span()``, ``area()``, ``mean_aerodynamic_chord()``,
``aerodynamic_center()`` (as a 3-vector; ALAS only ever reads its ``[0]``
component afterward, but the full vector pins down more), ``taper_ratio()``.

``subdivide_sections`` runs on ``main_wing`` at ``ratio=8``
(``WingConfig.n_subdivisions``'s default), recording the resulting xsec
count and every new xsec's ``xyz_le``/``chord``/``twist``, plus the blended
airfoil's coordinate array at one interior xsec in the break/tip section
(the branch that actually blends, since root/break share an airfoil and
the branch's very first fraction always reuses the inner airfoil
unblended -- see ``alas-geom::asb::wing``'s module doc).

``translate`` is checked by shifting ``main_wing`` by its X datum
(``WingConfig.root_datum_x_m + DesignVector.wing_x_shift_m`` at the
defaults) and recording every xsec's shifted ``xyz_le``.
"""

from __future__ import annotations

import math

import _framework
import numpy as np
from aerosandbox.geometry.airfoil.airfoil import Airfoil
from aerosandbox.geometry.wing import Wing, WingXSec


def _coords_to_list(coordinates: np.ndarray) -> list[list[float]]:
    return [[float(x), float(y)] for x, y in coordinates.tolist()]


def _vec3(v: np.ndarray) -> list[float]:
    return [float(v[0]), float(v[1]), float(v[2])]


def _wing_summary(wing: Wing) -> dict:
    return {
        "span": float(wing.span()),
        "area": float(wing.area()),
        "mean_aerodynamic_chord": float(wing.mean_aerodynamic_chord()),
        "aerodynamic_center": _vec3(wing.aerodynamic_center()),
        "taper_ratio": float(wing.taper_ratio()),
    }


def _xsec_record(xsec: WingXSec) -> dict:
    return {
        "xyz_le": _vec3(np.asarray(xsec.xyz_le, dtype=float)),
        "chord": float(xsec.chord),
        "twist": float(xsec.twist),
        "airfoil_name": xsec.airfoil.name,
    }


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
                airfoil=root_section,  # Same object as the root -- see module doc.
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
                airfoil=tail_airfoil,  # Same object as the root -- see module doc.
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
                airfoil=tail_airfoil,  # Same object as the root -- see module doc.
            ),
        ],
    )


def main() -> None:
    main_wing = _build_main_wing()
    hstab = _build_hstab()
    vstab = _build_vstab()

    wings = {
        "main_wing": _wing_summary(main_wing),
        "hstab": _wing_summary(hstab),
        "vstab": _wing_summary(vstab),
    }

    # WingConfig.n_subdivisions's default.
    ratio = 8
    subdivided = main_wing.subdivide_sections(ratio)

    # The break/tip loft section starts at index `ratio` (the root/break
    # section's `ratio` new xsecs come first); its second new xsec
    # (span_fraction index 1, i.e. overall index `ratio + 1`) is the first
    # one that actually blends two distinct airfoils -- index 0 of that
    # section reuses the break airfoil unblended (a_weight == 1, see the
    # Rust module doc).
    blended_index = ratio + 1
    blended_airfoil = subdivided.xsecs[blended_index].airfoil

    subdivide_case = {
        "ratio": ratio,
        "xsec_count": len(subdivided.xsecs),
        "xsecs": [_xsec_record(xsec) for xsec in subdivided.xsecs],
        "blended_index": blended_index,
        "blended_airfoil_name": blended_airfoil.name,
        "blended_airfoil_coordinates": _coords_to_list(blended_airfoil.coordinates),
    }

    # WingConfig.root_datum_x_m + DesignVector.wing_x_shift_m at the defaults.
    x_wing_global = 26.24 + 0.00
    translated = main_wing.translate([x_wing_global, 0, 0])
    translate_case = {
        "shift": [x_wing_global, 0.0, 0.0],
        "xsecs": [_xsec_record(xsec) for xsec in translated.xsecs],
    }

    _framework.write(
        "geom",
        "asb_wing",
        {
            "wings": wings,
            "subdivide_sections": subdivide_case,
            "translate": translate_case,
        },
        description=(
            "aerosandbox.geometry.wing.Wing/WingXSec, scoped to translate, "
            "subdivide_sections, span, area, mean_aerodynamic_chord, "
            "aerodynamic_center and taper_ratio at their defaults, on "
            "geometry shaped like aircraft_builder.py's main wing, hstab "
            "and vstab"
        ),
    )


if __name__ == "__main__":
    main()
