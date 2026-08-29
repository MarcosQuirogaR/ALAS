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

* ``normalize(return_dict=True)`` -- the frame every airfoil-surrogate query
  starts in, and this row's newest scope. ``alas-aero::neuralfoil`` is its
  only caller, and it uses all four reported numbers, not just the moved
  section: the translation corrects the moment coefficient, the scale divides
  the Reynolds number, and the rotation offsets the angle of attack. The
  cases are chosen so that each of the four is driven away from identity by
  something, because a port that returned the section and forgot one of the
  numbers would agree with a fixture built only from sections that are
  already normalized -- which is most of them. ``_assert_normalize_branches``
  refuses to write a fixture in which that has stopped being true.
"""

from __future__ import annotations

import _framework
import numpy as np

_framework.add_alas_to_path()

from aerosandbox.geometry.airfoil.airfoil import Airfoil  # noqa: E402
from aerosandbox.geometry.airfoil.airfoil_families import (  # noqa: E402
    get_NACA_coordinates,
)
from alas.geometry.airfoils import AirfoilLibrary  # noqa: E402

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


# Sections whose stored coordinates are *not* already in the standard frame,
# so that normalizing them actually has work to do. Every one is a section
# some part of this program can hand to the airfoil surrogate:
# `SC2-0714` is the default aircraft's root, the next four are database
# entries an airfoil sweep walks over, and `30p-30n` is the extreme case --
# a three-element high-lift deck whose chord is 1.51 and whose stored
# incidence is nearly three degrees.
NORMALIZE_CASES = ["SC2-0714", "e63", "sd7037", "s9104", "goe775", "30p-30n"]


def _source_for_normalize(name: str) -> Airfoil:
    """The section as this program would hand it over.

    ``AirfoilLibrary.get`` is the accessor every ``alas`` call site uses, and
    it resolves through the Selig archive, the built-in named coordinates and
    AeroSandbox's NACA generator in that order -- so a case named here is the
    coordinates the surrogate would actually see, not a re-parse of one file.
    """
    return AirfoilLibrary.get(name)


def _assert_normalize_branches(cases: dict) -> None:
    """Refuse a fixture in which normalization is indistinguishable from a copy.

    ``normalize`` reports four numbers and moves the section, and on a section
    that is already in the standard frame all four are zero or one and the
    coordinates come back unchanged. A fixture built only from such sections
    -- which is most of the NACA family, and a good deal of the database --
    would pass against a port that had forgotten the rotation, or the scaling,
    or that reported the translation with the wrong sign.
    """
    for field, identity in (
        ("x_translation", 0.0),
        ("y_translation", 0.0),
        ("scale_factor", 1.0),
        ("rotation_angle", 0.0),
    ):
        reached = [n for n, c in cases.items() if c[field] != identity]
        if not reached:
            raise SystemExit(
                f"no normalize case moves {field} off its identity value "
                f"({identity}); the fixture cannot tell a port that dropped it "
                "from one that did not"
            )


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

    normalize = {}
    for name in NORMALIZE_CASES:
        source = _source_for_normalize(name)
        result = source.normalize(return_dict=True)
        normalize[name] = {
            "input": _coords_to_list(source.coordinates),
            "coordinates": _coords_to_list(result["airfoil"].coordinates),
            "x_translation": float(result["x_translation"]),
            "y_translation": float(result["y_translation"]),
            "scale_factor": float(result["scale_factor"]),
            "rotation_angle": float(result["rotation_angle"]),
        }
    _assert_normalize_branches(normalize)

    _framework.write(
        "geom",
        "asb_airfoil",
        {
            "naca": naca,
            "surfaces": surfaces,
            "thickness": thickness,
            "repanel": repanel,
            "blends": blends,
            "normalize": normalize,
        },
        description=(
            "aerosandbox.geometry.airfoil.Airfoil, scoped to NACA "
            "generation, upper/lower surface split, repanel (through "
            "scipy.interpolate.CubicSpline), local/max thickness, "
            "blend_with_another_airfoil, and normalize(return_dict=True) on "
            "six sections that are not already in the standard frame"
        ),
    )


if __name__ == "__main__":
    main()
