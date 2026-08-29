# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-screen::runner``: Multi-stage airfoil screening, 2-D proxy, 3-D refinement, and score blending."""

from __future__ import annotations

import numpy as np

import _framework

_framework.add_alas_to_path()

from alas.analysis.airfoil_screening import (  # noqa: E402
    _blend_scores,
    _cruise_condition,
    _filter_names,
    _refine_candidate_3d,
    _score_candidate,
    run_airfoil_screening,
)
from alas.config.design_variables import DesignVector  # noqa: E402
from alas.config.settings import ALASConfig  # noqa: E402
from alas.geometry.airfoils import AirfoilLibrary  # noqa: E402


def generate() -> dict:
    config = ALASConfig()
    dv = DesignVector()

    # 1. Cruise condition
    mach, reynolds, cl_target, altitude = _cruise_condition(config, dv)
    cruise_cond_fixture = {
        "mach": float(mach),
        "reynolds": float(reynolds),
        "cl_target": float(cl_target),
        "altitude_m": float(altitude),
    }

    # 2. Name filtering
    all_names = AirfoilLibrary.get_available_airfoils()
    filter_cases = [
        {"pattern": "", "expected_count": len(all_names)},
        {"pattern": "sc20714", "expected_names": _filter_names(all_names, "sc20714")},
        {"pattern": "whitcomb, rae2822", "expected_names": _filter_names(all_names, "whitcomb, rae2822")},
        {"pattern": "sc207*", "expected_names": _filter_names(all_names, "sc207*")},
    ]

    # 3. Stage 1 (2-D) scoring on test airfoils
    section_mach = float(mach * np.cos(np.radians(dv.sweep_deg)))
    test_airfoils = ["sc20714", "rae2822", "whitcomb", "naca0012", "clarky"]
    n_alpha = int(round((14.0 - (-4.0)) / 0.5)) + 1
    alphas_deg = np.linspace(-4.0, 14.0, n_alpha)

    stage1_candidates = []
    for name in test_airfoils:
        cand = _score_candidate(
            name=name,
            config=config,
            dv=dv,
            mach=section_mach,
            reynolds=reynolds,
            cl_target=cl_target,
            usable_fraction=config.mass_model.fuel_tank_usable_fraction,
            alphas_deg=alphas_deg,
            model_size="large",
        )
        stage1_candidates.append(cand)

    stage1_fixtures = []
    for cand in stage1_candidates:
        stage1_fixtures.append({
            "name": cand.name,
            "status": cand.status,
            "error": cand.error,
            "l_over_d": float(cand.l_over_d) if cand.l_over_d is not None else None,
            "cl": float(cand.cl) if cand.cl is not None else None,
            "cd": float(cand.cd) if cand.cd is not None else None,
            "alpha_deg": float(cand.alpha_deg) if cand.alpha_deg is not None else None,
            "max_thickness_frac": float(cand.max_thickness_frac) if cand.max_thickness_frac is not None else None,
            "tank_volume_m3": float(cand.tank_volume_m3) if cand.tank_volume_m3 is not None else None,
            "tank_capacity_kg": float(cand.tank_capacity_kg) if cand.tank_capacity_kg is not None else None,
            "robustness": float(cand.robustness) if cand.robustness is not None else None,
        })

    # 4. Stage 2 (3-D) refinement
    stage2_fixtures = []
    for cand in stage1_candidates:
        if cand.status == "ok":
            _refine_candidate_3d(
                cand,
                config,
                dv,
                mach,
                altitude,
                cl_target,
                min_static_margin=None,
            )
            stage2_fixtures.append({
                "name": cand.name,
                "refined": bool(cand.refined),
                "l_over_d_3d": float(cand.l_over_d_3d) if cand.l_over_d_3d is not None else None,
                "cd_3d": float(cand.cd_3d) if cand.cd_3d is not None else None,
                "alpha_3d_deg": float(cand.alpha_3d_deg) if cand.alpha_3d_deg is not None else None,
                "cm_residual_3d": float(cand.cm_residual_3d) if cand.cm_residual_3d is not None else None,
                "static_margin_3d": float(cand.static_margin_3d) if cand.static_margin_3d is not None else None,
                "refine_error": cand.refine_error,
            })

    # 5. Score blending test
    ok_candidates = [c for c in stage1_candidates if c.status == "ok"]
    _blend_scores(
        ok_candidates,
        ld_weight=0.7,
        fuel_weight=0.3,
        robustness_weight=0.1,
        key=lambda r: r.l_over_d,
        attr="score",
    )
    blended_stage1 = [
        {"name": c.name, "score": float(c.score)} for c in ok_candidates
    ]

    refined_candidates = [c for c in ok_candidates if c.refined]
    _blend_scores(
        refined_candidates,
        ld_weight=0.7,
        fuel_weight=0.3,
        robustness_weight=0.1,
        key=lambda r: r.l_over_d_3d,
        attr="score_3d",
    )
    blended_stage2 = [
        {"name": c.name, "score_3d": float(c.score_3d)} for c in refined_candidates
    ]

    # 6. Full screening sweep on focused subset
    full_result = run_airfoil_screening(
        config,
        dv,
        name_filter="sc20714, rae2822, whitcomb",
        refine_3d=True,
        verify_mses=False,
        top_n=10,
    )

    full_sweep_fixture = {
        "baseline_airfoil": full_result.baseline_airfoil,
        "cruise_mach": float(full_result.cruise_mach),
        "cruise_reynolds": float(full_result.cruise_reynolds),
        "cruise_altitude_m": float(full_result.cruise_altitude_m),
        "cl_target": float(full_result.cl_target),
        "transonic_caveat": bool(full_result.transonic_caveat),
        "refined_3d": bool(full_result.refined_3d),
        "n_total": full_result.n_total,
        "n_ok": full_result.n_ok,
        "n_error": full_result.n_error,
        "n_refined": full_result.n_refined,
        "candidates": [
            {
                "name": c.name,
                "status": c.status,
                "l_over_d": float(c.l_over_d) if c.l_over_d is not None else None,
                "score": float(c.score) if c.score is not None else None,
                "refined": bool(c.refined),
                "l_over_d_3d": float(c.l_over_d_3d) if c.l_over_d_3d is not None else None,
                "score_3d": float(c.score_3d) if c.score_3d is not None else None,
                "is_reference": bool(c.is_reference),
            }
            for c in full_result.candidates
        ],
    }

    return {
        "cruise_condition": cruise_cond_fixture,
        "filter_cases": filter_cases,
        "stage1_candidates": stage1_fixtures,
        "stage2_candidates": stage2_fixtures,
        "blended_stage1": blended_stage1,
        "blended_stage2": blended_stage2,
        "full_sweep": full_sweep_fixture,
    }


if __name__ == "__main__":
    _framework.write(
        "screen",
        "screening",
        generate(),
        description="Airfoil screening cruise condition, 2-D scoring, 3-D refinement, score blending, and full multi-stage sweep",
    )
