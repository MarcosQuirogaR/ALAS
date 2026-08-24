# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""Collect W6.1 landing-gear evidence from the real reference pipeline.

This is an evidence generator, not a physics or renderer change.  It runs the
same default full-analysis report used by the reference figure path, records
the effective configuration and every scalar entering the landing-gear sizing
call, then captures the figure's wheel/axis contract in both themes.  The
fixture is deliberately written without the family manifest: W6.1 evidence
is integration-owned and must not change shared registry or ledger files.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import _framework

_framework.add_alas_to_path()

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402

from alas.analysis.full_analysis import FullAnalysis  # noqa: E402
from alas.config.design_variables import DesignVector  # noqa: E402
from alas.config.settings import ALASConfig  # noqa: E402
from alas.physics import landing_gear as landing_gear_module  # noqa: E402
from alas.physics.mass import OEW_KEYS  # noqa: E402
from alas.reporting import visualization as visualization_module  # noqa: E402


ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "golden" / "report" / "w61_mlg_count.json"
IMAGE_DIR = ROOT / "reference_figures" / "w61"


def _float(value: Any) -> float:
    return float(value)


def _wheel_contract(wheel: Any) -> dict[str, Any]:
    return {
        "x": _float(wheel.x),
        "y": _float(wheel.y),
        "group": wheel.group,
        "strut_label": wheel.strut_label,
        "diameter_m": _float(wheel.diameter_m),
        "width_m": _float(wheel.width_m),
    }


def _gear_contract(gear: Any) -> dict[str, Any]:
    return {
        "n_nlg_wheels": int(gear.n_nlg_wheels),
        "n_mlg_struts": int(gear.n_mlg_struts),
        "wheels_per_mlg_strut": int(gear.wheels_per_mlg_strut),
        "nlg_tire": {
            "code": gear.nlg_tire.code,
            "name": gear.nlg_tire.name,
            "rated_load_kg": _float(gear.nlg_tire.rated_load_kg),
            "diameter_m": _float(gear.nlg_tire.diameter_m),
            "width_m": _float(gear.nlg_tire.width_m),
        },
        "mlg_tire": {
            "code": gear.mlg_tire.code,
            "name": gear.mlg_tire.name,
            "rated_load_kg": _float(gear.mlg_tire.rated_load_kg),
            "diameter_m": _float(gear.mlg_tire.diameter_m),
            "width_m": _float(gear.mlg_tire.width_m),
        },
        "strut_material": gear.strut_material,
        "x_nlg": _float(gear.x_nlg),
        "x_mlg": _float(gear.x_mlg),
        "track_width_m": _float(gear.track_width_m),
        "wheelbase_m": _float(gear.wheelbase_m),
        "r_nlg_design_kg": _float(gear.r_nlg_design_kg),
        "r_mlg_total_design_kg": _float(gear.r_mlg_total_design_kg),
        "pct_load_nlg_max": _float(gear.pct_load_nlg_max),
        "pct_load_mlg_max": _float(gear.pct_load_mlg_max),
        "turnover_angle_deg": _float(gear.turnover_angle_deg),
        "turnover_ok": bool(gear.turnover_ok),
        "wheels": [_wheel_contract(wheel) for wheel in gear.wheels],
    }


