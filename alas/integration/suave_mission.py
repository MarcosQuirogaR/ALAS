# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Builds the ``mission`` half of the SUAVE mission request: cruise targets from
:class:`~alas.config.requirements.DesignRequirements`, departure/arrival
field elevation from :class:`~alas.config.airports.Airport` (already
present and exactly matching the figures ``suave_example.py`` hand-coded for
Madrid/LEMD), and the route distance supplied by the caller (see
:mod:`alas.routing` for how that's computed).
"""

from __future__ import annotations

import dataclasses
from typing import Any, Dict

from ..config.airports import Airport
from ..config.settings import ALASConfig


def build_mission_request(
    config: ALASConfig,
    origin: Airport,
    dest: Airport,
    route_distance_m: float,
) -> Dict[str, Any]:
    req = config.requirements
    return {
        "mission_tag": f"{origin.icao}_to_{dest.icao}",
        # Note: cruise_mach is NOT read by mission_builder.py -- every cruise
        # segment's air speed comes from config.mission.profile (below) as an
        # explicit TAS, not derived from Mach. The vehicle request (see
        # suave_vehicle.py) carries its own cruise_mach for engine sizing.
        "cruise_altitude_m": req.cruise_altitude_m,
        "departure_elevation_m": origin.elevation_m,
        "arrival_elevation_m": dest.elevation_m,
        "departure_isa_deviation_c": origin.isa_deviation_c,
        "route_distance_m": route_distance_m,
        # Every climb/cruise/descent speed and rate is user-editable in
        # Advanced Settings -> Mission Analysis (config/mission_config.py),
        # not hardcoded in the SUAVE runner.
        "profile": dataclasses.asdict(config.mission.profile),
    }
