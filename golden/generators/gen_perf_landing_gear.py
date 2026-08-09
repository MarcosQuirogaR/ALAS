# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-perf::landing_gear``: the gear sizing buildup end to end.

Exercises ``alas/physics/landing_gear.py``'s public ``size_landing_gear`` --
which composes the private ``_select_tire`` and ``_size_bogie`` helpers, the
two-point static reaction, the derived strength fractions, the lateral
turnover check and the planform wheel positions -- across a spread of MTOW,
CG limits, fuselage size and ``LandingGearConfig`` overrides chosen to reach
each branch:

* ``narrowbody_auto``: an A320-class weight on defaults -- single nose wheel
  (below ``nlg_dual_wheel_mtow_kg``), two main struts (below
  ``mlg_body_gear_mtow_kg``), auto tire selection.
* ``widebody_auto``: a 787-class weight -- dual nose wheel, two struts, a
  heavier auto-selected tire and larger bogie.
* ``heavy_body_gear``: an A380-class weight above ``mlg_body_gear_mtow_kg``,
  so four struts (centreline body gear) are added and the wheel layout grows
  the Body-L/Body-R struts.
* ``forced_counts``: explicit ``n_nlg_wheels``/``n_mlg_struts``/
  ``wheels_per_mlg_strut`` and an explicit ``tire_class``, exercising
  ``_size_bogie``'s forced-count path and ``_select_tire``'s explicit branch.
* ``tippy_forward_cg``: a tall CG with a forward limit close to the nose gear,
  driving the turnover angle past the limit so ``turnover_ok`` is False.