def _figure_contract(figure: Any, image_path: Path, theme: str) -> dict[str, Any]:
    figure.canvas.draw()
    axis = figure.axes[0]
    return {
        "theme": theme,
        "image": str(image_path.relative_to(ROOT)).replace("\\", "/"),
        "size_inches": [_float(value) for value in figure.get_size_inches()],
        "dpi": _float(figure.dpi),
        "panel_count": len(figure.axes),
        "suptitle": figure._suptitle.get_text() if figure._suptitle else "",
        "axes": {
            "xlabel": axis.get_xlabel(),
            "ylabel": axis.get_ylabel(),
            "title": axis.get_title(),
            "aspect": str(axis.get_aspect()),
            "xlim": [_float(value) for value in axis.get_xlim()],
            "ylim": [_float(value) for value in axis.get_ylim()],
            "xticks": [_float(value) for value in axis.get_xticks()],
            "yticks": [_float(value) for value in axis.get_yticks()],
            "legend": [
                text.get_text()
                for text in axis.get_legend().get_texts()
            ]
            if axis.get_legend() is not None
            else [],
            "patches": [
                {
                    "kind": type(patch).__name__,
                    "label": patch.get_label(),
                    "center": [_float(value) for value in patch.center]
                    if hasattr(patch, "center")
                    else None,
                    "width": _float(patch.width)
                    if hasattr(patch, "width")
                    else None,
                    "height": _float(patch.height)
                    if hasattr(patch, "height")
                    else None,
                    "facecolor": [
                        _float(value) for value in patch.get_facecolor()
                    ],
                }
                for patch in axis.patches
            ],
        },
    }


def _renderer_inputs(config: ALASConfig, report: Any) -> dict[str, Any]:
    plane = report.airplane
    mac = plane.c_ref
    wing = next(
        wing for wing in plane.wings if wing.name == "Main Wing"
    )
    x_wing_ac = _float(wing.aerodynamic_center()[0])
    x_mac_le = x_wing_ac - 0.25 * mac
    fuselage = plane.fuselages[0]
    fus_start_x = _float(fuselage.xsecs[0].xyz_c[0])
    fus_end_x = _float(fuselage.xsecs[-1].xyz_c[0])
    fuselage_diameter_m = _float(config.geometry.fuselage.diameter_m) or max(
        _float(section.width) for section in fuselage.xsecs
    )
    x_nlg = fus_start_x + (fus_end_x - fus_start_x) * config.mass_model.nlg_x_fraction
    x_mlg = x_mac_le + config.mass_model.mlg_x_fraction_mac * mac
    sm = (
        _float(report.static_margin)
        if report.static_margin == report.static_margin
        else 0.10
    )
    x_np = _float(plane.xyz_ref[0]) + sm * mac
    np_pct = (x_np - x_mac_le) / max(mac, 0.001) * 100.0
    aft_limit_mac = np_pct - config.requirements.target_static_margin * 100.0
    fwd_limit_mac = aft_limit_mac - config.requirements.cg_range_pct_mac
    aero_fwd_lim_x = x_mac_le + fwd_limit_mac / 100.0 * mac
    aero_aft_lim_x = x_mac_le + aft_limit_mac / 100.0 * mac
    masses = {
        key: _float(report.component_masses.get(key, 0.0)) for key in OEW_KEYS
    }
    payload_kg = _float(report.component_masses.get("Payload", 0.0))
    fuel_kg = _float(report.component_masses.get("Fuel", 0.0))
    mtow_kg = sum(masses.values()) + payload_kg + max(0.0, fuel_kg)
    return {
        "airplane": {
            "name": plane.name,
            "xyz_ref": [_float(value) for value in plane.xyz_ref],
            "c_ref_m": _float(mac),
            "x_wing_ac_m": x_wing_ac,
            "x_mac_le_m": x_mac_le,
            "fuselage_start_x_m": fus_start_x,
            "fuselage_end_x_m": fus_end_x,
            "fuselage_diameter_m": fuselage_diameter_m,
        },
        "masses_kg": {
            "oew_components": masses,
            "payload": payload_kg,
            "fuel": fuel_kg,
            "mtow": mtow_kg,
        },
        "cg": {
            "physical_cg_m": [_float(value) for value in report.physical_cg],
            "static_margin": sm,
            "x_neutral_point_m": _float(report.x_neutral_point),
            "x_np_used_m": x_np,
            "np_pct_mac": np_pct,
            "target_static_margin": _float(
                config.requirements.target_static_margin
            ),
            "cg_range_pct_mac": _float(config.requirements.cg_range_pct_mac),
            "aero_fwd_lim_x_m": aero_fwd_lim_x,
            "aero_aft_lim_x_m": aero_aft_lim_x,
        },
        "gear_sizing": {
            "mtow_kg": mtow_kg,
            "x_nlg_m": x_nlg,
            "x_mlg_m": x_mlg,
            "aero_fwd_lim_x_m": aero_fwd_lim_x,
            "aero_aft_lim_x_m": aero_aft_lim_x,
            "fuselage_diameter_m": fuselage_diameter_m,
            "cg_height_estimate_m": fuselage_diameter_m * 1.1,
            "landing_gear_config": config.landing_gear.__dict__.copy(),
        },
    }


