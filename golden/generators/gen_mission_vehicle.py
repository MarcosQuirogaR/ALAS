# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-mission::vehicle``: the vehicle half of the SUAVE mission request.

``build_vehicle_request`` (``alas/integration/suave_vehicle.py``) reads five
fields off a full-analysis ``AnalysisReport`` -- the two design-point
lift-to-drag ratios that decide the cruise thrust, and the design vector,
geometry summary and component masses it carries through -- and folds them in
with the requirements and engine cycle off ``ALASConfig``. The one computed
quantity is ``_cruise_thrust_kn_per_engine``; everything else is routing.

So this fixture runs the *real* report: for each preset it builds the aircraft,
runs the full high-fidelity analysis the pipeline runs (``FullAnalysis.run``),
and calls ``build_vehicle_request`` on the result. It records the five report
fields the builder reads as a ``report_view`` -- exactly the [`ReportView`] the
Rust port takes, following the "take the fields you read, not the type" scoping
of ``alas-perf``'s ``build_vn_diagram`` -- and the assembled ``request``. The
Rust parity test rebuilds ``AlasConfig::from_value({"preset": name})``, feeds it
the recorded ``report_view``, and compares the request it assembles.

Runs under the alas interpreter (``.venv``); the analysis is in-process (no
external solver).
"""

from __future__ import annotations

import dataclasses

import _framework

_framework.add_alas_to_path()

from alas.analysis.full_analysis import FullAnalysis  # noqa: E402
from alas.config.presets import get_preset  # noqa: E402
from alas.config.settings import ALASConfig  # noqa: E402
from alas.integration.suave_vehicle import build_vehicle_request  # noqa: E402

# Presets whose full analysis exercises distinct engines, requirements and
# geometry, so the cruise-thrust arithmetic and the geometry passthrough are not
# checked on one aircraft.
PRESETS = ["A320-200", "A340-300"]


def _case(preset: str) -> dict:
    config = ALASConfig.from_dict({"preset": preset})
    design = get_preset(preset).design_vector
    report = FullAnalysis(config).run(design, verbose=False)

    trimmed = report.trimmed_design_point
    report_view = {
        "trimmed_l_over_d": None if trimmed is None else float(trimmed.l_over_d),
        "plain_l_over_d": float(report.design_point.l_over_d),
        "design_vector": dataclasses.asdict(report.design),
        "geometry_summary": report.geometry_summary,
        "component_masses": report.component_masses,
    }
    request = build_vehicle_request(report, config)
    return {"preset": preset, "report_view": report_view, "request": request}


def main() -> None:
    cases = [_case(preset) for preset in PRESETS]
    _framework.write(
        "mission",
        "vehicle",
        {"cases": cases},
        description=(
            "build_vehicle_request over two presets' full-analysis reports: the "
            "vehicle half of the SUAVE mission request, checking the cruise-"
            "thrust derivation, the engine/requirements blocks and the geometry "
            "and report passthroughs."
        ),
    )


if __name__ == "__main__":
    main()
