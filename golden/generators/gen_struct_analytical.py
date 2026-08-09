# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-struct::analytical``: the no-NASTRAN deflection/stress/frequency solve.

Runs ``alas/physics/structural_analysis.py``'s ``analyze_structure`` on the
sized wingbox ``size_wingbox`` produces for the same ``WingStructureGeometry``
``pipeline.py``'s structural stage builds -- the default ``DesignVector``/
``WingConfig`` geometry, the default ``EngineConfig`` (whose GE9X fallback
thrust is non-zero, so the engine-relief and modal engine-mass branches run),
and the four wingbox materials the ``StructuresConfig`` names.

Cases (varying only the ``StructuresConfig`` so the spread isolates the
solver's own branches; geometry, requirements, engine and materials stay at
their defaults):

* ``default``: the two-spar box.
* ``center_spar``: the optional partial-span centre spar, so the per-spar EI
  sum, mean-height and stress loops run over three spars, one of them zeroed
  outboard of the break.

The Rust parity test recomputes ``size_wingbox`` itself (it is ``green``), so
the fixture records only ``analyze_structure``'s report. Each ``+inf`` margin
of safety (demand below 1 N.m, near the tip) is written as JSON ``null`` -- an
explicit +inf sentinel the test maps back rather than a finite stand-in.
"""

from __future__ import annotations

import math

import _framework

_framework.add_alas_to_path()

from alas.config.design_variables import DesignVector  # noqa: E402
from alas.config.geometry_config import GeometryConfig, WingConfig  # noqa: E402
from alas.config.materials import get_material  # noqa: E402
from alas.config.mass_config import MassModelConfig  # noqa: E402
from alas.config.requirements import DesignRequirements  # noqa: E402
from alas.config.structures_config import (  # noqa: E402
    StructuresConfig,
    resolve_spar_geometry,
)
from alas.geometry.airfoils import AirfoilLibrary, build_section  # noqa: E402
from alas.geometry.wing_structure import WingStructureGeometry  # noqa: E402
from alas.physics.structural_analysis import analyze_structure  # noqa: E402
from alas.physics.structural_sizing import size_wingbox  # noqa: E402


def _structures_config(overrides: dict) -> StructuresConfig:
    scfg = StructuresConfig()
    for key, value in overrides.items():
        setattr(scfg, key, value)
    return scfg


def _f(seq) -> list:
    return [float(x) for x in seq]


def _ms_list(ms) -> list:
    out = []
    for v in ms:
        fv = float(v)
        if math.isnan(fv):
            raise SystemExit("margin_of_safety produced a NaN, which the port does not expect")
        out.append(None if math.isinf(fv) else fv)
    return out


def _spar_stress_record(s) -> dict:
    return {
        "chord_fraction": float(s.chord_fraction),
        "stress_pa": _f(s.stress_pa),
        "margin_of_safety": _ms_list(s.margin_of_safety),
    }


def _load_case_record(lc) -> dict:
    return {
        "name": lc.name,
        "load_factor": float(lc.load_factor),
        "y": _f(lc.y),
        "q_net": _f(lc.q_net),
        "shear_n": _f(lc.shear_n),
        "moment_nm": _f(lc.moment_nm),
        "deflection_m": _f(lc.deflection_m),
        "tip_deflection_m": float(lc.tip_deflection_m),
        "spar_stress": [_spar_stress_record(s) for s in lc.spar_stress],
    }


def _report_record(report) -> dict:
    return {
        "y": _f(report.y),
        "ei_nm2": _f(report.ei_nm2),
        "load_cases": [_load_case_record(lc) for lc in report.load_cases.values()],
        "modal": {
            "frequencies_hz": _f(report.modal.frequencies_hz),
            "mode_shapes": [_f(shape) for shape in report.modal.mode_shapes],
        },
    }


_CASES = [
    {"name": "default", "config": {}},
    {"name": "center_spar", "config": {"center_spar_enabled": True}},
]


def main() -> None:
    dv = DesignVector()
    wing_cfg = WingConfig()
    req = DesignRequirements()
    engine_cfg = GeometryConfig().engine
    mass_cfg = MassModelConfig()

    root_base = AirfoilLibrary.get(wing_cfg.root_airfoil)
    if root_base is None:
        raise SystemExit(f"AirfoilLibrary.get({wing_cfg.root_airfoil!r}) did not resolve")
    root_section = build_section(dv, root_base.coordinates)
    tip_airfoil = AirfoilLibrary.get(wing_cfg.tip_airfoil)
    if tip_airfoil is None:
        raise SystemExit(f"AirfoilLibrary.get({wing_cfg.tip_airfoil!r}) did not resolve")

    if engine_cfg.thrust_kn <= 0.0:
        raise SystemExit(
            "the default EngineConfig has zero thrust, so the engine-relief "
            "branch this fixture is meant to exercise would not run"
        )

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
        report = analyze_structure(
            wsg, sizing, scfg, req, engine_cfg, mass_cfg, skin_mat, web_mat, cap_mat
        )

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
                "report": _report_record(report),
            }
        )

    _framework.write(
        "struct",
        "analytical",
        {"cases": results},
        description=(
            "alas.physics.structural_analysis.analyze_structure on the sized "
            "wingbox for the default DesignVector/WingConfig geometry, default "
            "engine relief, across two-spar and partial-span-centre-spar "
            "StructuresConfig cases -- EI(y), per-load-case shear/moment/"
            "deflection/tip-deflection/spar stress, and the Rayleigh modal "
            "frequencies and shapes"
        ),
    )


if __name__ == "__main__":
    main()
