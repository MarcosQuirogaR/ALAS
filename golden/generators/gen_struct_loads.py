# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-struct::loads``: the shared spanwise load-integration primitive.

Covers all four functions in ``alas/physics/structural_loads.py``:

* ``load_cases`` at a handful of ``DesignRequirements`` variations, including
  ``additional_safety_factor != 1.0``, to pin the same ``n_ult_pos``/
  ``n_ult_neg`` derivation ``performance.py``'s V-n diagram uses (not yet
  ported; reproduced here from the requirements fields directly).

* ``elliptic_distributed_load`` at a few span/force/resolution combinations,
  including a point deliberately just past the tip (``y > semi_span``) to
  exercise the ``np.clip`` guard against a negative argument to ``sqrt``.

* ``cantilever_shear_moment``, chained onto three of the same elliptic cases
  (mirroring how ``structural_sizing``/``structural_analysis`` actually use
  the two together) plus one dense case (201 stations) standing in for a
  realistic FEM discretization, and one synthetic uniform load exercised in
  isolation from the ellipse.

* ``engine_point_loads_n`` across zero thrust, a symmetric wing-mounted pair,
  a centerline (y=0) engine, more than two engines, a non-default mass
  config, and the ``y > 1e-6`` near-zero threshold itself.
"""

from __future__ import annotations

import dataclasses

import numpy as np

import _framework

_framework.add_alas_to_path()

from alas.config.geometry_config import EngineConfig  # noqa: E402
from alas.config.mass_config import MassModelConfig  # noqa: E402
from alas.config.requirements import DesignRequirements  # noqa: E402
from alas.physics.structural_loads import (  # noqa: E402
    cantilever_shear_moment,
    elliptic_distributed_load,
    engine_point_loads_n,
    load_cases,
)


def _load_case_case(req: DesignRequirements, additional_safety_factor: float) -> dict:
    cases = load_cases(req, additional_safety_factor)
    return {
        "req": {
            "gravity_m_s2": req.gravity_m_s2,
            "mtow_kg": req.mtow_kg,
            "ultimate_load_factor": req.ultimate_load_factor,
            "limit_load_factor_neg": req.limit_load_factor_neg,
        },
        "additional_safety_factor": additional_safety_factor,
        "cases": [
            {"name": c.name, "load_factor": c.load_factor, "total_force_n": c.total_force_n}
            for c in cases
        ],
    }


def _elliptic_case(y: np.ndarray, semi_span: float, total_force_n: float) -> dict:
    q = elliptic_distributed_load(y, semi_span, total_force_n)
    return {
        "y": y.tolist(),
        "semi_span": semi_span,
        "total_force_n": total_force_n,
        "q": q.tolist(),
    }


def _cantilever_case(y: np.ndarray, q_net: np.ndarray) -> dict:
    v, m = cantilever_shear_moment(y, q_net)
    return {
        "y": y.tolist(),
        "q_net": q_net.tolist(),
        "v": v.tolist(),
        "m": m.tolist(),
    }


def _engine_case(engine_cfg: EngineConfig, mass_cfg: MassModelConfig, req: DesignRequirements) -> dict:
    result = engine_point_loads_n(engine_cfg, mass_cfg, req)
    return {
        "engine": {
            "thrust_kn": engine_cfg.thrust_kn,
            "spanwise_positions_m": list(engine_cfg.spanwise_positions_m),
        },
        "mass_cfg": {
            "propulsion_twr_factor": mass_cfg.propulsion_twr_factor,
            "propulsion_installation_factor": mass_cfg.propulsion_installation_factor,
        },
        "gravity_m_s2": req.gravity_m_s2,
        "result": [[y, m] for y, m in result],
    }


def main() -> None:
    default_req = DesignRequirements()

    load_case_cases = [
        _load_case_case(default_req, 1.0),
        _load_case_case(default_req, 1.5),
        _load_case_case(
            dataclasses.replace(
                default_req,
                mtow_kg=180_000.0,
                gravity_m_s2=9.80665,
                ultimate_load_factor=2.5,
                limit_load_factor_neg=-1.2,
            ),
            1.0,
        ),
        _load_case_case(
            dataclasses.replace(
                default_req,
                mtow_kg=42_500.0,
                gravity_m_s2=9.81,
                ultimate_load_factor=4.4,
                limit_load_factor_neg=-1.76,
            ),
            0.9,
        ),
    ]

    # A: coarse. B: medium, negative force. C: dense -- a realistic FEM
    # discretization, per the module's own framing. D: one station
    # deliberately past the tip, to exercise np.clip's guard on the sqrt
    # argument going negative.
    elliptic_cases = [
        _elliptic_case(np.linspace(0.0, 32.0, 5), 32.0, 550_000.0),
        _elliptic_case(np.linspace(0.0, 25.5, 21), 25.5, -180_000.0),
        _elliptic_case(np.linspace(0.0, 35.8, 201), 35.8, 900_000.0),
        _elliptic_case(
            np.array([0.0, 2.0, 5.0, 9.0, 10.0, 10.0000001]), 10.0, 100_000.0
        ),
    ]

    # Chain three of the elliptic cases straight into the cantilever
    # integration, exactly as structural_sizing/structural_analysis will:
    # the q(y) computed above becomes q_net below.
    cantilever_cases = [
        _cantilever_case(
            np.linspace(0.0, 32.0, 5),
            elliptic_distributed_load(np.linspace(0.0, 32.0, 5), 32.0, 550_000.0),
        ),
        _cantilever_case(
            np.linspace(0.0, 25.5, 21),
            elliptic_distributed_load(np.linspace(0.0, 25.5, 21), 25.5, -180_000.0),
        ),
        _cantilever_case(
            np.linspace(0.0, 35.8, 201),
            elliptic_distributed_load(np.linspace(0.0, 35.8, 201), 35.8, 900_000.0),
        ),
        # A synthetic uniform load, decoupled from the ellipse entirely, to
        # check the integration on its own known shape.
        _cantilever_case(np.linspace(0.0, 18.0, 9), np.full(9, 5_000.0)),
    ]

    default_mass_cfg = MassModelConfig()
    default_engine_req = DesignRequirements()

    engine_cases = [
        # Zero thrust: an empty result regardless of positions.
        _engine_case(
            dataclasses.replace(
                EngineConfig(), thrust_kn=0.0, spanwise_positions_m=(9.8, -9.8)
            ),
            default_mass_cfg,
            default_engine_req,
        ),
        # A symmetric wing-mounted pair: only the y > 0 station survives.
        _engine_case(
            dataclasses.replace(
                EngineConfig(), thrust_kn=350.0, spanwise_positions_m=(9.8, -9.8)
            ),
            default_mass_cfg,
            default_engine_req,
        ),
        # A lone centerline engine: skipped entirely.
        _engine_case(
            dataclasses.replace(
                EngineConfig(), thrust_kn=200.0, spanwise_positions_m=(0.0,)
            ),
            default_mass_cfg,
            default_engine_req,
        ),
        # Four wing-mounted engines (two symmetric pairs): two survive.
        _engine_case(
            dataclasses.replace(
                EngineConfig(),
                thrust_kn=300.0,
                spanwise_positions_m=(9.8, 22.5, -9.8, -22.5),
            ),
            default_mass_cfg,
            default_engine_req,
        ),
        # A non-default mass config and gravity, to exercise the full
        # per-engine dry-mass arithmetic, not only the filtering.
        _engine_case(
            dataclasses.replace(
                EngineConfig(), thrust_kn=310.0, spanwise_positions_m=(10.5, -10.5)
            ),
            dataclasses.replace(
                MassModelConfig(),
                propulsion_twr_factor=5.5,
                propulsion_installation_factor=1.15,
            ),
            dataclasses.replace(default_engine_req, gravity_m_s2=9.80665),
        ),
        # The y > 1e-6 threshold itself: one station just below it (treated
        # as centerline), one just above, one on the port side (dropped for
        # being negative, not for being near zero).
        _engine_case(
            dataclasses.replace(
                EngineConfig(),
                thrust_kn=100.0,
                spanwise_positions_m=(1e-7, 5.0, -5.0),
            ),
            default_mass_cfg,
            default_engine_req,
        ),
    ]

    _framework.write(
        "struct",
        "loads",
        {
            "load_cases": load_case_cases,
            "elliptic_distributed_load": elliptic_cases,
            "cantilever_shear_moment": cantilever_cases,
            "engine_point_loads_n": engine_cases,
        },
        description=(
            "alas.physics.structural_loads: load_cases across DesignRequirements "
            "variations including additional_safety_factor != 1.0; "
            "elliptic_distributed_load across span/force/resolution combinations "
            "including a point past the tip; cantilever_shear_moment chained onto "
            "three of those plus a dense FEM-like case and a synthetic uniform "
            "load; engine_point_loads_n across zero thrust, a symmetric pair, a "
            "centerline engine, more than two engines, a non-default mass config, "
            "and the y > 1e-6 threshold"
        ),
    )


if __name__ == "__main__":
    main()
