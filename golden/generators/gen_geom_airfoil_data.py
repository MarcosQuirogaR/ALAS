# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``NAMED_COORDINATES``, the built-in reference-section registry.

``alas/data/airfoil_data.py`` hardcodes one supercritical section,
``COORDS_SC2_0714`` (the root/break section of the reference wing), and
registers it under ``"SC2-0714"`` in ``NAMED_COORDINATES`` -- a dict from
built-in section name to its raw ``(x, y)`` coordinate array, looked up by
exact name (unlike ``alas-geom::selig``'s case-insensitive corpus lookup,
because this is a plain Python dict, not a case-folded index).

This generator records every name in the registry and its full coordinate
array, at ``exact`` tier: the data is literal, so there is no arithmetic
for a tolerance to forgive.
"""

from __future__ import annotations

import _framework

_framework.add_alas_to_path()

from alas.data import airfoil_data  # noqa: E402


def main() -> None:
    named = {
        name: [[float(x), float(y)] for x, y in coords.tolist()]
        for name, coords in airfoil_data.NAMED_COORDINATES.items()
    }

    if "SC2-0714" not in named:
        raise SystemExit("expected 'SC2-0714' in NAMED_COORDINATES")
    # A closed Selig loop: starts and ends at the trailing edge (x == 1.0),
    # and passes through the leading edge (x == 0.0) somewhere in between.
    sc2_0714 = named["SC2-0714"]
    if sc2_0714[0][0] != 1.0 or sc2_0714[-1][0] != 1.0:
        raise SystemExit("SC2-0714 does not start and end at the trailing edge")
    if not any(x == 0.0 for x, _ in sc2_0714):
        raise SystemExit("SC2-0714 has no leading-edge point")

    _framework.write(
        "geom",
        "airfoil_data",
        {"named_coordinates": named},
        description=(
            "Every name in alas.data.airfoil_data.NAMED_COORDINATES and its "
            "full (x, y) coordinate array, in file order"
        ),
    )


if __name__ == "__main__":
    main()
