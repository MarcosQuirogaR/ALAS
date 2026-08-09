# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-geom::airfoil_library``: name resolution and parametric shaping.

Covers ``alas/geometry/airfoils.py``'s ``AirfoilLibrary.get`` (all three
resolution branches), ``AirfoilLibrary.normalize_coordinates``,
``apply_bumps``, ``morph_airfoil`` and ``build_section``.

Cases:

* ``AirfoilLibrary.get`` resolving through each of the three branches, the
  exact three names the default aircraft needs (``docs/PORTING.md``,
  Geometry): ``"naca2410"`` (the Selig zip corpus), ``"SC2-0714"`` (the
  built-in ``NAMED_COORDINATES``), ``"naca0012"`` (AeroSandbox's NACA
  fallback). Each records the resolved name and its normalized coordinates.

* ``normalize_coordinates`` on three synthetic/rearranged inputs that do not
  merely echo already-canonical corpus data: a real corpus section's
  normalized loop, fed back in *reversed* order (exercises the branch where
  the first split segment is the lower surface, not the upper -- the
  ``y1_mid < y2_mid`` branch that a forward-ordered input never reaches), and
  two small hand-built loops that isolate the trailing-edge duplicate check,
  one with the upper/lower trailing-edge points coincident and one with them
  held apart. A generator-side sanity check cross-references the reversed
  case against the forward case: both must normalize to the same loop.

* ``apply_bumps`` on the (normalized) SC2-0714 section: one case with a
  nonzero amplitude at all four bump stations, one with every amplitude at
  zero (checked to equal a plain ``repanel`` -- the zero-amplitude short
  circuit in ``add_bump`` should make bump application a no-op).

* ``morph_airfoil`` on the same section with a representative
  thickness/camber scale pair, both different from 1.

* ``build_section`` end-to-end with a ``DesignVector`` carrying nonzero
  values in all six fields it reads (the four bump amplitudes plus the two
  morph scales), against the SC2-0714 base section -- the root/break section
  ``build_section`` is actually used for (its own docstring says so).
