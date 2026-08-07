# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""Unit conversion factors, read out of SUAVE's unit table.

SUAVE's ``Units`` is a thin wrapper over pint: ``Units.ft`` evaluates to the
ratio that converts feet into the base SI unit. Every weight correlation in the
mission stack is written in imperial units and brackets itself with these
ratios, so a factor that is wrong in the tenth digit moves an aircraft's empty
weight without anything looking broken.

That is the reason this is the first fixture. The values are not interesting;
being able to prove they were copied rather than remembered is.

The names dumped here are the ones reachable from this program's inputs, found
by reading every ``Units.<name>`` in the mission runner, the weight
correlations, the propulsion sizing and the segment solver.
"""

from __future__ import annotations

import _framework

# Ordered as they appear in the source they were collected from, so that a
# reader comparing the two lists can walk them together.
NAMES = [
    "m",
    "meter",
    "km",
    "ft",
    "feet",
    "inch",
    "inches",
    "nmi",
    "nautical_miles",
    "kg",
    "kilogram",
    "lb",
    "lbs",
    "pounds",
    "lbf",
    "force_pound",
    "N",
    "s",
    "sec",
    "seconds",
    "knots",
    "kts",
    "deg",
    "degrees",
    "degR",
    "pascal",
    "pascals",
    "psi",
    "gallons",
    "horsepower",
    "kW",
    "g",
]


# Values that pin down whether the ratios were read the way SUAVE's own code
# reads them. If multiplying by a unit ever stops meaning "convert into base
# SI", these fail here rather than silently reshaping every imperial weight
# correlation downstream.
EXPECTED = {"ft": 0.3048, "lb": 0.45359237, "nmi": 1852.0, "degrees": 0.017453292519943295}


def main() -> None:
    _framework.add_suave_to_path()
    from SUAVE.Core import Units  # noqa: PLC0415 - needs the path set first

    # Multiplication is how SUAVE converts into base units, and it is the only
    # way to read a ratio out: the registry monkeypatches ``__getattr__`` on
    # its quantities, which breaks ``float()`` and most other inspection. Each
    # ratio is taken from a freshly fetched quantity, because the patched
    # multiply mutates the object it is called on.
    factors = {name: 1.0 * getattr(Units, name) for name in NAMES}

    for name, expected in EXPECTED.items():
        actual = factors[name]
        if abs(actual - expected) > 1e-12 * abs(expected):
            raise SystemExit(f"Units.{name} read as {actual!r}, expected {expected!r}")

    _framework.write(
        "units",
        "factors",
        {"to_base_si": factors},
        description="SUAVE Units ratios to base SI, for every unit reachable from this program",
    )


if __name__ == "__main__":
    main()
