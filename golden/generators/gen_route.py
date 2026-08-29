# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-route``: the lateral path between two airports, at all four tiers.

Three of the four routing tiers read data this repository cannot contain. The
open enroute navigation data is GPLv3 and deliberately not bundled; a
hand-exported KML is a user's own file; and a dispatch plan arrives over the
network from a service that only ever returns the *most recently generated*
one, which is not a reproducible input by any definition.

So this generator **authors its own inputs** and checks them in beside the
fixture, under ``golden/route/inputs/``. That is not a weaker check than
running against the real data -- it is a stronger one. A synthetic navdata set
of a dozen fixes can be built to reach the branches that matter and that a real
global set reaches only by accident: an identifier naming two unrelated fixes
half a world apart, a fix no airway touches, an airport beyond the transition
limit, and two paths between the same pair whose lengths differ by a tenth of a
percent, so the search has to weigh them rather than take whichever it reached
first. The real files are tens of thousands of lines in which none of those is
locatable.

Five sections:

``distances``
    ``haversine_m`` over pairs chosen to reach what the formula is delicate
    about: a degenerate zero-length leg, an antipodal pair where the arcsine's
    argument rounds past one, a polar crossing, and the date line.

``great_circles``
    ``Route.great_circle`` on real city pairs at several sampling densities,
    recording every interpolated waypoint and the cumulative distance at each,
    plus the case where both airports are the same one and the spherical
    interpolation is skipped.

``airways``
    ``find_airway_route`` against the authored navdata, through the real
    ``_load_navdata`` cache and ``_nearest_fix``/``_dijkstra`` path, plus the
    parsed graph itself (every fix and every adjacency) so a disagreement lands
    on the parse rather than on the search.

``kml``
    ``route_from_kml`` on an authored export carrying the malformed tokens a
    real one contains.

``ofp``
    ``fetch_simbrief_route`` against a canned response, with the network call
    replaced for the duration. Both the matching and the mismatched city pair,
    and both settings of ``allow_mismatch``, so the airport-override branch and
    the airport it synthesizes are both recorded.
"""

from __future__ import annotations

import io
import json
import urllib.request
from contextlib import contextmanager
from pathlib import Path

import _framework

_framework.add_alas_to_path()

from alas.config.airports import Airport  # noqa: E402
from alas.integration import assets  # noqa: E402
from alas.routing.kml_import import route_from_kml  # noqa: E402
from alas.routing.navdata_graph import (  # noqa: E402
    _load_navdata,
    find_airway_route,
)
from alas.routing.route import Route, haversine_m  # noqa: E402
from alas.routing.simbrief_route import fetch_simbrief_route  # noqa: E402

INPUTS_DIR = _framework.GOLDEN_DIR / "route" / "inputs"

# An authored fix set. ALPHA..FOXTR is one chain across southern Europe;
# GOLFF/HOTEL form a second path between BRAVO and ECHOO, within a tenth of a
# percent of the first in length, so the search has to weigh two routes rather
# than take whichever it reaches first. BRAVO appears twice -- once in Spain and once in the
# Coral Sea -- which is the duplicate-ident case the module exists to handle.
# LONER is in no airway at all.
FIXES = """\
I
1100 Version - data cycle 2601, authored for the ALAS port's parity fixture

 40.100000  -4.200000 ALPHA
 41.050000  -2.150000 BRAVO
 42.300000   0.400000 CHRLI
 43.150000   2.900000 DELTA
 44.700000   5.600000 ECHOO
 45.900000   8.100000 FOXTR
 41.900000  -1.100000 GOLFF
 43.400000   1.700000 HOTEL
 60.250000  30.750000 LONER
-18.400000 158.200000 BRAVO
99
"""

# `<fix1> <region1> <term1> <fix2> <region2> <term2> <direction> <type>
# <base_fl> <top_fl> <airway_names>`. The GOLFF/HOTEL branch reaches ECHOO in
# four legs where the CHRLI/DELTA one takes three, and is the shorter of the
# two by distance.
AIRWAYS = """\
I
1100 Version - authored for the ALAS port's parity fixture

