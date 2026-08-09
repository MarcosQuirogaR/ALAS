# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-struct::sizing``: direct strength-based wingbox sizing, end to end.

Runs ``alas/physics/structural_sizing.py``'s ``size_wingbox`` on the same
``WingStructureGeometry`` ``pipeline.py``'s structural stage builds -- the
default ``DesignVector``/``WingConfig``, the root section via ``build_section``
and the tip via ``AirfoilLibrary.get`` -- with the four wingbox materials the
``StructuresConfig`` names, exactly as ``gen_geom_wing_structure.py`` already
does to read off ``num_ribs``.

Cases, each varying only the ``StructuresConfig`` (the geometry, requirements
and materials are held at their defaults so the spread isolates the sizing's
own branches):

* ``default``: the plain front/rear two-spar box -- every spar full-span, no
  rib override.
* ``center_spar``: ``center_spar_enabled`` adds the optional partial-span
  centre spar, so ``spar_full_span`` carries a False entry and the module's
  outboard-of-break height-zeroing branch runs.
* ``ribs_override``: ``num_ribs_override`` set, exercising the branch that
  takes the configured rib count instead of the panel-buckling estimate.

``resolve_spar_geometry`` (not yet ported) resolves each case's spar list; the
resolved ``(spar_chord_fractions, spar_full_span)`` is recorded so the Rust
parity test builds the identical ``WingStructureGeometry``, the same seam
``gen_geom_wing_structure.py`` uses. Each spar's ``margin_of_safety`` is +inf
wherever the local bending demand is below 1 N.m (near the tip); those entries
are written as JSON ``null`` -- an explicit +inf sentinel the parity test maps
back rather than a finite number that would misrepresent the value.
"""

from __future__ import annotations

import math

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


def _structures_config(overrides: dict) -> StructuresConfig:
    scfg = StructuresConfig()
    for key, value in overrides.items():
        setattr(scfg, key, value)
    return scfg


def _ms_list(ms) -> list:
    out = []
    for v in ms:
        fv = float(v)
        if math.isnan(fv):
            raise SystemExit("margin_of_safety produced a NaN, which the port does not expect")
        out.append(None if math.isinf(fv) else fv)
    return out


def _spar_record(spar) -> dict:
    return {
        "chord_fraction": float(spar.chord_fraction),
        "h": [float(x) for x in spar.h],
        "w_cap": [float(x) for x in spar.w_cap],
        "t_cap": [float(x) for x in spar.t_cap],
        "a_cap": [float(x) for x in spar.a_cap],
        "t_web": float(spar.t_web),
        "frac_moment": [float(x) for x in spar.frac_moment],
        "margin_of_safety": _ms_list(spar.margin_of_safety),
    }


def _sizing_record(sizing) -> dict:
    return {
        "y_stations": [float(x) for x in sizing.y_stations],
        "eta_stations": [float(x) for x in sizing.eta_stations],
        "chord": [float(x) for x in sizing.chord],
        "spar_fracs": [float(x) for x in sizing.spar_fracs],
        "spars": [_spar_record(s) for s in sizing.spars],
        "t_skin": float(sizing.t_skin),
        "num_ribs": int(sizing.num_ribs),
        "rib_spacing_m": float(sizing.rib_spacing_m),
        "mass_breakdown_kg": {k: float(v) for k, v in sizing.mass_breakdown_kg.items()},
        "total_mass_kg": float(sizing.total_mass_kg),
        "sizing_load_case": sizing.sizing_load_case,
    }


_CASES = [
    {"name": "default", "config": {}},
    {"name": "center_spar", "config": {"center_spar_enabled": True}},
    {"name": "ribs_override", "config": {"num_ribs_override": 25}},
]


def main() -> None:
    dv = DesignVector()
    wing_cfg = WingConfig()
    req = DesignRequirements()

    root_base = AirfoilLibrary.get(wing_cfg.root_airfoil)
    if root_base is None:
        raise SystemExit(f"AirfoilLibrary.get({wing_cfg.root_airfoil!r}) did not resolve")
    root_section = build_section(dv, root_base.coordinates)
    tip_airfoil = AirfoilLibrary.get(wing_cfg.tip_airfoil)
    if tip_airfoil is None:
        raise SystemExit(f"AirfoilLibrary.get({wing_cfg.tip_airfoil!r}) did not resolve")

    results = []
    for case in _CASES:
        scfg = _structures_config(case["config"])
        spar_fracs, spar_full_span = resolve_spar_geometry(scfg)
        wsg = WingStructureGeometry(
            dv, wing_cfg, root_section, tip_airfoil, spar_fracs, spar_full_span
        )
        skin_mat = get_material(scfg.skin_material)
        web_mat = get_material(scfg.spar_web_material)
        cap_mat = get_material(scfg.spar_cap_material)
        rib_mat = get_material(scfg.rib_material)
        sizing = size_wingbox(wsg, scfg, req, skin_mat, web_mat, cap_mat, rib_mat)

        results.append(
            {
                "name": case["name"],
                "config": case["config"],
                "spar_chord_fractions": [float(x) for x in spar_fracs],
                "spar_full_span": [bool(x) for x in spar_full_span],
                "materials": {
                    "skin": scfg.skin_material,
                    "web": scfg.spar_web_material,
                    "cap": scfg.spar_cap_material,
                    "rib": scfg.rib_material,
                },
                "sizing": _sizing_record(sizing),
            }
        )

    if not any(False in r["spar_full_span"] for r in results):
        raise SystemExit(
            "no case produced a partial-span spar; the center-spar zeroing "
            "branch would go unexercised"
        )

    _framework.write(
        "struct",
        "sizing",
        {"cases": results},
        description=(
            "alas.physics.structural_sizing.size_wingbox on the default "
            "DesignVector/WingConfig geometry across StructuresConfig cases "
            "(two-spar default, partial-span centre spar, rib-count override) "
            "-- full WingboxSizing: per-station chord/spar caps/webs/margins, "
            "rib spacing, mass breakdown and the sizing load case"
        ),
    )


if __name__ == "__main__":
    main()
