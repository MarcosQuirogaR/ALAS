# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Import a manually-exported SimBrief route KML.

Direct Python port of ``route_globe.m``'s coordinate extraction (it read
``LEMDHKJK.kml`` with a regex over the ``<coordinates>`` block): a SimBrief
"Download flight plan as KML" export packs the whole route -- SID, airway
waypoints, STAR -- into one ``<coordinates>lon,lat,alt lon,lat,alt ...`` list.
No SimBrief account/API key is needed to generate this file, just the
website's free flight planner.
"""

from __future__ import annotations

import re
from pathlib import Path

from .route import Route, Waypoint

_COORDS_RE = re.compile(r"<coordinates>(.*?)</coordinates>", re.DOTALL)


def route_from_kml(path: str | Path) -> Route:
    """Parse a SimBrief-exported KML into a :class:`Route`."""
    text = Path(path).read_text(encoding="utf-8")
    match = _COORDS_RE.search(text)
    if not match:
        raise ValueError(f"No <coordinates> block found in {path}")

    waypoints = []
    for token in match.group(1).split():
        parts = token.split(",")
        if len(parts) < 2:
            continue
        lon, lat = float(parts[0]), float(parts[1])
        alt = float(parts[2]) if len(parts) > 2 else 0.0
        waypoints.append(Waypoint(lat=lat, lon=lon, alt_m=alt))

    if len(waypoints) < 2:
        raise ValueError(f"KML at {path} produced fewer than 2 waypoints")
    return Route(waypoints=waypoints, source="simbrief_kml")
