#!/usr/bin/env python
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
SUAVE mission runner -- entry point invoked as a subprocess from
``alas.integration.suave_bridge`` inside the isolated Python 3.10 venv.

Usage::

    python run_mission.py --request request.json --output-csv flight.csv \
        --output-summary summary.json

``request.json`` has two top-level keys, ``vehicle`` (consumed by
``vehicle_builder.build_vehicle``) and ``mission`` (consumed by
``mission_builder.mission_setup``). See
``alas/integration/suave_vehicle.py`` and
``alas/integration/suave_mission.py`` for how those are assembled from an
ALAS ``AnalysisReport`` + ``ALASConfig``.
"""

from __future__ import annotations

import argparse
import json
import sys
import traceback
from pathlib import Path

import _compat  # noqa: F401  (must run before `import SUAVE`)


def main(argv=None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--request", required=True, type=Path)
    parser.add_argument("--output-csv", required=True, type=Path)
    parser.add_argument("--output-summary", required=True, type=Path)
    args = parser.parse_args(argv)

    request = json.loads(args.request.read_text(encoding="utf-8"))

    import vehicle_builder
    import mission_builder
    import export_data
    import SUAVE
    from SUAVE.Core import Units

    vehicle = vehicle_builder.build_vehicle(request["vehicle"])

    configs = mission_builder.configs_setup(vehicle)
    mission_builder.simple_sizing(configs)
    configs.finalize()

    configs_analyses = mission_builder.analyses_setup(configs)
    mission = mission_builder.mission_setup(configs_analyses, request["mission"])
    missions_analyses = mission_builder.missions_setup(mission)

    analyses = SUAVE.Analyses.Analysis.Container()
    analyses.configs = configs_analyses
    analyses.missions = missions_analyses
    analyses.finalize()

    weights = analyses.configs.base.weights
    weights.evaluate()

    results = analyses.missions.base.evaluate()
    export_data.export_simulation_results(results, args.output_csv)

    segments = list(results.segments.values())
    first_mass = float(segments[0].conditions.weights.total_mass[0, 0])
    last_mass = float(segments[-1].conditions.weights.total_mass[-1, 0])
    block_time_s = sum(
        float(seg.conditions.frames.inertial.time[-1, 0]) - float(seg.conditions.frames.inertial.time[0, 0])
        for seg in segments
    )
    summary = {
        "status": "ok",
        "initial_mass_kg": first_mass,
        "final_mass_kg": last_mass,
        "fuel_burned_kg": first_mass - last_mass,
        "block_time_s": block_time_s,
        "n_segments": len(segments),
    }
    args.output_summary.write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(json.dumps(summary))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception:
        traceback.print_exc()
        sys.exit(1)
