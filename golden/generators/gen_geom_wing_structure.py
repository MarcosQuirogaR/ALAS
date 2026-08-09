# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-geom::wing_structure``: the generic rib/spar wingbox geometry
generator.

Builds the exact objects ``pipeline.py``'s structural-analysis stage does
(``pipeline.py:759-777``): the default ``DesignVector``, the default
``WingConfig``, the root section via ``build_section`` on the configured
root airfoil, the tip section via ``AirfoilLibrary.get`` on the configured
tip airfoil, and the spar list via ``resolve_spar_geometry`` on the default
``StructuresConfig`` (a plain front/rear 2-spar box; ``center_spar_enabled``
is off by default, so this fixture does not exercise a partial-span spar --
that path is instead covered by a Rust-side unit test built on synthetic
geometry, since no default-config input reaches it).

``num_ribs`` for the end-to-end ``get_rib_stations`` case is not a config
default -- ``structural_sizing.size_wingbox`` derives it from a panel-
buckling rib-spacing criterion, so this generator runs that real sizing pass
(with the default ``DesignRequirements`` and the four wingbox materials
``StructuresConfig`` names by default) purely to read off the ``num_ribs``
it lands on, the same number ``wing_mesh_bdf.py`` would actually mesh with
for this aircraft. ``num_pts_chord`` is ``StructuresConfig.
mesh_chordwise_points``'s default (50), which also happens to be
``get_rib_stations``'s own Python default.

Cases:

* ``planform``: ``local_chord``/``x_le``/``z_le``/``rib_vector``/
  ``le_direction`` at five spanwise stations -- root, inboard of the break,
  exactly at the break, outboard of the break, and the tip.

* ``airfoil_zu_zl``: upper/lower surface height at a handful of (eta, x/c)
  pairs, split between an inboard station (root section verbatim) and an
  outboard one (blended toward the tip section).

* ``rib_lengths_and_spars``: ``get_rib_lengths`` and
  ``compute_spar_intersections`` at four stations along the real rib
  spacing this aircraft's sizing produces -- the root, the first rib
  outboard of it (where the root-plane truncation the module's docstring
  describes actually triggers, confirmed below rather than assumed), a
  mid-span station, and a near-tip station.

* ``rib_stations``: ``get_rib_stations`` end to end at the real
  ``num_ribs``/``num_pts_chord``, recording every station's full geometry.
