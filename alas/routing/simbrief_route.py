# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Route generation via the SimBrief API.

Fetches the user's own most recently generated SimBrief dispatch flight plan
(an "OFP") instead of ALAS needing to source and parse licensed
ARINC424/CIFP terminal-procedure data itself -- SimBrief's own dispatch
engine has already computed real SID/STAR/airway routing against a current
AIRAC cycle by the time this module ever runs.

**What this can and can't do.** SimBrief's public, no-approval-required
endpoint (``xml.fetcher.php``) only returns a user's *most recently
generated* OFP -- there is no free, unapproved way to trigger a *new*
dispatch for a specific origin/destination pair on demand (that needs a
Navigraph-approved API key). So the practical workflow is:

1. Generate the OFP for the exact origin/destination pair on simbrief.com
   (or the SimBrief mobile/desktop app, or a compatible EFB) using your own
   SimBrief/Navigraph account, same as you would to fly that route in a
   simulator.
2. Run ALAS's mission analysis for that same city pair -- it fetches
   your last OFP and uses it as the route, *only* if its origin/destination
   actually match what was requested (otherwise it's someone else's plan,
   or an older one for a different route, and this returns ``None`` rather
   than silently using the wrong route).

Never raises: any network error, timeout, malformed response, or an
origin/destination mismatch all fall through to ``None`` so
``routing.route.Route.for_airports`` can move on to the next routing tier.
"""

from __future__ import annotations

import json
import logging
import urllib.error
import urllib.parse
import urllib.request
from typing import Optional

from ..config.airports import Airport
from .route import Route, Waypoint

_FETCH_URL = "https://www.simbrief.com/api/xml.fetcher.php"
_DEFAULT_TIMEOUT_S = 15.0

# SimBrief reports field elevation and runway lengths in feet.
_M_PER_FT = 0.3048

# Logs to the sidecar's stderr (drained to the desktop app's log by
# desktop/sidecar.go), so a user can tell WHY the SimBrief tier was skipped --
# a network/parse failure vs. the legitimate "your last OFP is a different city
# pair" case -- instead of it silently looking broken.
logger = logging.getLogger("alas.routing.simbrief")


def _airport_from_ofp(node: dict, fallback: Airport) -> Airport:
    """Build an :class:`Airport` from a SimBrief OFP origin/destination node.

    Used when the OFP is for a different city pair than the one configured and
    SimBrief is allowed to override it: the OFP's airport may not be in
    ALAS's own 20-entry database at all, so synthesize an equivalent entry
    from the OFP's data. Runway/elevation figures come from the OFP where
    present; anything SimBrief doesn't supply falls back to the configured
    airport's value so downstream field-performance maths still has a sane
    number to work with.
    """

    def _f(key: str, default: float) -> float:
        try:
            return float(node[key])
        except (KeyError, TypeError, ValueError):
            return default

    icao = str(node.get("icao_code", "")).strip().upper() or fallback.icao
    name = str(node.get("name", "")).strip() or icao
    # SimBrief reports elevation/runway length in feet.
    elevation_m = _f("elevation", fallback.elevation_m / _M_PER_FT) * _M_PER_FT
    runway_m = _f("plan_rwy_length", 0.0) * _M_PER_FT
    return Airport(
        name=f"{name} ({icao})",
        icao=icao,
        elevation_m=elevation_m,
        toda_m=runway_m or fallback.toda_m,
        lda_m=runway_m or fallback.lda_m,
        isa_deviation_c=fallback.isa_deviation_c,
        notes="From SimBrief OFP",
        latitude_deg=_f("pos_lat", fallback.latitude_deg),
        longitude_deg=_f("pos_long", fallback.longitude_deg),
    )


def fetch_simbrief_route(
    identifier: str,
    origin: Airport,
    dest: Airport,
    timeout_s: float = _DEFAULT_TIMEOUT_S,
    allow_mismatch: bool = False,
) -> Optional[Route]:
    """Fetch ``identifier``'s (SimBrief username or numeric Pilot ID) most
    recent OFP and return it as a :class:`Route`, or ``None`` if SimBrief is
    unreachable or the response is unparseable.

    ``allow_mismatch`` decides what happens when the fetched OFP is for a
    *different* city pair than ``origin``/``dest``:

    * ``False`` -- return ``None`` so the caller falls through to the other
      route tiers (the original, conservative behaviour).
    * ``True`` -- use the OFP anyway and let it define the route's real
      endpoints, which are attached to the returned
      :class:`~alas.routing.route.Route` as ``origin_airport``/
      ``dest_airport``. A real dispatched OFP is the highest-fidelity routing
      ALAS can get, so when the user has opted into SimBrief it should win
      over the manually-picked pair rather than being silently discarded.
    """
    identifier = (identifier or "").strip()
    if not identifier:
        return None

    param = "userid" if identifier.isdigit() else "username"
    url = f"{_FETCH_URL}?{param}={urllib.parse.quote(identifier)}&json=1"

    try:
        with urllib.request.urlopen(url, timeout=timeout_s) as resp:
            data = json.loads(resp.read().decode("utf-8"))
    except (urllib.error.URLError, TimeoutError, ValueError, OSError) as exc:
        logger.warning(
            "SimBrief fetch failed for '%s' (%s) -- falling back to other route tiers.",
            identifier,
            exc,
        )
        return None

    try:
        ofp_origin = str(data["origin"]["icao_code"]).strip().upper()
        ofp_dest = str(data["destination"]["icao_code"]).strip().upper()
        mismatch = ofp_origin != origin.icao.upper() or ofp_dest != dest.icao.upper()
        if mismatch and not allow_mismatch:
            # A real OFP, but for a different city pair than requested (the
            # user hasn't generated this exact route on SimBrief, or this is
            # a stale one from an earlier session) -- don't silently use it.
            logger.info(
                "SimBrief's most recent OFP for '%s' is %s->%s, not the requested %s->%s -- "
                "generate that exact city pair on simbrief.com first. Using the next route tier.",
                identifier,
                ofp_origin,
                ofp_dest,
                origin.icao.upper(),
                dest.icao.upper(),
            )
            return None

        # When overriding, the OFP's OWN airports define the route -- using the
        # configured pair's coordinates here would splice a Madrid endpoint onto
        # a London navlog and produce a nonsense track.
        eff_origin = _airport_from_ofp(data["origin"], origin) if mismatch else origin
        eff_dest = _airport_from_ofp(data["destination"], dest) if mismatch else dest
        if mismatch:
            logger.info(
                "SimBrief OFP for '%s' is %s->%s; overriding the configured %s->%s "
                "(a real dispatched OFP is the highest-fidelity route available).",
                identifier,
                ofp_origin,
                ofp_dest,
                origin.icao.upper(),
                dest.icao.upper(),
            )

        fixes = data["navlog"]["fix"]
        if isinstance(fixes, dict):
            fixes = [fixes]  # SimBrief's XML->JSON conversion collapses a
            # single-element navlog to a bare dict, not a list

        waypoints = [
            Waypoint(
                eff_origin.latitude_deg, eff_origin.longitude_deg, ident=eff_origin.icao
            )
        ]
        for fx in fixes:
            waypoints.append(
                Waypoint(
                    lat=float(fx["pos_lat"]),
                    lon=float(fx["pos_long"]),
                    ident=str(fx.get("ident", "")),
                )
            )
        waypoints.append(
            Waypoint(eff_dest.latitude_deg, eff_dest.longitude_deg, ident=eff_dest.icao)
        )
    except (KeyError, TypeError, ValueError):
        # Response shape didn't match what's expected -- fall back rather
        # than guess at a changed/undocumented SimBrief schema.
        return None

    return Route(
        waypoints=waypoints,
        source="simbrief_api",
        origin_airport=eff_origin,
        dest_airport=eff_dest,
    )
