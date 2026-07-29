# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Enroute airway routing from open navigation data.

Parses the open, X-Plane-format enroute navdata (``earth_fix.dat`` +
``earth_awy.dat``) into a waypoint/airway graph and finds the shortest path
(by great-circle leg distance) between the fixes nearest the departure and
arrival airports -- approximating the enroute portion of a real flight plan
(real jet airways, not a straight line).

**Duplicate idents:** 5-letter fix idents are only regionally unique, not
globally -- this dataset's 3-column legacy fix format (``lat lon ident``, no
region code; see the module-level note in ``routing/route.py``'s docs) means
~3 % of idents in a typical global set name two or more unrelated physical
fixes (e.g. a "MITSO" near the UK and a completely different "MITSO" near
Riyadh). ``earth_awy.dat`` references fixes by ident only in this mirror, so
naively collapsing same-ident fixes to a single location (as an earlier
version of this module did, keeping only the first-seen occurrence) can wire
a short local airway segment to the wrong, thousands-of-km-away same-named
fix -- producing a route that looks airway-derived but contains one
physically implausible "hop". ``_parse_airways`` instead keeps every
occurrence and disambiguates each edge by picking the candidate pair with
the smallest great-circle distance: real airway segments are tens to a few
hundred km, so the nearest candidate is always the physically correct one.

**Known simplification:** SID/STAR terminal procedures are not modeled (that
needs full ARINC424/CIFP parsing, a much larger and licensing-encumbered
effort for non-US data). The transition between the airport and the nearest
enroute fix is a straight line.

