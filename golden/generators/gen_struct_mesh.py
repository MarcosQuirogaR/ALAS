# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-struct::mesh``: the wingbox FEM deck ``build_wing_mesh_bdf`` writes.

Runs ``alas/geometry/wing_mesh_bdf.py``'s ``build_wing_mesh_bdf`` on the same
``WingStructureGeometry``/``WingboxSizing`` pair ``pipeline.py``'s structural
stage builds -- the default ``DesignVector``/``WingConfig``, the root section
via ``build_section``, the tip via ``AirfoilLibrary.get``, and the four wingbox
materials the ``StructuresConfig`` names -- exactly as ``gen_struct_sizing.py``
already does.

**What is recorded is the deck, not the in-memory model.** Each case's
``pyNastran`` ``BDF`` is written out with ``write_bdf(size=16,
is_double=False)`` -- the same call ``nastran_runner.run_nastran_analysis``
makes -- and then read back, and it is the *re-read* cards that go into the
fixture. That is deliberate: what a NASTRAN solve consumes is the file, so the
claim worth checking is that the Rust port's file carries the same cards, not
that two object graphs happen to agree. The large-field round trip also costs
about a digit of precision, which the recorded values therefore already carry,
so the fixture states what the solver actually reads.

Cases. Four vary the ``StructuresConfig``'s mesh-shaping fields at a coarse
resolution, where a whole deck is small enough to record in full and every
mechanism still fires; the fifth is the production-resolution mesh the pipeline
actually builds, which is the only one that produces an aero-only rib:

* ``coarse``: 8 ribs, 9 chordwise points, ``te_rib_mode`` at its ``all``
  default -- the plain front/rear two-spar box.
* ``coarse_center_spar``: 12 ribs, 11 chordwise points, ``center_spar_enabled``
  -- three spars, one of them partial-span, so a spar's node list stops at the
  break and the cap/web loops run over unequal lengths.
* ``coarse_te_none`` / ``coarse_te_alternate`` / ``coarse_te_inboard`` /
  ``coarse_te_outboard`` / ``coarse_te_inboard_alternate`` /
  ``coarse_te_step_3``: the trailing-edge rib-panel selector's remaining
  branches, one case each.
* ``default``: the production mesh -- 39 ribs, 50 chordwise points.