"""

from __future__ import annotations

import _framework

_framework.add_alas_to_path()

from alas.config.design_variables import DesignVector  # noqa: E402
from alas.config.geometry_config import WingConfig  # noqa: E402
from alas.config.materials import get_material  # noqa: E402
from alas.config.requirements import DesignRequirements  # noqa: E402
from alas.config.structures_config import (  # noqa: E402
    StructuresConfig,
    resolve_spar_geometry,
)
from alas.geometry.airfoils import AirfoilLibrary, build_section  # noqa: E402
from alas.geometry.wing_structure import WingStructureGeometry  # noqa: E402
from alas.physics.structural_sizing import size_wingbox  # noqa: E402


def _rib_station_record(station) -> dict:
    return {
        "index": station.index,
        "eta": station.eta,
        "y_station": station.y_station,
        "is_full": station.is_full,
        "frac_actual": station.frac_actual,
        "extrados": station.extrados.tolist(),
        "intrados": station.intrados.tolist(),
        "j_spars": list(station.j_spars),
        "rib_dir_xy": list(station.rib_dir_xy),
    }


def _rib_lengths_and_spars_case(wsg: WingStructureGeometry, y: float) -> dict:
    eta = y / wsg.semi_span
    x_le_val = wsg.x_le(eta)
    aft_x, aft_y = wsg.rib_vector(eta)
    l_nominal, l_actual = wsg.get_rib_lengths(y, x_le_val, aft_x, aft_y)
    spar_intersections = wsg.compute_spar_intersections(
        y, x_le_val, aft_x, aft_y, l_nominal
    )
    return {
        "y_le_val": y,
        "eta": eta,
        "x_le_val": x_le_val,
        "aft_x": aft_x,
        "aft_y": aft_y,
        "l_nominal": l_nominal,
        "l_actual": l_actual,
        "truncated": l_actual < l_nominal - 1e-9,
        "spar_intersections": list(spar_intersections),
    }


def main() -> None:
    dv = DesignVector()
    wing_cfg = WingConfig()
    scfg = StructuresConfig()
    req = DesignRequirements()

    root_base = AirfoilLibrary.get(wing_cfg.root_airfoil)
    if root_base is None:
        raise SystemExit(f"AirfoilLibrary.get({wing_cfg.root_airfoil!r}) did not resolve")
    root_section = build_section(dv, root_base.coordinates)
    tip_airfoil = AirfoilLibrary.get(wing_cfg.tip_airfoil)
    if tip_airfoil is None:
        raise SystemExit(f"AirfoilLibrary.get({wing_cfg.tip_airfoil!r}) did not resolve")

    spar_fracs, spar_full_span = resolve_spar_geometry(scfg)

    wsg = WingStructureGeometry(
        dv, wing_cfg, root_section, tip_airfoil, spar_fracs, spar_full_span
    )

    skin_mat = get_material(scfg.skin_material)
    web_mat = get_material(scfg.spar_web_material)
    cap_mat = get_material(scfg.spar_cap_material)
    rib_mat = get_material(scfg.rib_material)
    sizing = size_wingbox(wsg, scfg, req, skin_mat, web_mat, cap_mat, rib_mat)
    num_ribs = sizing.num_ribs
    num_pts_chord = scfg.mesh_chordwise_points

    planform_etas = [0.0, 0.15, wsg.break_eta, 0.7, 1.0]
    planform_cases = []
    for eta in planform_etas:
        rib_vec = wsg.rib_vector(eta)
        le_dir = wsg.le_direction(eta)
        planform_cases.append(
            {
                "eta": eta,
                "local_chord": wsg.local_chord(eta),
                "x_le": wsg.x_le(eta),
                "z_le": wsg.z_le(eta),
                "rib_vector": list(rib_vec),
                "le_direction": list(le_dir),
            }
        )

    airfoil_stations = [
        ("inboard", 0.10, [0.05, 0.25, 0.5, 0.75, 0.95]),
        ("outboard", 0.70, [0.05, 0.25, 0.5, 0.75, 0.95]),
        ("tip", 1.0, [0.5]),
    ]
    airfoil_cases = []
    for label, eta, xc_fracs in airfoil_stations:
        for xc in xc_fracs:
            zu, zl = wsg.airfoil_zu_zl(eta, xc)
            airfoil_cases.append(
                {"label": label, "eta": eta, "xc_frac": xc, "zu": zu, "zl": zl}
            )

    # Real rib y-stations for this aircraft/sizing: the root, the first rib
    # outboard of it (where root-plane truncation is expected to trigger --
    # see the module docstring), a mid-span rib, and a near-tip rib.
    y_stations = [
        0.0,
        wsg.semi_span / max(num_ribs - 1, 1),
        0.5 * wsg.semi_span,
        wsg.semi_span * (num_ribs - 2) / max(num_ribs - 1, 1),
    ]
    rib_lengths_cases = [_rib_lengths_and_spars_case(wsg, y) for y in y_stations]
    if not any(case["truncated"] for case in rib_lengths_cases):
        raise SystemExit(
            "expected at least one sampled station to trigger root-plane "
            "rib-length truncation for the default configuration"
        )

    rib_stations = wsg.get_rib_stations(num_ribs, num_pts_chord)

    _framework.write(
        "geom",
        "wing_structure",
        {
            "config": {
                "spar_chord_fractions": list(spar_fracs),
                "spar_full_span": list(spar_full_span),
                "root_airfoil": wing_cfg.root_airfoil,
                "tip_airfoil": wing_cfg.tip_airfoil,
                "num_ribs": num_ribs,
                "num_pts_chord": num_pts_chord,
            },
            "derived": {
                "semi_span": wsg.semi_span,
                "break_eta": wsg.break_eta,
                "y_break": wsg.y_break,
                "sweep_in": wsg.sweep_in,
                "sweep_out": wsg.sweep_out,
                "dx_break": wsg.dx_break,
                "dx_tip": wsg.dx_tip,
                "spar_fracs_sorted": list(wsg.spar_fracs),
                "spar_full_span_sorted": list(wsg.spar_full_span),
            },
            "planform": planform_cases,
            "airfoil_zu_zl": airfoil_cases,
            "rib_lengths_and_spars": rib_lengths_cases,
            "rib_stations": [_rib_station_record(s) for s in rib_stations],
        },
        description=(
            "alas.geometry.wing_structure.WingStructureGeometry on the "
            "default DesignVector/WingConfig/StructuresConfig, root section "
            "built via build_section and tip via AirfoilLibrary.get exactly "
            "as pipeline.py's structural-analysis stage does: planform "
            "(local_chord/x_le/z_le/rib_vector/le_direction), "
            "airfoil_zu_zl, get_rib_lengths/compute_spar_intersections, and "
            "get_rib_stations end to end at the real sizing-derived num_ribs"
        ),
    )


if __name__ == "__main__":
    main()