**Not bundled.** This data is published under GPLv3 by the X-Plane project
(via the ``mcantsin/x-plane-navdata`` mirror). Shipping it would impose that
licence on anyone redistributing ALAS, so it is neither committed nor
frozen into the executable. Fetch it once from Setup > External Tools, or run
``scripts/download_navdata.py``; until then this module reports no route and
callers fall back to a great circle.
"""

from __future__ import annotations

import heapq
import math
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Optional, Tuple

import numpy as np

from ..config.airports import Airport
from .route import EARTH_RADIUS_M, Route, Waypoint, haversine_m

from ..integration.assets import default_navdata_dir

_MAX_TRANSITION_M = 400_000.0  # don't snap to a fix more than ~215 NM from the airport

# Cache of the parsed (all_fixes, graph, connected-fix index list, coordinate
# array) for the last-used navdata_dir, keyed by resolved path + both files'
# mtimes so a re-download (or switching navdata_dir) invalidates it
# automatically. Every mission run otherwise re-parsed the full
# earth_fix.dat/earth_awy.dat (tens of thousands of lines for a global
# X-Plane navdata set) from scratch. Bounded to one entry since a single
# ALAS session normally works against one navdata directory. Graph
# nodes are indices into `all_fixes` (not idents -- see module docstring),
# so the cached coordinate array covers only fixes that are actually
# reachable in the airway graph, letting _nearest_fix scan them as one
# vectorized NumPy operation instead of a per-fix Python loop.
_CacheKey = Tuple[str, float, float]
_cache: Dict[
    _CacheKey,
    Tuple[List["Fix"], Dict[int, List[Tuple[int, float]]], List[int], np.ndarray],
] = {}


@dataclass
class Fix:
    ident: str
    lat: float
    lon: float


def navdata_available(navdata_dir: Path) -> bool:
    return (navdata_dir / "earth_fix.dat").exists() and (
        navdata_dir / "earth_awy.dat"
    ).exists()


def _parse_fixes(path: Path) -> Tuple[List[Fix], Dict[str, List[int]]]:
    """Parse the X-Plane fix data, keeping *every* occurrence of each ident.

    Returns ``(all_fixes, by_ident)``: ``all_fixes`` is every parsed
    :class:`Fix` in file order; ``by_ident`` maps each ident to the list of
    indices into ``all_fixes`` sharing that name (almost always length 1 --
    see the module docstring for why duplicates matter and aren't just
    collapsed to the first occurrence here).
    """
    all_fixes: List[Fix] = []
    by_ident: Dict[str, List[int]] = {}
    with path.open(encoding="utf-8", errors="ignore") as f:
        for line in f:
            parts = line.split()
            if len(parts) < 3:
                continue
            try:
                lat, lon = float(parts[0]), float(parts[1])
            except ValueError:
                continue  # header / version / footer lines
            ident = parts[2]
            by_ident.setdefault(ident, []).append(len(all_fixes))
            all_fixes.append(Fix(ident, lat, lon))
    return all_fixes, by_ident


def _parse_airways(
    path: Path,
    all_fixes: List[Fix],
    by_ident: Dict[str, List[int]],
) -> Dict[int, List[Tuple[int, float]]]:
    """Parse the X-Plane airway data into an adjacency map keyed by fix index.

    Each data line: ``<fix1> <region1> <term1> <fix2> <region2> <term2>
    <direction> <type> <base_fl> <top_fl> <airway_names>``. Endpoints are
    matched by ident only (this mirror's fix file has no region code to
    cross-reference against ``<region1>``/``<region2>``); when an ident has
    multiple candidate fixes, the pair (one candidate per endpoint) with the
    smallest great-circle distance is chosen, since a real airway leg is
    always short.
    """
    graph: Dict[int, List[Tuple[int, float]]] = {}

    def resolve_pair(ident_a: str, ident_b: str) -> Optional[Tuple[int, int, float]]:
        cand_a, cand_b = by_ident.get(ident_a), by_ident.get(ident_b)
        if not cand_a or not cand_b:
            return None
        if len(cand_a) == 1 and len(cand_b) == 1:
            ia, ib = cand_a[0], cand_b[0]
            fa, fb = all_fixes[ia], all_fixes[ib]
            return ia, ib, haversine_m(fa.lat, fa.lon, fb.lat, fb.lon)
        best: Optional[Tuple[float, int, int]] = None
        for ia in cand_a:
            fa = all_fixes[ia]
            for ib in cand_b:
                fb = all_fixes[ib]
                d = haversine_m(fa.lat, fa.lon, fb.lat, fb.lon)
                if best is None or d < best[0]:
                    best = (d, ia, ib)
        if best is None:
            return None
        d, ia, ib = best
        return ia, ib, d

    def link(ident_a: str, ident_b: str) -> None:
        resolved = resolve_pair(ident_a, ident_b)
        if resolved is None:
            return
        ia, ib, d = resolved
        graph.setdefault(ia, []).append((ib, d))
        graph.setdefault(ib, []).append((ia, d))

    with path.open(encoding="utf-8", errors="ignore") as f:
        for line in f:
            parts = line.split()
            if len(parts) < 10:
                continue
            fix1, fix2, direction = parts[0], parts[3], parts[6]
            link(fix1, fix2)
            # "F" = forward-only airway leg; otherwise bidirectional (already linked both ways).
            del direction  # direction restriction not modeled at this fidelity
    return graph


def _nearest_fix(
    connected: List[int],
    coords_rad: np.ndarray,
    lat: float,
    lon: float,
) -> Optional[int]:
    """Index (into ``all_fixes``) of the nearest *graph-connected* fix to
    ``(lat, lon)`` by great-circle distance, vectorized over the whole
    connected-fix set instead of a per-fix Python loop (``connected``/
    ``coords_rad`` come from :func:`_load_navdata`'s cache -- built once per
    navdata set, not once per lookup). Restricting to connected fixes avoids
    snapping to an isolated fix that ``_dijkstra`` could never route from.
    """
    if not connected:
        return None
    lat_r, lon_r = math.radians(lat), math.radians(lon)
    lat_arr, lon_arr = coords_rad[:, 0], coords_rad[:, 1]
    dlat = lat_arr - lat_r
    dlon = lon_arr - lon_r
    a = np.sin(dlat / 2) ** 2 + np.cos(lat_r) * np.cos(lat_arr) * np.sin(dlon / 2) ** 2
    dist_m = EARTH_RADIUS_M * 2 * np.arcsin(np.minimum(1.0, np.sqrt(a)))
    row = int(np.argmin(dist_m))
    if dist_m[row] > _MAX_TRANSITION_M:
        return None
    return connected[row]


def _dijkstra(
    graph: Dict[int, List[Tuple[int, float]]], start: int, goal: int
) -> Optional[List[int]]:
    if start not in graph or goal not in graph:
        return None
    dist = {start: 0.0}
    prev: Dict[int, int] = {}
    visited = set()
    heap = [(0.0, start)]
    while heap:
        d, node = heapq.heappop(heap)
        if node in visited:
            continue
        visited.add(node)
        if node == goal:
            break
        for neighbor, weight in graph.get(node, []):
            nd = d + weight
            if nd < dist.get(neighbor, math.inf):
                dist[neighbor] = nd
                prev[neighbor] = node
                heapq.heappush(heap, (nd, neighbor))
    if goal not in dist:
        return None
    path = [goal]
    while path[-1] != start:
        path.append(prev[path[-1]])
    path.reverse()
    return path


def _load_navdata(navdata_dir: Path):
    """Parse (or return the cached) fixes/graph/coordinate-array for ``navdata_dir``."""
    fix_path = navdata_dir / "earth_fix.dat"
    awy_path = navdata_dir / "earth_awy.dat"
    key: _CacheKey = (
        str(navdata_dir.resolve()),
        fix_path.stat().st_mtime,
        awy_path.stat().st_mtime,
    )
    cached = _cache.get(key)
    if cached is not None:
        return cached

    all_fixes, by_ident = _parse_fixes(fix_path)
    graph = _parse_airways(awy_path, all_fixes, by_ident)
    connected = sorted(graph.keys())
    coords_rad = (
        np.radians(
            np.array(
                [[all_fixes[i].lat, all_fixes[i].lon] for i in connected], dtype=float
            )
        )
        if connected
        else np.empty((0, 2))
    )

    _cache.clear()  # bounded to one navdata_dir at a time
    _cache[key] = (all_fixes, graph, connected, coords_rad)
    return _cache[key]


def find_airway_route(
    origin: Airport, dest: Airport, navdata_dir: Optional[Path] = None
) -> Optional[Route]:
    """Find a route following real jet airways, or ``None`` if navdata isn't available."""
    navdata_dir = navdata_dir or default_navdata_dir()
    if not navdata_available(navdata_dir):
        return None

    all_fixes, graph, connected, coords_rad = _load_navdata(navdata_dir)

    entry_idx = _nearest_fix(
        connected, coords_rad, origin.latitude_deg, origin.longitude_deg
    )
    exit_idx = _nearest_fix(
        connected, coords_rad, dest.latitude_deg, dest.longitude_deg
    )
    if entry_idx is None or exit_idx is None:
        return None

    path = _dijkstra(graph, entry_idx, exit_idx)
    if path is None:
        return None

    waypoints = [Waypoint(origin.latitude_deg, origin.longitude_deg, ident=origin.icao)]
    for idx in path:
        fx = all_fixes[idx]
        waypoints.append(Waypoint(fx.lat, fx.lon, ident=fx.ident))
    waypoints.append(Waypoint(dest.latitude_deg, dest.longitude_deg, ident=dest.icao))
    return Route(waypoints=waypoints, source="navdata_graph")
