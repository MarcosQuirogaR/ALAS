# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-mission::profile``: the mission half of the SUAVE mission request.

``build_mission_request`` (``alas/integration/suave_mission.py``) is glue: it
reads the cruise altitude off the requirements, the field elevations and the
departure ISA deviation off the two airports, threads the caller's route
distance through, and embeds the mission profile unchanged. There is nothing
to solve, so the fixture's job is to pin the *routing* -- that each value is
read off the airport or config field the reference reads it off, and that the
tag is assembled origin-then-destination -- rather than any arithmetic.

Each case names a preset (``""`` is the default configuration) so the cruise
altitude and requirements vary between them, and two distinct real airports so
a departure/arrival mix-up shows up as a wrong elevation. The Rust parity test
rebuilds the same inputs -- ``AlasConfig::from_value({"preset": name})`` and
``airports::get(icao)`` -- from these codes and compares the request it builds
against ``request`` below.
"""

from __future__ import annotations

import _framework

_framework.add_alas_to_path()

from alas.config.airports import get_airport  # noqa: E402
from alas.config.settings import ALASConfig  # noqa: E402
from alas.integration.suave_mission import build_mission_request  # noqa: E402

# (preset name, origin ICAO, destination ICAO, route distance in metres). The
# airports are chosen with distinct elevations so departure and arrival cannot
# be confused, and the presets so the cruise altitude is not constant.
CASES = [
    ("", "LEMD", "LXGB", 1_480_000.0),
    ("A320-200", "OMDB", "LEMD", 1_264_000.0),
    ("A340-300", "SEQM", "EGLL", 5_540_000.0),
    ("A220-300", "LFPG", "EDDF", 448_000.0),
]


def _case(preset: str, origin_icao: str, dest_icao: str, route_distance_m: float) -> dict:
    config = ALASConfig.from_dict({"preset": preset}) if preset else ALASConfig()
    origin = get_airport(origin_icao)
    dest = get_airport(dest_icao)
    request = build_mission_request(config, origin, dest, route_distance_m)
    return {
        "preset": preset,
        "origin": origin_icao,
        "dest": dest_icao,
        "route_distance_m": route_distance_m,
        "request": request,
    }


def main() -> None:
    cases = [_case(*args) for args in CASES]
    _framework.write(
        "mission",
        "profile",
        {"cases": cases},
        description=(
            "build_mission_request over four preset/route pairs: the mission "
            "half of the SUAVE mission request, checking field routing and the "
            "origin-to-destination tag."
        ),
    )


if __name__ == "__main__":
    main()