ALPHA ES 11 BRAVO ES 11 N 1 100 400 UN870
BRAVO ES 11 CHRLI ES 11 N 1 100 400 UN870
CHRLI ES 11 DELTA LF 11 N 1 100 400 UN870
DELTA LF 11 ECHOO LF 11 N 1 100 400 UN870
ECHOO LF 11 FOXTR LI 11 F 1 100 400 UN870
BRAVO ES 11 GOLFF ES 11 N 1 100 400 UM616
GOLFF ES 11 HOTEL LF 11 N 1 100 400 UM616
HOTEL LF 11 ECHOO LF 11 N 1 100 400 UM616
this line is too short to be an airway
99
"""

# A hand-exported route, carrying the things a real export does: a leading
# newline inside the element, a bare altitude-less pair, and a token that is
# not a coordinate at all.
KML = """\
<?xml version="1.0" encoding="UTF-8"?>
<kml xmlns="http://www.opengis.net/kml/2.2"><Document>
  <Placemark><name>LEMD-LFPG</name><LineString><coordinates>
    -3.5626,40.4719,610 -2.1500,41.0500,9500 0.4000,42.3000,11500
    1.7000,43.4000 notacoordinate 2.5500,48.9000,119
  </coordinates></LineString></Placemark>
</Document></kml>
"""

# A canned dispatch response, in the shape the service's XML-to-JSON conversion
# produces: every number a string, and the endpoints carrying elevation and
# runway length in feet.
OFP = {
    "origin": {
        "icao_code": "LEMD",
        "name": "Madrid Barajas",
        "pos_lat": "40.4719",
        "pos_long": "-3.5626",
        "elevation": "1998",
        "plan_rwy_length": "13448",
    },
    "destination": {
        "icao_code": "LFPG",
        "name": "Paris Charles De Gaulle",
        "pos_lat": "49.0097",
        "pos_long": "2.5479",
        "elevation": "392",
        "plan_rwy_length": "13829",
    },
    "navlog": {
        "fix": [
            {"ident": "BARDI", "pos_lat": "40.9500", "pos_long": "-3.2000"},
            {"ident": "PINAR", "pos_lat": "42.1000", "pos_long": "-2.4000"},
            {"ident": "LOMAS", "pos_lat": "44.3000", "pos_long": "-0.8000"},
            {"ident": "TUDRA", "pos_lat": "47.2000", "pos_long": "1.1000"},
        ]
    },
}

# A single-fix navigation log, which the same conversion collapses to a bare
# object rather than a one-element list.
OFP_SINGLE_FIX = {
    "origin": OFP["origin"],
    "destination": OFP["destination"],
    "navlog": {"fix": {"ident": "SOLO1", "pos_lat": "43.0", "pos_long": "-1.0"}},
}


def _airport(icao, name, lat, lon, elevation_m=0.0, toda_m=3500.0, lda_m=3200.0):
    return Airport(
        name=name,
        icao=icao,
        elevation_m=elevation_m,
        toda_m=toda_m,
        lda_m=lda_m,
        isa_deviation_c=15.0,
        notes="authored for the routing parity fixture",
        latitude_deg=lat,
        longitude_deg=lon,
    )


AIRPORTS = {
    "LEMD": _airport("LEMD", "Madrid Barajas", 40.4719, -3.5626, 610.0, 4350.0, 4100.0),
    "LFPG": _airport("LFPG", "Paris CDG", 49.0097, 2.5479, 119.0, 4215.0, 4200.0),
    "LIRF": _airport("LIRF", "Rome Fiumicino", 41.8003, 12.2389, 5.0),
    "SCEL": _airport("SCEL", "Santiago", -33.3930, -70.7858, 474.0),
    "NZAA": _airport("NZAA", "Auckland", -37.0082, 174.7850, 7.0),
    # Beside the authored chain's two ends, so the airway tier reaches it.
    "XALP": _airport("XALP", "Alpha Field", 39.9000, -4.5000),
    "XFOX": _airport("XFOX", "Foxtrot Field", 46.1000, 8.4000),
    "XECH": _airport("XECH", "Echo Field", 44.6000, 5.4000),
    # Sitting on the fix no airway touches, and far from every connected one.
    "XLON": _airport("XLON", "Loner Field", 60.2500, 30.7500),
}

# (name, lat1, lon1, lat2, lon2, why).
DISTANCE_CASES = [
    ("zero", 40.0, -3.0, 40.0, -3.0, "a leg of no length, where the arcsine's argument is zero"),
    ("short_leg", 41.05, -2.15, 42.30, 0.40, "an airway leg, which is what the graph weights are"),
    ("madrid_paris", 40.4719, -3.5626, 49.0097, 2.5479, "a short-haul sector"),
    ("madrid_auckland", 40.4719, -3.5626, -37.0082, 174.7850, "close to antipodal, where the argument rounds toward one"),
    ("antipodal", 0.0, 0.0, 0.0, 180.0, "exactly antipodal, which is where the clamp fires"),
    ("pole_to_pole", 90.0, 0.0, -90.0, 0.0, "over both poles, where the longitudes mean nothing"),
    ("date_line", 35.0, 179.5, 35.0, -179.5, "across the date line, a short leg written as a 359-degree change"),
    ("equator_quarter", 0.0, 0.0, 0.0, 90.0, "a quarter of the equator, which has a closed-form answer"),
]

# (name, origin, dest, n, why).
GREAT_CIRCLE_CASES = [
    ("madrid_paris_50", "LEMD", "LFPG", 50, "the default sampling on a short sector"),
    ("madrid_auckland_50", "LEMD", "NZAA", 50, "a near-antipodal arc at the default sampling"),
    ("madrid_santiago_8", "LEMD", "SCEL", 8, "a coarse sampling, where each waypoint carries more of the arc"),
    ("rome_paris_2", "LIRF", "LFPG", 2, "the fewest intervals that interpolate anything at all"),
    ("same_airport", "LEMD", "LEMD", 50, "both endpoints identical, so the interpolation is skipped entirely"),
]

# (name, origin, dest, why).
AIRWAY_CASES = [
    ("along_the_chain", "XALP", "XFOX", "end to end, so the search crosses the branch point"),
    ("through_the_branch", "XALP", "XECH", "the pair with two paths between them, within a tenth of a percent of each other in length"),
    ("reversed", "XFOX", "XALP", "the same route flown the other way, which must be the same path"),
    ("unreachable_airport", "XALP", "XLON", "an airport beyond the transition limit from every connected fix"),
    ("both_ends_the_same_fix", "XALP", "XALP", "an airport routed to itself, a zero-length path"),
]

# (name, allow_mismatch, requested origin, requested dest, response, why).
OFP_CASES = [
    ("matching_pair", False, "LEMD", "LFPG", OFP, "the plan is for the pair that was asked for"),
    ("mismatch_refused", False, "LIRF", "LFPG", OFP, "a plan for somewhere else, with the override off"),
    ("mismatch_allowed", True, "LIRF", "LFPG", OFP, "the same, with the override on, so the plan's own airports win"),
    ("single_fix", False, "LEMD", "LFPG", OFP_SINGLE_FIX, "a one-entry navigation log, which arrives as a bare object"),
    ("unparseable", True, "LEMD", "LFPG", {"fetch": {"status": "Error: Unknown UserID"}}, "a response of the wrong shape"),
]


@contextmanager
def _canned_response(document):
    """Answer the one network call ``fetch_simbrief_route`` makes.

    Replacing the transport rather than skipping the function is what makes
    this a recording of the real parsing, mismatch and override logic instead
    of a restatement of it.
    """

    class _Response(io.BytesIO):
        def __enter__(self):
            return self

        def __exit__(self, *_exc):
            self.close()
            return False

    original = urllib.request.urlopen
    urllib.request.urlopen = lambda *_a, **_k: _Response(
        json.dumps(document).encode("utf-8")
    )
    try:
        yield
    finally:
        urllib.request.urlopen = original


def _airport_record(airport):
    return {
        "name": airport.name,
        "icao": airport.icao,
        "elevation_m": airport.elevation_m,
        "toda_m": airport.toda_m,
        "lda_m": airport.lda_m,
        "isa_deviation_c": airport.isa_deviation_c,
        "notes": airport.notes,
        "latitude_deg": airport.latitude_deg,
        "longitude_deg": airport.longitude_deg,
    }


def _route_record(route):
    if route is None:
        return None
    return {
        "source": route.source,
        "waypoints": [
            {"lat": w.lat, "lon": w.lon, "alt_m": w.alt_m, "ident": w.ident}
            for w in route.waypoints
        ],
        "cumulative_distance_m": [float(d) for d in route.cumulative_distance_m],
        "total_distance_m": route.total_distance_m,
        "origin_airport": _airport_record(route.origin_airport)
        if route.origin_airport is not None
        else None,
        "dest_airport": _airport_record(route.dest_airport)
        if route.dest_airport is not None
        else None,
    }


def _write_inputs() -> Path:
    """Write the authored routing inputs beside the fixture."""
    INPUTS_DIR.mkdir(parents=True, exist_ok=True)
    for name, text in (
        ("earth_fix.dat", FIXES),
        ("earth_awy.dat", AIRWAYS),
        ("route.kml", KML),
    ):
        (INPUTS_DIR / name).write_text(text, encoding="utf-8", newline="\n")
    (INPUTS_DIR / "ofp.json").write_text(
        json.dumps({"multi_fix": OFP, "single_fix": OFP_SINGLE_FIX}, indent=2, sort_keys=True)
        + "\n",
        encoding="utf-8",
        newline="\n",
    )
    return INPUTS_DIR


def _graph_section() -> dict:
    """The parsed graph itself, so a disagreement lands on the parse."""
    all_fixes, graph, connected, _coords = _load_navdata(INPUTS_DIR)
    return {
        "fixes": [{"ident": f.ident, "lat": f.lat, "lon": f.lon} for f in all_fixes],
        "connected": list(connected),
        "edges": {
            str(node): [[int(other), float(d)] for other, d in neighbors]
            for node, neighbors in sorted(graph.items())
        },
    }


def _assets_section(inputs: Path) -> dict:
    """The optional assets' addresses, locations and size floors.

    The ``detail`` string each status carries is not recorded. It embeds a
    filesystem path, and Python renders a ``Path`` with backslashes on Windows
    where Rust's ``Path::display`` keeps the separator it was given, so the two
    would disagree on the rendering of a path rather than on anything about the
    asset. The verdict is what a caller branches on, and that is compared.
    """
    return {
        "navdata_base_url": assets._NAVDATA_BASE_URL,
        "texture_url": assets._TEXTURE_URL,
        "navdata_rel": assets.NAVDATA_REL,
        "texture_rel": assets.TEXTURE_REL,
        "navdata_files": list(assets._NAVDATA_FILES),
        "min_bytes": dict(assets._MIN_BYTES),
        "min_texture_bytes": assets._MIN_TEXTURE_BYTES,
        # The authored inputs hold two of the three files, which is the partial
        # case: an incomplete set must not report itself usable.
        "status_partial": assets.navdata_status(inputs).available,
        "status_absent": assets.navdata_status(Path("no/such/directory")).available,
        "texture_absent": assets.texture_status(Path("no/such/texture.jpg")).available,
    }


def main() -> None:
    inputs = _write_inputs()

    distances = [
        {
            "name": name,
            "why": why,
            "from": [lat1, lon1],
            "to": [lat2, lon2],
            "distance_m": haversine_m(lat1, lon1, lat2, lon2),
        }
        for name, lat1, lon1, lat2, lon2, why in DISTANCE_CASES
    ]

    great_circles = [
        {
            "name": name,
            "why": why,
            "origin": origin,
            "dest": dest,
            "n": n,
            "route": _route_record(
                Route.great_circle(AIRPORTS[origin], AIRPORTS[dest], n=n)
            ),
        }
        for name, origin, dest, n, why in GREAT_CIRCLE_CASES
    ]

    airways = [
        {
            "name": name,
            "why": why,
            "origin": origin,
            "dest": dest,
            "route": _route_record(
                find_airway_route(
                    AIRPORTS[origin], AIRPORTS[dest], navdata_dir=inputs
                )
            ),
        }
        for name, origin, dest, why in AIRWAY_CASES
    ]

    ofps = []
    for name, allow, origin, dest, document, why in OFP_CASES:
        with _canned_response(document):
            route = fetch_simbrief_route(
                "alasport",
                AIRPORTS[origin],
                AIRPORTS[dest],
                allow_mismatch=allow,
            )
        ofps.append(
            {
                "name": name,
                "why": why,
                "allow_mismatch": allow,
                "origin": origin,
                "dest": dest,
                "document": document,
                "route": _route_record(route),
            }
        )

    payload = {
        "assets": _assets_section(inputs),
        "airports": {icao: _airport_record(a) for icao, a in AIRPORTS.items()},
        "distances": distances,
        "great_circles": great_circles,
        "graph": _graph_section(),
        "airways": airways,
        "kml": _route_record(route_from_kml(inputs / "route.kml")),
        "ofp": ofps,
    }

    duplicated = [f for f in payload["graph"]["fixes"] if f["ident"] == "BRAVO"]
    if len(duplicated) < 2:
        raise SystemExit(
            "the authored fix set no longer names one identifier twice; the "
            "duplicate-ident disambiguation would go unchecked"
        )
    if not any(case["route"] is None for case in payload["airways"]):
        raise SystemExit(
            "every airway case found a route; the transition-limit branch that "
            "makes the caller fall back to a great circle would go unchecked"
        )

    _framework.write(
        "route",
        "route",
        payload,
        description=(
            "alas.routing: haversine distances, Route.great_circle, the parsed "
            "airway graph and find_airway_route over an authored navdata set, "
            "route_from_kml, and fetch_simbrief_route against a canned dispatch "
            "response"
        ),
    )


if __name__ == "__main__":
    main()
