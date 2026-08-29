# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""Golden generator for `alas.reporting.route_globe` (spherical geometry and sync).

Captures waypoint 3D coordinate conversions and mass/altitude profile projection
against reference Python calculations.
"""

from __future__ import annotations

from types import SimpleNamespace
import numpy as np

import _framework

_framework.add_alas_to_path()

import alas.reporting.route_globe as rg  # noqa: E402
from alas.routing.route import Route, Waypoint  # noqa: E402


def main():
    # Case 1: Route with waypoints
    wps = [
        Waypoint(lat=40.4168, lon=-3.7038, alt_m=0.0, ident="LEMD"),
        Waypoint(lat=48.8566, lon=2.3522, alt_m=0.0, ident="LFPG"),
        Waypoint(lat=51.5074, lon=-0.1278, alt_m=0.0, ident="EGLL"),
    ]
    route = Route(waypoints=wps, source="navdata_graph")
    alts = np.array([600.0, 11000.0, 30.0])

    xyz = rg._route_to_xyz(route, alts).tolist()

    # Case 2: Mission sync projection
    mission_time = np.array([0.0, 1800.0, 5400.0, 7200.0])
    mission_tas = np.array([120.0, 230.0, 240.0, 140.0])
    mission_mass = np.array([75000.0, 72000.0, 68000.0, 66000.0])
    mission_alt = np.array([600.0, 11000.0, 11000.0, 30.0])

    fake_mission = SimpleNamespace(
        time_s=mission_time,
        tas_m_s=mission_tas,
        mass_kg=mission_mass,
        altitude_m=mission_alt,
    )

    mass_synced, alt_synced = rg.sync_mass_to_route(route, fake_mission)

    payload = {
        "route_to_xyz": {
            "wps": [{"lat": w.lat, "lon": w.lon} for w in wps],
            "alts": alts.tolist(),
            "xyz": xyz,
        },
        "sync_mass_to_route": {
            "mass": mass_synced.tolist(),
            "altitude": alt_synced.tolist(),
        },
    }

    _framework.write(
        "report",
        "route_geometry",
        payload,
        description="3D sphere Cartesian projection and mission profile distance interpolation.",
    )


if __name__ == "__main__":
    main()
