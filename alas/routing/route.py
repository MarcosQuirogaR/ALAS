# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Core route representation: a sequence of lateral waypoints between two
airports, with cumulative great-circle distance along the path.

Altitude is left at 0 (ground track) for generated routes -- the route's job
is the *lateral* path (great circle vs. airway vs. real SID/STAR/airway KML).
Vertical profile comes from the SUAVE mission CSV and is blended in by
:func:`alas.reporting.route_globe.sync_mass_to_route`, which is more
accurate than a guessed climb/descent ramp since ALAS actually has a
simulated altitude-vs-distance profile once a mission has been run.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from pathlib import Path
from typing import List, Optional

import numpy as np

from ..config.airports import Airport

EARTH_RADIUS_M = 6_371_000.0


@dataclass
class Waypoint:
    """One point along a route."""

    lat: float  # degrees, +N
    lon: float  # degrees, +E
    alt_m: float = 0.0
    ident: str = ""  # fix/airport identifier, if known


@dataclass
class Route:
    """A lateral flight path between two airports."""

    waypoints: List[Waypoint] = field(default_factory=list)
    # "great_circle" | "navdata_graph" | "simbrief_kml" (manual KML import) |
    # "simbrief_api" (live fetch of the user's last-generated SimBrief OFP)
    source: str = "great_circle"
    # The route's EFFECTIVE endpoints. Normally the configured departure/
    # arrival pair, but a SimBrief OFP is allowed to override them (see
    # simbrief_route.fetch_simbrief_route's ``allow_mismatch``): a real
    # dispatched OFP outranks a manually-picked pair, and callers must then
    # size the mission against the airports actually flown, not the ones the
    # user happened to leave selected. ``None`` means "unchanged from what the
    # caller passed in".
    origin_airport: Optional[Airport] = None
    dest_airport: Optional[Airport] = None

    @property
    def cumulative_distance_m(self) -> np.ndarray:
        """Cumulative great-circle distance (m) at each waypoint, starting at 0."""
        if not self.waypoints:
            return np.array([])
        dist = np.zeros(len(self.waypoints))
        for i in range(1, len(self.waypoints)):
            a, b = self.waypoints[i - 1], self.waypoints[i]
            dist[i] = dist[i - 1] + haversine_m(a.lat, a.lon, b.lat, b.lon)
        return dist

    @property
    def total_distance_m(self) -> float:
        d = self.cumulative_distance_m
        return float(d[-1]) if len(d) else 0.0

    # -- constructors --------------------------------------------------------
    @classmethod
    def great_circle(cls, origin: Airport, dest: Airport, n: int = 50) -> "Route":
        """Interpolate ``n`` points along the great-circle arc between two airports."""
        lat1, lon1 = (
            math.radians(origin.latitude_deg),
            math.radians(origin.longitude_deg),
        )
        lat2, lon2 = math.radians(dest.latitude_deg), math.radians(dest.longitude_deg)

        d = _angular_distance(lat1, lon1, lat2, lon2)
        waypoints = [
            Waypoint(origin.latitude_deg, origin.longitude_deg, ident=origin.icao)
        ]
        if d > 1e-9:
            for i in range(1, n):
                f = i / n
                lat, lon = _slerp(lat1, lon1, lat2, lon2, d, f)
                waypoints.append(Waypoint(math.degrees(lat), math.degrees(lon)))
        waypoints.append(
            Waypoint(dest.latitude_deg, dest.longitude_deg, ident=dest.icao)
        )
        return cls(waypoints=waypoints, source="great_circle")

    @classmethod
    def for_airports(
        cls,
        origin: Airport,
        dest: Airport,
        routes_dir: Optional[Path] = None,
        navdata_dir: Optional[Path] = None,
        great_circle_points: int = 50,
        simbrief_username: Optional[str] = None,
        simbrief_timeout_s: float = 15.0,
        simbrief_overrides_airports: bool = True,
    ) -> "Route":
        """Best available route: SimBrief API > manual KML > open navdata graph > great circle.

        ``routes_dir``/``navdata_dir`` are normally
        ``config.mission.routes_dir``/``navdata_dir`` (editable in Advanced
        Settings -> Mission Analysis) resolved to absolute paths by the
        caller; both default to the standard ``alas/data/...`` location.
        ``simbrief_username`` (``config.mission.simbrief_username``) is a
        SimBrief username or numeric Pilot ID; when set, this tries fetching
        that user's most recently generated OFP first (see
        ``routing.simbrief_route`` for what that can/can't do) before
        falling through to the other tiers.

        ``simbrief_overrides_airports`` (default True,
        ``config.mission.simbrief_overrides_airports``) lets a fetched OFP for a
        *different* city pair win over ``origin``/``dest`` -- a real dispatched
        OFP is the highest-fidelity routing available, so it takes precedence
        over the manually-selected pair. The effective endpoints come back on
        the returned route's ``origin_airport``/``dest_airport``. Set it False
        to keep the old behaviour (skip a mismatched OFP entirely).
        """
        if simbrief_username:
            import logging
            from .simbrief_route import fetch_simbrief_route

            try:
                route = fetch_simbrief_route(
                    simbrief_username,
                    origin,
                    dest,
                    timeout_s=simbrief_timeout_s,
                    allow_mismatch=simbrief_overrides_airports,
                )
                if route is not None:
                    logging.getLogger("alas.routing").info(
                        "Using SimBrief OFP route %s->%s (%d waypoints).",
                        origin.icao,
                        dest.icao,
                        len(route.waypoints),
                    )
                    return route
            except Exception:
                pass  # SimBrief unreachable/misconfigured -- fall through to the next tier

        routes_dir = routes_dir or (
            Path(__file__).resolve().parents[2] / "alas" / "data" / "routes"
        )
        kml_path = routes_dir / f"{origin.icao}_{dest.icao}.kml"
        if kml_path.exists():
            from .kml_import import route_from_kml

            try:
                return route_from_kml(kml_path)
            except Exception:
                pass  # fall through to the next tier

        try:
            from .navdata_graph import find_airway_route

            route = find_airway_route(origin, dest, navdata_dir=navdata_dir)
            if route is not None:
                return route
        except Exception:
            pass  # navdata not downloaded / unparseable -- fall back

        return cls.great_circle(origin, dest, n=great_circle_points)


def haversine_m(lat1, lon1, lat2, lon2) -> float:
    p1, l1, p2, l2 = map(math.radians, (lat1, lon1, lat2, lon2))
    return EARTH_RADIUS_M * _angular_distance(p1, l1, p2, l2)


def _angular_distance(lat1, lon1, lat2, lon2) -> float:
    """Central angle (radians) between two points given in radians."""
    dlat, dlon = lat2 - lat1, lon2 - lon1
    a = (
        math.sin(dlat / 2) ** 2
        + math.cos(lat1) * math.cos(lat2) * math.sin(dlon / 2) ** 2
    )
    return 2 * math.asin(min(1.0, math.sqrt(a)))


def _slerp(lat1, lon1, lat2, lon2, d, f):
    """Spherical interpolation at fraction ``f`` along the arc of angular length ``d``."""
    a = math.sin((1 - f) * d) / math.sin(d)
    b = math.sin(f * d) / math.sin(d)
    x = a * math.cos(lat1) * math.cos(lon1) + b * math.cos(lat2) * math.cos(lon2)
    y = a * math.cos(lat1) * math.sin(lon1) + b * math.cos(lat2) * math.sin(lon2)
    z = a * math.sin(lat1) + b * math.sin(lat2)
    lat = math.atan2(z, math.sqrt(x * x + y * y))
    lon = math.atan2(y, x)
    return lat, lon
