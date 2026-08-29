# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-opt::sampling``: Design variable sampling, widened bounds, and validation checks."""

from __future__ import annotations

import numpy as np

import _framework

_framework.add_alas_to_path()

from alas.config.design_variables import DesignVector  # noqa: E402
from alas.config.settings import ALASConfig  # noqa: E402
from alas.optimization.sampling import _widened_bounds  # noqa: E402


def generate() -> dict:
    # 1. Widened bounds test cases
    default_bounds = DesignVector.bounds()
    lo_wide, hi_wide = _widened_bounds(default_bounds, slack=0.2)

    custom_bounds = [
        (65.0, 75.0),  # span_m
        (14.0, 18.0),  # root_chord_m
        (7.0, 9.0),    # break_chord_m
        (2.5, 3.5),    # tip_chord_m
        (28.0, 34.0),  # sweep_deg
        (65.0, 75.0),  # fuselage_length_m
        (0.95, 1.05),  # airfoil_thickness_scale
        (1.0, 3.0),    # washout_deg
        (-0.005, 0.005),  # hicks_henne_u1
        (-0.005, 0.005),  # hicks_henne_u2
        (-0.005, 0.005),  # hicks_henne_l1
        (-0.005, 0.005),  # hicks_henne_l2
    ]
    lo_cust_wide, hi_cust_wide = _widened_bounds(custom_bounds, slack=0.15)

    widened_cases = [
        {
            "label": "default_bounds_slack_0.2",
            "slack": 0.2,
            "bounds": [{"lo": float(lo), "hi": float(hi)} for lo, hi in default_bounds],
            "widened_lo": [float(v) for v in lo_wide],
            "widened_hi": [float(v) for v in hi_wide],
        },
        {
            "label": "custom_bounds_slack_0.15",
            "slack": 0.15,
            "bounds": [{"lo": float(lo), "hi": float(hi)} for lo, hi in custom_bounds],
            "widened_lo": [float(v) for v in lo_cust_wide],
            "widened_hi": [float(v) for v in hi_cust_wide],
        },
    ]

    # 2. Validation check cases on candidates
    config = ALASConfig()
    from alas.validation import validate

    val_issues = validate(config)
    error_count = len([i for i in val_issues if getattr(i, "severity", "error") == "error"])

    return {
        "widened_cases": widened_cases,
        "validation_test": {
            "default_config_error_count": error_count,
        },
    }


if __name__ == "__main__":
    _framework.write(
        "opt",
        "sampling",
        generate(),
        description="Design variable sampling, widened bounds calculations, and validation error filtering",
    )