The ``LandingGearConfig`` overrides are applied by ``setattr`` on a default
instance (no dataclass ``__post_init__`` re-derivation), the same as the Rust
parity test reconstructs them.
"""

from __future__ import annotations

import _framework

_framework.add_alas_to_path()

from alas.config.landing_gear_config import LandingGearConfig  # noqa: E402
from alas.physics.landing_gear import size_landing_gear  # noqa: E402


def _gear_config(overrides: dict) -> LandingGearConfig:
    cfg = LandingGearConfig()
    for key, value in overrides.items():
        setattr(cfg, key, value)
    return cfg


def _tire_record(tire) -> dict:
    return {
        "code": tire.code,
        "name": tire.name,
        "rated_load_kg": float(tire.rated_load_kg),
        "diameter_m": float(tire.diameter_m),
        "width_m": float(tire.width_m),
    }


def _wheel_record(wheel) -> dict:
    return {
        "x": float(wheel.x),
        "y": float(wheel.y),
        "group": wheel.group,
        "strut_label": wheel.strut_label,
        "diameter_m": float(wheel.diameter_m),
        "width_m": float(wheel.width_m),
    }


def _layout_record(layout) -> dict:
    return {
        "n_nlg_wheels": int(layout.n_nlg_wheels),
        "n_mlg_struts": int(layout.n_mlg_struts),
        "wheels_per_mlg_strut": int(layout.wheels_per_mlg_strut),
        "nlg_tire": _tire_record(layout.nlg_tire),
        "mlg_tire": _tire_record(layout.mlg_tire),
        "strut_material": layout.strut_material,
        "x_nlg": float(layout.x_nlg),
        "x_mlg": float(layout.x_mlg),
        "track_width_m": float(layout.track_width_m),
        "wheelbase_m": float(layout.wheelbase_m),
        "wheels": [_wheel_record(w) for w in layout.wheels],
        "r_nlg_design_kg": float(layout.r_nlg_design_kg),
        "r_mlg_total_design_kg": float(layout.r_mlg_total_design_kg),
        "pct_load_nlg_max": float(layout.pct_load_nlg_max),
        "pct_load_mlg_max": float(layout.pct_load_mlg_max),
        "turnover_angle_deg": float(layout.turnover_angle_deg),
        "turnover_ok": bool(layout.turnover_ok),
    }


# Each case: the eight positional/scalar inputs and the LandingGearConfig
# overrides. x_nlg/x_mlg and the aerodynamic CG limits are physical fuselage
# stations [m]; the geometry is shaped like objective.py's own call site.
_CASES = [
    {
        "name": "light_single_wheel",
        "mtow_kg": 12_000.0,
        "x_nlg": 2.2,
        "x_mlg": 8.0,
        "aero_fwd_lim_x": 7.0,
        "aero_aft_lim_x": 7.8,
        "fuselage_diameter_m": 2.7,
        "cg_height_estimate_m": 2.7 * 1.1,
        "config": {},
    },
    {
        "name": "narrowbody_auto",
        "mtow_kg": 79_000.0,
        "x_nlg": 5.7,
        "x_mlg": 18.5,
        "aero_fwd_lim_x": 16.8,
        "aero_aft_lim_x": 18.2,
        "fuselage_diameter_m": 3.95,
        "cg_height_estimate_m": 3.95 * 1.1,
        "config": {},
    },
    {
        "name": "widebody_auto",
        "mtow_kg": 254_000.0,
        "x_nlg": 8.5,
        "x_mlg": 32.0,
        "aero_fwd_lim_x": 29.0,
        "aero_aft_lim_x": 31.5,
        "fuselage_diameter_m": 5.77,
        "cg_height_estimate_m": 5.77 * 1.1,
        "config": {},
    },
    {
        "name": "heavy_body_gear",
        "mtow_kg": 560_000.0,
        "x_nlg": 10.0,
        "x_mlg": 40.0,
        "aero_fwd_lim_x": 36.5,
        "aero_aft_lim_x": 39.5,
        "fuselage_diameter_m": 7.14,
        "cg_height_estimate_m": 7.14 * 1.1,
        "config": {},
    },
    {
        "name": "forced_counts",
        "mtow_kg": 254_000.0,
        "x_nlg": 8.5,
        "x_mlg": 32.0,
        "aero_fwd_lim_x": 29.0,
        "aero_aft_lim_x": 31.5,
        "fuselage_diameter_m": 5.77,
        "cg_height_estimate_m": 5.77 * 1.1,
        "config": {
            "n_nlg_wheels": 2,
            "n_mlg_struts": 4,
            "wheels_per_mlg_strut": 6,
            "tire_class": "widebody",
            "strut_material": "titanium (custom)",
        },
    },
    {
        "name": "tippy_forward_cg",
        "mtow_kg": 120_000.0,
        "x_nlg": 6.0,
        "x_mlg": 12.0,
        "aero_fwd_lim_x": 6.4,
        "aero_aft_lim_x": 11.0,
        "fuselage_diameter_m": 6.5,
        "cg_height_estimate_m": 9.5,
        "config": {},
    },
]


def main() -> None:
    results = []
    for case in _CASES:
        gear_config = _gear_config(case["config"])
        layout = size_landing_gear(
            case["mtow_kg"],
            case["x_nlg"],
            case["x_mlg"],
            case["aero_fwd_lim_x"],
            case["aero_aft_lim_x"],
            fuselage_diameter_m=case["fuselage_diameter_m"],
            cg_height_estimate_m=case["cg_height_estimate_m"],
            gear_config=gear_config,
        )
        results.append(
            {
                "name": case["name"],
                "inputs": {k: v for k, v in case.items() if k not in ("name", "config")},
                "config": case["config"],
                "layout": _layout_record(layout),
            }
        )

    _framework.write(
        "perf",
        "landing_gear",
        {"cases": results},
        description=(
            "alas.physics.landing_gear.size_landing_gear across MTOW/CG-limit/"
            "fuselage/LandingGearConfig cases reaching each tire-selection, "
            "bogie-sizing, gear-count and turnover branch -- full LandingGearLayout"
        ),
    )


if __name__ == "__main__":
    main()
