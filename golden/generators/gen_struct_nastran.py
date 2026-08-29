# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-struct::nastran``: the solution decks the runner writes.

Runs ``alas/integration/nastran_runner.py``'s four bulk-data builders --
``build_sol101_bulk`` (linear static), ``build_sol103_bulk`` (normal modes),
``build_sol111_sine_bulk`` and ``build_sol111_random_bulk`` (modal frequency
response) -- on the mesh ``gen_struct_mesh.py`` already records, and captures
each deck verbatim.

These decks are what a NASTRAN run consumes, and unlike the mesh they are
written by hand rather than by ``pyNastran``: every line is a free-field card
this module formats itself. So the fixture holds the text, line for line, and
the parity test compares text. The one exception is the ``FORCE`` cards, whose
magnitudes come from distributing each load case's total over the front spar's
own node line -- arithmetic over mesh coordinates, which is why the parity test
splits them out and compares the number rather than its rendering.

Also recorded, because it is the module's own primitive and every card in every
deck goes through it: ``_f``, the free-field float formatter, over a table of
values chosen to reach each of its branches -- the zero shortcut, the
integer-valued case that has to gain a decimal point (a bare ``500`` is read as
an integer and triggers USER FATAL 9994), the ordinary case, and the two that
``%.8g`` renders with an exponent and the formatter re-renders with ``%.6f``.

Cases vary the ``StructuresConfig`` fields the decks read -- the mode count, the
sweep and damping parameters, the excitation PSD, and the extra safety factor
that scales the load cases -- at the coarse mesh resolution, since the decks
depend on the mesh only through the front spar's node line and the four monitor
nodes.
"""

from __future__ import annotations

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
from alas.integration.nastran_runner import (  # noqa: E402
    _f,
    _monitor_set,
    _node_y,
    build_sol101_bulk,
    build_sol103_bulk,
    build_sol111_random_bulk,
    build_sol111_sine_bulk,
)
from alas.physics import structural_loads as loads  # noqa: E402
from alas.physics.structural_sizing import size_wingbox  # noqa: E402

MESH_INCLUDE = "../wing_mesh.bdf"

# Values reaching every branch of `_f`: zero, integer-valued (needs the point
# appended), ordinary, negative, one that `%.8g` renders with a positive
# exponent, and two it renders with a negative one.
_FORMAT_SAMPLES = [
    0.0,
    -0.0,
    500.0,
    1.0,
    -1.0,
    9.81,
    0.02,
    0.05,
    1.5,
    39.24,
    2000.0,
    123456.789,
    1e8,
    3.6e8,
    -2.5e9,
    1e-5,
    4.8e-5,
    1.2345678e-7,
    0.000123456789,
    98765432.1,
    1234567.89,
]

_CASES = [
    {"name": "defaults", "config": {}},
    {
        "name": "safety_factor",
        "config": {"additional_safety_factor": 1.25},
    },
    {
        "name": "dynamics",
        "config": {
            "n_modes": 12,
            "freq_sweep_max_hz": 120.0,
            "freq_step_hz": 0.5,
            "modal_damping_ratio": 0.03,
            "psd_base_g2_per_hz": 0.008,
        },
    },
    {
        "name": "coarse_step",
        "config": {
            "n_modes": 5,
            "freq_sweep_max_hz": 55.0,
            "freq_step_hz": 2.0,
            "modal_damping_ratio": 0.1,
            "psd_base_g2_per_hz": 0.02,
        },
    },
]

# The mesh every case is built on: coarse enough that the recorded decks stay
# short, since what varies between cases is the configuration and not the mesh.
_MESH_CONFIG = {"num_ribs_override": 8, "mesh_chordwise_points": 9}


def _structures_config(overrides: dict) -> StructuresConfig:
    scfg = StructuresConfig()
    for key, value in {**_MESH_CONFIG, **overrides}.items():
        setattr(scfg, key, value)
    return scfg


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
        mats = [
            get_material(scfg.skin_material),
            get_material(scfg.spar_web_material),
            get_material(scfg.spar_cap_material),
            get_material(scfg.rib_material),
        ]
        sizing = size_wingbox(wsg, scfg, req, *mats)
        model, _, node_index = build_wing_mesh_bdf(
            wsg, sizing, scfg, engine_cfg, mass_cfg, req, *mats
        )

        front_upper = node_index.spar_upper_nids[0]
        nid_y = [(int(n), _node_y(model, n)) for n in front_upper]
        semi_span = _node_y(model, node_index.tip_nid)
        case_loads = loads.load_cases(req, scfg.additional_safety_factor)

        results.append(
            {
                "name": case["name"],
                # The merged configuration, mesh fields included, so the parity
                # test rebuilds exactly this case rather than re-deriving which
                # mesh the decks were written over.
                "config": {**_MESH_CONFIG, **case["config"]},
                "spar_chord_fractions": [float(x) for x in spar_fracs],
                "spar_full_span": [bool(x) for x in spar_full_span],
                "materials": {
                    "skin": scfg.skin_material,
                    "web": scfg.spar_web_material,
                    "cap": scfg.spar_cap_material,
                    "rib": scfg.rib_material,
                },
                "monitors": {k: int(v) for k, v in _monitor_set(node_index).items()},
                "front_spar_nid_y": [[nid, float(y)] for nid, y in nid_y],
                "semi_span_m": float(semi_span),
                "load_cases": [
                    {
                        "name": c.name,
                        "load_factor": float(c.load_factor),
                        "total_force_n": float(c.total_force_n),
                    }
                    for c in case_loads
                ],
                "decks": {
                    "sol101": build_sol101_bulk(
                        model, node_index, req, scfg, MESH_INCLUDE
                    ).splitlines(),
                    "sol103": build_sol103_bulk(scfg, MESH_INCLUDE).splitlines(),
                    "sol111_sine": build_sol111_sine_bulk(
                        scfg, node_index, MESH_INCLUDE
                    ).splitlines(),
                    "sol111_random": build_sol111_random_bulk(
                        scfg, node_index, MESH_INCLUDE
                    ).splitlines(),
                },
            }
        )

    formatted = [[float(v), _f(v)] for v in _FORMAT_SAMPLES]
    if not any(text.endswith(".0") for _, text in formatted):
        raise SystemExit(
            "no sample reached _f's exponent branch, which re-renders through "
            "%.6f and is the one place its two format strings disagree"
        )
    if not any(text.endswith(".") and text != "0." for _, text in formatted):
        raise SystemExit(
            "no sample reached _f's integer-valued branch, the one that stops "
            "NASTRAN reading a whole number as an integer"
        )

    _framework.write(
        "struct",
        "nastran",
        {"cases": results, "format_samples": formatted},
        description=(
            "alas.integration.nastran_runner's four bulk-data builders "
            "(SOL 101 static, SOL 103 normal modes, SOL 111 sine and random) "
            "verbatim on the coarse wingbox mesh across StructuresConfig "
            "cases, with the monitor node set, the front-spar node line, the "
            "elliptic FORCE distribution per load case, and the free-field "
            "float formatter every card goes through"
        ),
    )


if __name__ == "__main__":
    main()
