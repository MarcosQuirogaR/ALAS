# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Route generation between ALAS's preset airports.

Three fidelity tiers, tried in order by :func:`Route.for_airports`:

1. A manually-imported SimBrief KML for that exact origin/destination pair
   (``alas/data/routes/{ORIGIN_ICAO}_{DEST_ICAO}.kml``) -- highest
   fidelity, real SID/STAR/airway-derived waypoints.
2. Open enroute navigation data (waypoints + jet airways), if the user has
   downloaded it -- see ``scripts/download_navdata.py``. Not bundled: the
   open X-Plane-format navdata is GPLv3-licensed and bundling it would impose
   copyleft obligations on anyone redistributing ALAS.
3. A great-circle interpolation -- always available, no setup required.
"""

from .route import Route, Waypoint

__all__ = ["Route", "Waypoint"]