The generator refuses to write a fixture that has stopped exercising the three
mechanisms this module's own docstring is about: the zipper CTRIA3 bridging
(``n_ctria3``), the transition-rib RBE3 rivets (``rbe3_count``), and the
aero-only rib exclusion (its warning string).
"""

from __future__ import annotations

import tempfile
from pathlib import Path

import _framework

_framework.add_alas_to_path()

from alas.config.design_variables import DesignVector  # noqa: E402
from alas.config.geometry_config import EngineConfig, WingConfig  # noqa: E402
from alas.config.mass_config import MassModelConfig  # noqa: E402
from alas.config.materials import get_material  # noqa: E402
from alas.config.requirements import DesignRequirements  # noqa: E402
from alas.config.structures_config import (  # noqa: E402
    StructuresConfig,
    resolve_spar_geometry,
)
from alas.geometry.airfoils import AirfoilLibrary, build_section  # noqa: E402
from alas.geometry.wing_mesh_bdf import build_wing_mesh_bdf  # noqa: E402
from alas.geometry.wing_structure import WingStructureGeometry  # noqa: E402
from alas.physics.structural_sizing import size_wingbox  # noqa: E402
from pyNastran.bdf.bdf import BDF  # noqa: E402

_CASES = [
    {"name": "coarse", "config": {"num_ribs_override": 8, "mesh_chordwise_points": 9}},
    {
        "name": "coarse_center_spar",
        "config": {
            "num_ribs_override": 12,
            "mesh_chordwise_points": 11,
            "center_spar_enabled": True,
        },
    },
    {
        "name": "coarse_te_none",
        "config": {
            "num_ribs_override": 10,
            "mesh_chordwise_points": 9,
            "te_rib_mode": "none",
        },
    },
    {
        "name": "coarse_te_alternate",
        "config": {
            "num_ribs_override": 10,
            "mesh_chordwise_points": 9,
            "te_rib_mode": "alternate",
        },
    },
    {
        "name": "coarse_te_inboard",
        "config": {
            "num_ribs_override": 10,
            "mesh_chordwise_points": 9,
            "te_rib_mode": "inboard",
        },
    },
    {
        "name": "coarse_te_outboard",
        "config": {
            "num_ribs_override": 10,
            "mesh_chordwise_points": 9,
            "te_rib_mode": "outboard",
        },
    },
    {
        "name": "coarse_te_inboard_alternate",
        "config": {
            "num_ribs_override": 10,
            "mesh_chordwise_points": 9,
            "te_rib_mode": "inboard_alternate",
        },
    },
    {
        "name": "coarse_te_step_3",
        "config": {
            "num_ribs_override": 10,
            "mesh_chordwise_points": 9,
            "te_rib_mode": "step_3",
        },
    },
    {"name": "default", "config": {}},
]


def _structures_config(overrides: dict) -> StructuresConfig:
    scfg = StructuresConfig()
    for key, value in overrides.items():
        setattr(scfg, key, value)
    return scfg


def _reread(model) -> BDF:
    """Write the deck the way ``run_nastran_analysis`` does, then read it back.

    ``punch=True`` on the read because ``write_bdf`` marks the file as one: the
    mesh is an INCLUDE fragment with no executive or case control of its own.
    """
    directory = Path(tempfile.mkdtemp(prefix="alas-mesh-"))
    path = directory / "wing_mesh.bdf"
    model.write_bdf(str(path), size=16, is_double=False)
    reread = BDF(debug=False)
    reread.read_bdf(str(path), punch=True)
    return reread


def _deck_record(model: BDF) -> dict:
    grids = sorted(
        [int(n.nid), *[float(v) for v in n.xyz]] for n in model.nodes.values()
    )
    mat1 = sorted(
        [int(m.mid), float(m.e), float(m.g), float(m.nu), float(m.rho)]
        for m in model.materials.values()
    )
    pshell, pbarl = [], []
    for prop in model.properties.values():
        if prop.type == "PSHELL":
            pshell.append(
                [int(prop.pid), int(prop.mid1), float(prop.t), int(prop.mid2)]
            )
        elif prop.type == "PBARL":
            pbarl.append(
                [int(prop.pid), int(prop.mid), prop.Type, [float(d) for d in prop.dim]]
            )
        else:
            raise SystemExit(f"unexpected property card {prop.type} in the mesh deck")

    cquad4, ctria3, cbar = [], [], []
    for elem in model.elements.values():
        if elem.type == "CQUAD4":
            cquad4.append([int(elem.eid), int(elem.pid), *map(int, elem.node_ids)])
        elif elem.type == "CTRIA3":
            ctria3.append([int(elem.eid), int(elem.pid), *map(int, elem.node_ids)])
        elif elem.type == "CBAR":
            if elem.g0 is not None:
                raise SystemExit("a CBAR was written with G0 rather than an X vector")
            cbar.append(
                [
                    int(elem.eid),
                    int(elem.pid),
                    *map(int, elem.node_ids),
                    [float(v) for v in elem.x],
                    str(elem.offt),
                ]
            )
        else:
            raise SystemExit(f"unexpected element card {elem.type} in the mesh deck")

    conm2 = sorted(
        [
            int(e.eid),
            int(e.nid),
            int(e.cid),
            float(e.mass),
            [float(v) for v in e.X],
        ]
        for e in model.masses.values()
    )
    rbe3 = []
    for elem in model.rigid_elements.values():
        if elem.type != "RBE3":
            raise SystemExit(f"unexpected rigid card {elem.type} in the mesh deck")
        if len(elem.weights) != 1 or len(elem.comps) != 1 or len(elem.Gijs) != 1:
            raise SystemExit("an RBE3 carried more than one weight/component group")
        rbe3.append(
            [
                int(elem.eid),
                int(elem.refgrid),
                str(elem.refc),
                float(elem.weights[0]),
                str(elem.comps[0]),
                [int(g) for g in elem.Gijs[0]],
            ]
        )

    spc1 = []
    for sid, constraints in model.spcs.items():
        for spc in constraints:
            if spc.type != "SPC1":
                raise SystemExit(f"unexpected constraint card {spc.type}")
            spc1.append([int(sid), str(spc.components), [int(n) for n in spc.nodes]])

    params = sorted(
        [str(key), [v if isinstance(v, str) else float(v) for v in param.values]]
        for key, param in model.params.items()
    )

    return {
        "params": params,
        "grids": grids,
        "mat1": mat1,
        "pshell": sorted(pshell),
        "pbarl": sorted(pbarl),
        "cquad4": sorted(cquad4),
        "ctria3": sorted(ctria3),
        "cbar": sorted(cbar),
        "conm2": conm2,
        "rbe3": sorted(rbe3),
        "spc1": sorted(spc1),
    }


def _health_record(report) -> dict:
    return {
        "n_nodes": int(report.n_nodes),
        "n_elements": int(report.n_elements),
        "n_perp_warnings": int(report.n_perp_warnings),
        "n_warping_bad": int(report.n_warping_bad),
        "warping_max": float(report.warping_max),
        "warping_mean": float(report.warping_mean),
        "n_cquad4": int(report.n_cquad4),
        "n_ctria3": int(report.n_ctria3),
        "triangle_ratio": float(report.triangle_ratio),
        "n_spar_straightness_warnings": int(report.n_spar_straightness_warnings),
        "spar_straightness_max_dev_m": [
            [float(frac), float(dev)]
            for frac, dev in report.spar_straightness_max_dev_m.items()
        ],
        "rbe3_count": int(report.rbe3_count),
        "warnings": list(report.warnings),
        "ok": bool(report.ok),
    }


def _node_index_record(node_index) -> dict:
    return {
        "root_nid": int(node_index.root_nid),
        "tip_nid": int(node_index.tip_nid),
        "kink_nid": int(node_index.kink_nid),
        "spar_upper_nids": [[int(n) for n in s] for s in node_index.spar_upper_nids],
        "spar_lower_nids": [[int(n) for n in s] for s in node_index.spar_lower_nids],
        "engine_nids": [int(n) for n in node_index.engine_nids],
    }


def main() -> None:
    dv = DesignVector()
    wing_cfg = WingConfig()
    req = DesignRequirements()
    engine_cfg = EngineConfig()
    mass_cfg = MassModelConfig()

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

        model, report, node_index = build_wing_mesh_bdf(
            wsg,
            sizing,
            scfg,
            engine_cfg,
            mass_cfg,
            req,
            skin_mat,
            web_mat,
            cap_mat,
            rib_mat,
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
                "num_ribs": int(sizing.num_ribs),
                "health": _health_record(report),
                "node_index": _node_index_record(node_index),
                "deck": _deck_record(_reread(model)),
            }
        )

    if not any(r["health"]["n_ctria3"] > 0 for r in results):
        raise SystemExit(
            "no case produced a CTRIA3; the zipper skin-bridging that this "
            "module exists for would go unexercised"
        )
    if not any(r["health"]["rbe3_count"] > 0 for r in results):
        raise SystemExit(
            "no case produced an RBE3; the transition-rib rivets would go "
            "unexercised"
        )
    if not any(
        any("aero-only" in w for w in r["health"]["warnings"]) for r in results
    ):
        raise SystemExit(
            "no case excluded an aero-only rib; that classification branch "
            "would go unexercised"
        )

    _framework.write(
        "struct",
        "mesh",
        {"cases": results},
        description=(
            "alas.geometry.wing_mesh_bdf.build_wing_mesh_bdf on the default "
            "DesignVector/WingConfig geometry: the whole NASTRAN deck as "
            "write_bdf(size=16) wrote it and pyNastran read it back (grids, "
            "shells, bars, masses, RBE3 rivets, SPC1, materials, properties, "
            "params), the mesh health report and the load/monitor node index, "
            "across eight coarse StructuresConfig cases and the production mesh"
        ),
    )


if __name__ == "__main__":
    main()
