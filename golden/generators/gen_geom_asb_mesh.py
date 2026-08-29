# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-geom::asb::mesh``: AeroSandbox's ``Wing.mesh_thin_surface`` and
``Wing.mesh_line``, the meshing this program reaches through
``VortexLatticeMethod.run()`` rather than through its own source -- see
``docs/PORTING.md``'s corrected rationale for this row.

Builds the same three wing shapes ``gen_geom_asb_wing.py`` does (this file
duplicates that generator's literal geometry rather than importing it, which
is how every generator in this directory stays a self-contained script), so a
reader can compare this row's numbers against that row's on the same
geometry.

``mesh_thin_surface`` is exercised with ``method="quad"``, a small
``chordwise_resolution`` (kept readable) and ``add_camber=True`` -- the only
values ``VortexLatticeMethod.run()`` (and therefore this program) ever
passes, per the corrected ``docs/PORTING.md`` row. ``main_wing`` is
symmetric, so its case exercises the mirroring branch; ``vstab`` is not, so
its case exercises the unmirrored path.

``mesh_line`` is recorded independently for ``main_wing`` at two
``x_nondim`` stations, one of them nonzero, so the fixture can catch a
translation that reads the wrong per-cross-section camber station (see the
Rust module doc for the upstream indexing bug this port does not
reproduce -- invisible at this call site because ``x_nondim`` is always a
scalar here, but worth pinning down independently of ``mesh_thin_surface``'s
own use of it).
"""

from __future__ import annotations

import math

import _framework
import numpy as np
from aerosandbox.geometry.airfoil.airfoil import Airfoil
from aerosandbox.geometry.wing import Wing, WingXSec
from aerosandbox.numpy import cosspace

CHORDWISE_RESOLUTION = 3


def _vec3(v: np.ndarray) -> list[float]:
    return [float(v[0]), float(v[1]), float(v[2])]


def _points_list(points: np.ndarray) -> list[list[float]]:
    return [_vec3(p) for p in points]


def _faces_list(faces: np.ndarray) -> list[list[int]]:
    return [[int(i) for i in row] for row in faces]


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


def _mesh_case(wing: Wing) -> dict:
    points, faces = wing.mesh_thin_surface(
        method="quad",
        chordwise_resolution=CHORDWISE_RESOLUTION,
        chordwise_spacing_function=cosspace,
        add_camber=True,
    )
    return {
        "chordwise_resolution": CHORDWISE_RESOLUTION,
        "points": _points_list(points),
        "faces": _faces_list(faces),
    }


def main() -> None:
    main_wing = _build_main_wing()
    vstab = _build_vstab()

    meshes = {
        "main_wing": _mesh_case(main_wing),
        "vstab": _mesh_case(vstab),
    }

    mesh_line_cases = {}
    for x_nondim in (0.0, 0.4):
        points = main_wing.mesh_line(x_nondim=x_nondim, z_nondim=0, add_camber=True)
        mesh_line_cases[str(x_nondim)] = {
            "x_nondim": float(x_nondim),
            "points": _points_list(np.stack(points, axis=0)),
        }

    _framework.write(
        "geom",
        "asb_mesh",
        {
            "mesh_thin_surface": meshes,
            "mesh_line": {
                "wing": "main_wing",
                "cases": mesh_line_cases,
            },
        },
        description=(
            "aerosandbox.geometry.wing.Wing.mesh_thin_surface (method="
            "'quad', add_camber=True) and Wing.mesh_line, on the same "
            "main_wing/vstab geometry gen_geom_asb_wing.py builds -- the "
            "meshing VortexLatticeMethod.run() performs on every wing, "
            "every VLM solve"
        ),
    )


if __name__ == "__main__":
    main()