"""

from __future__ import annotations

import _framework
import numpy as np

_framework.add_alas_to_path()

from alas.config.design_variables import DesignVector  # noqa: E402
from alas.geometry.airfoils import (  # noqa: E402
    AirfoilLibrary,
    apply_bumps,
    build_section,
    morph_airfoil,
)


def _coords_to_list(coordinates: np.ndarray) -> list[list[float]]:
    return [[float(x), float(y)] for x, y in coordinates.tolist()]


def _resolve_case(name: str) -> dict:
    airfoil = AirfoilLibrary.get(name)
    if airfoil is None or airfoil.coordinates is None:
        raise SystemExit(f"AirfoilLibrary.get({name!r}) did not resolve")
    return {
        "requested_name": name,
        "resolved_name": airfoil.name,
        "coordinates": _coords_to_list(airfoil.coordinates),
    }


def _normalize_case(coords: list[list[float]]) -> dict:
    array = np.array(coords, dtype=float)
    normalized = AirfoilLibrary.normalize_coordinates(array)
    return {
        "input": coords,
        "output": _coords_to_list(normalized),
    }


def main() -> None:
    get_cases = {
        "naca2410": _resolve_case("naca2410"),  # Selig zip corpus branch
        "SC2-0714": _resolve_case("SC2-0714"),  # NAMED_COORDINATES branch
        "naca0012": _resolve_case("naca0012"),  # AeroSandbox NACA fallback
    }

    # `normalize_coordinates`: a real, already-normalized loop fed back
    # reversed exercises the branch a forward-ordered loop never reaches
    # (which split segment is "upper" flips), without inventing geometry.
    sc2_normalized = np.array(get_cases["SC2-0714"]["coordinates"], dtype=float)
    reversed_input = sc2_normalized[::-1]
    reversed_output = AirfoilLibrary.normalize_coordinates(reversed_input)
    forward_output = AirfoilLibrary.normalize_coordinates(sc2_normalized)
    if not np.allclose(reversed_output, forward_output, atol=1e-12):
        raise SystemExit(
            "normalize_coordinates disagreed between the forward and "
            "reversed orderings of the same loop"
        )

    # Two hand-built diamonds isolating the trailing-edge duplicate check:
    # one where the upper and lower trailing-edge points already coincide
    # (the drop-the-duplicate branch), one where they are held apart (the
    # keep-both branch). Neither is real section data; both are valid inputs
    # to a static method that only looks at (x, y) pairs and an argmin.
    coincident_te = [[1.0, 0.0], [0.5, 0.1], [0.0, 0.0], [0.5, -0.1], [1.0, 0.0]]
    separated_te = [[1.0, 0.05], [0.5, 0.1], [0.0, 0.0], [0.5, -0.1], [1.0, -0.05]]

    normalize_cases = {
        "reversed_sc2_0714": _normalize_case(get_cases["SC2-0714"]["coordinates"][::-1]),
        "coincident_trailing_edge": _normalize_case(coincident_te),
        "separated_trailing_edge": _normalize_case(separated_te),
    }

    base_coords = np.array(get_cases["SC2-0714"]["coordinates"], dtype=float)

    bumps_nonzero = apply_bumps(
        base_coords,
        bumps_upper=[0.0015, -0.003],
        bumps_lower=[0.0022, -0.0035],
    )
    bumps_zero = apply_bumps(base_coords, bumps_upper=[0.0, 0.0], bumps_lower=[0.0, 0.0])
    # Sanity check the generator's own premise: a zero-amplitude bump pass is
    # supposed to be indistinguishable from a plain repanel, because
    # `add_bump` returns its input untouched when `amp == 0.0`.
    from aerosandbox import Airfoil as _AsbAirfoil  # local import: only needed here

    plain_repanel = _AsbAirfoil("check", coordinates=base_coords).repanel(
        n_points_per_side=120
    )
    if not np.allclose(bumps_zero.coordinates, plain_repanel.coordinates, atol=1e-12):
        raise SystemExit(
            "apply_bumps with every amplitude at zero did not match a plain repanel"
        )

    apply_bumps_cases = {
        "nonzero": {
            "base": "SC2-0714",
            "bumps_upper": [0.0015, -0.003],
            "bumps_lower": [0.0022, -0.0035],
            "n_points_per_side": 120,
            "coordinates": _coords_to_list(bumps_nonzero.coordinates),
        },
        "zero": {
            "base": "SC2-0714",
            "bumps_upper": [0.0, 0.0],
            "bumps_lower": [0.0, 0.0],
            "n_points_per_side": 120,
            "coordinates": _coords_to_list(bumps_zero.coordinates),
        },
    }

    morphed = morph_airfoil(base_coords, thickness_scale=1.15, camber_scale=0.85)
    morph_cases = {
        "sc2_0714": {
            "base": "SC2-0714",
            "thickness_scale": 1.15,
            "camber_scale": 0.85,
            "n_points": 150,
            "coordinates": _coords_to_list(morphed.coordinates),
        }
    }

    dv = DesignVector(
        bump_upper_front=0.0015,
        bump_upper_rear=-0.003,
        bump_lower_mid=0.0022,
        bump_lower_rear=-0.0035,
        airfoil_thickness_scale=1.15,
        airfoil_camber_scale=0.85,
    )
    section = build_section(dv, base_coords)
    build_section_case = {
        "base": "SC2-0714",
        "design_vector": {
            "bump_upper_front": dv.bump_upper_front,
            "bump_upper_rear": dv.bump_upper_rear,
            "bump_lower_mid": dv.bump_lower_mid,
            "bump_lower_rear": dv.bump_lower_rear,
            "airfoil_thickness_scale": dv.airfoil_thickness_scale,
            "airfoil_camber_scale": dv.airfoil_camber_scale,
        },
        "coordinates": _coords_to_list(section.coordinates),
    }

    _framework.write(
        "geom",
        "airfoil_library",
        {
            "get": get_cases,
            "normalize_coordinates": normalize_cases,
            "apply_bumps": apply_bumps_cases,
            "morph_airfoil": morph_cases,
            "build_section": build_section_case,
        },
        description=(
            "alas.geometry.airfoils.AirfoilLibrary.get (all three resolution "
            "branches), .normalize_coordinates, apply_bumps, morph_airfoil "
            "and build_section"
        ),
    )


if __name__ == "__main__":
    main()
