# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-geom::asb::airfoil``: AeroSandbox's ``Airfoil``, scoped surface.

Covers the pieces ``docs/PORTING.md`` names for this row: NACA generation
(``get_NACA_coordinates``), ``upper_coordinates``/``lower_coordinates``,
``repanel`` (through ``scipy.interpolate.CubicSpline``, the reason this row
is ``linalg`` rather than ``closed``), and ``local_thickness``/
``max_thickness``.

Cases:

* NACA generation for ``naca0012`` (symmetric, the section this program's
  presets actually resolve through this fallback) and ``naca2412``
  (cambered, to exercise the piecewise camber-line branch), each at the
  default ``n_points_per_side=200``, plus ``naca2412`` at a non-default
  count (50) to check that parameter is threaded through correctly.

* ``repanel`` of ``naca2412``'s default 200-point section down to 80 points
  per side (``mses_analysis.py``'s actual call) and up to 120
  (``airfoils.py``'s ``apply_bumps`` default), recording both the input and
  output coordinate arrays so the Rust test does not have to reproduce NACA
  generation to set up the repanel case.

* ``upper_coordinates``/``lower_coordinates`` on both NACA sections above.

* ``local_thickness`` at a handful of ``x/c`` stations and ``max_thickness``
  at its upstream default sample grid (``np.linspace(0, 1, 101)``), for both
  sections. Every airfoil this generator builds already spans the full
  ``[0, 1]`` chord (the NACA formula's cosine-spaced ``x_t`` runs exactly
  0 to 1), so ``max_thickness``'s default sample grid never actually queries
  outside a surface's own data range -- there is nothing here that would
  pin down ``numpy.interp``'s clamping behaviour that the Rust side's own
  unit test (a synthetic probe shape, deliberately queried out of range)
  does not already cover more directly.

* ``blend_with_another_airfoil`` of the two 200-point NACA sections above, at
  a 50/50 blend and a non-50/50 (30/70) blend, each recording the resulting
  name and coordinate array -- ``alas-geom::asb::wing``'s
  ``subdivide_sections`` is this method's only caller in this program, always
  at the default ``n_points_per_side=100``.
"""

from __future__ import annotations

import _framework
import numpy as np
from aerosandbox.geometry.airfoil.airfoil import Airfoil
from aerosandbox.geometry.airfoil.airfoil_families import get_NACA_coordinates

NACA_CASES = [
    ("naca0012", 200),
    ("naca2412", 200),
    ("naca2412", 50),
]

REPANEL_TARGETS = [80, 120]

THICKNESS_STATIONS = [0.0, 0.0125, 0.05, 0.1, 0.25, 0.5, 0.75, 0.9, 0.99, 1.0]

DEFAULT_MAX_THICKNESS_SAMPLE = np.linspace(0, 1, 101)


def _coords_to_list(coordinates: np.ndarray) -> list[list[float]]:
    return [[float(x), float(y)] for x, y in coordinates.tolist()]


def main() -> None:
    naca = {}
    airfoils: dict[str, Airfoil] = {}
    for name, n_points_per_side in NACA_CASES:
        coordinates = get_NACA_coordinates(name=name, n_points_per_side=n_points_per_side)
        key = f"{name}_{n_points_per_side}"
        naca[key] = {
            "name": name,
            "n_points_per_side": n_points_per_side,
            "coordinates": _coords_to_list(coordinates),
        }
        if n_points_per_side == 200:
            airfoils[name] = Airfoil(name=name, coordinates=coordinates)

    surfaces = {}
    thickness = {}
    for name, airfoil in airfoils.items():
        surfaces[name] = {
            "upper": _coords_to_list(airfoil.upper_coordinates()),
            "lower": _coords_to_list(airfoil.lower_coordinates()),
        }
        thickness[name] = {
            "x_over_c": THICKNESS_STATIONS,
            "local_thickness": [
                float(v)
                for v in airfoil.local_thickness(x_over_c=np.array(THICKNESS_STATIONS))
            ],
            "max_thickness": float(
                airfoil.max_thickness(x_over_c_sample=DEFAULT_MAX_THICKNESS_SAMPLE)
            ),
        }

    repanel = {}
    base_name = "naca2412"
    base_airfoil = airfoils[base_name]
    for n_points_per_side in REPANEL_TARGETS:
        repaneled = base_airfoil.repanel(n_points_per_side=n_points_per_side)
        repanel[f"{base_name}_to_{n_points_per_side}"] = {
            "source": base_name,
            "n_points_per_side": n_points_per_side,
            "input": _coords_to_list(base_airfoil.coordinates),
            "output": _coords_to_list(repaneled.coordinates),
        }

    blends = {}
    foil_a = airfoils["naca0012"]
    foil_b = airfoils["naca2412"]
    for key, blend_fraction in [("50_50", 0.5), ("30_70", 0.7)]:
        blended = foil_a.blend_with_another_airfoil(
            airfoil=foil_b, blend_fraction=blend_fraction
        )
        blends[key] = {
            "airfoil_a": "naca0012",
            "airfoil_b": "naca2412",
            "blend_fraction": blend_fraction,
            "name": blended.name,
            "coordinates": _coords_to_list(blended.coordinates),
        }

    _framework.write(
        "geom",
        "asb_airfoil",
        {
            "naca": naca,
            "surfaces": surfaces,
            "thickness": thickness,
            "repanel": repanel,
            "blends": blends,
        },
        description=(
            "aerosandbox.geometry.airfoil.Airfoil, scoped to NACA "
            "generation, upper/lower surface split, repanel (through "
            "scipy.interpolate.CubicSpline), local/max thickness, and "
            "blend_with_another_airfoil"
        ),
    )


if __name__ == "__main__":
    main()