def main() -> None:
    reference_commit = _framework.alas_baseline()
    config = ALASConfig()
    report = FullAnalysis(config).run(
        DesignVector(), include_engines=True, verbose=False
    )
    renderer_inputs = _renderer_inputs(config, report)
    calls: list[dict[str, Any]] = []
    original_sizer = landing_gear_module.size_landing_gear

    def record_sizer(*args: Any, **kwargs: Any) -> Any:
        positional_names = (
            "mtow_kg",
            "x_nlg",
            "x_mlg",
            "aero_fwd_lim_x",
            "aero_aft_lim_x",
            "fuselage_diameter_m",
            "cg_height_estimate_m",
            "gear_config",
        )
        values = dict(zip(positional_names, args))
        values.update(kwargs)
        calls.append(
            {
                "mtow_kg": _float(values["mtow_kg"]),
                "x_nlg_m": _float(values["x_nlg"]),
                "x_mlg_m": _float(values["x_mlg"]),
                "aero_fwd_lim_x_m": _float(values["aero_fwd_lim_x"]),
                "aero_aft_lim_x_m": _float(values["aero_aft_lim_x"]),
                "fuselage_diameter_m": _float(values["fuselage_diameter_m"]),
                "cg_height_estimate_m": _float(values["cg_height_estimate_m"]),
                "landing_gear_config": values["gear_config"].__dict__.copy(),
            }
        )
        return original_sizer(*args, **kwargs)

    landing_gear_module.size_landing_gear = record_sizer
    figures = {}
    try:
        for theme in ("light", "dark"):
            figure = visualization_module.figure_landing_gear_planform(
                report, config, theme=theme
            )
            image_path = IMAGE_DIR / f"landing_gear_planform__{theme}.png"
            IMAGE_DIR.mkdir(parents=True, exist_ok=True)
            figure.savefig(image_path, dpi=150)
            figures[theme] = _figure_contract(figure, image_path, theme)
            plt.close(figure)
    finally:
        landing_gear_module.size_landing_gear = original_sizer
        plt.close("all")

    if len(calls) != 2:
        raise RuntimeError(f"expected two renderer sizing calls, got {len(calls)}")

    payload = {
        "schema": "w61-mlg-count-evidence/v1",
        "reference": {
            "git_commit": reference_commit,
            "environment": _framework.environment(),
            "input": "ALASConfig() + FullAnalysis(ALASConfig()).run(DesignVector(), include_engines=True)",
        },
        "effective_config": config.to_dict(),
        "report": {
            "static_margin": _float(report.static_margin),
            "x_neutral_point_m": _float(report.x_neutral_point),
            "physical_cg_m": [_float(value) for value in report.physical_cg],
            "component_masses_kg": {
                key: _float(value)
                for key, value in report.component_masses.items()
            },
        },
        "renderer_inputs": renderer_inputs,
        "renderer_calls": calls,
        "computed_gear": _gear_contract(original_sizer(
            renderer_inputs["gear_sizing"]["mtow_kg"],
            renderer_inputs["gear_sizing"]["x_nlg_m"],
            renderer_inputs["gear_sizing"]["x_mlg_m"],
            renderer_inputs["gear_sizing"]["aero_fwd_lim_x_m"],
            renderer_inputs["gear_sizing"]["aero_aft_lim_x_m"],
            renderer_inputs["gear_sizing"]["fuselage_diameter_m"],
            renderer_inputs["gear_sizing"]["cg_height_estimate_m"],
            config.landing_gear,
        )),
        "figures": figures,
    }
    OUT.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
