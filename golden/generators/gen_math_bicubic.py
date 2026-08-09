# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""Interpolating bicubic splines on a rectangular grid, read out of SciPy.

``scipy.interpolate.RectBivariateSpline(x, y, z)`` with its defaults --
``kx = ky = 3``, ``s = 0`` -- is how SUAVE turns a table of vortex-lattice
results into the lift and induced-drag surrogates its mission evaluates
against, one call per coefficient per wing. Reproducing it is therefore not
optional, and reproducing it means reproducing FITPACK's ``regrid``: the
answer depends on where the knots are placed, and nothing about that placement
is implied by the phrase "bicubic interpolation".

Three things are captured per case, because a fixture that only held the
evaluated values would leave a wrong-knots/right-values coincidence
indistinguishable from agreement:

* the full knot vectors, which pin down the placement rule itself;
* the tensor-product coefficients, which pin down the interpolation solve;
* values at query points, which pin down the evaluation.

Query points deliberately run outside the data's range on every side. FITPACK
clamps the argument to the boundary knots rather than extrapolating the edge
polynomial, so the surface is constant outside its rectangle -- a property the
mission relies on the moment it asks for a Mach number the training grid does
not cover, and one that a from-scratch implementation would not arrive at by
accident.
"""

from __future__ import annotations

import _framework
import numpy as np
from scipy.interpolate import RectBivariateSpline

# SUAVE's own training grid, `SUAVE.Analyses.Aerodynamics.Vortex_Lattice`:
# ten angles of attack in radians against the subsonic half of its Mach list.
# The surrogate this module exists to serve is built on exactly this grid, so
# one case uses it rather than a shape chosen for the test's convenience.
SUAVE_AOA_DEG = [-5.0, -2.0, 0.0, 2.0, 5.0, 8.0, 10.0, 12.0, 45.0, 75.0]
SUAVE_MACH_SUB = [0.0, 0.1, 0.2, 0.3, 0.5, 0.75, 0.85, 0.9]


def _suave_lift_surface(aoa: np.ndarray, mach: np.ndarray) -> np.ndarray:
    """A smooth stand-in for the vortex-lattice lift table.

    The point of this case is the grid's shape and spacing, not the physics,
    so the values come from a closed form rather than from a VLM run: a
    lift-curve slope with a Prandtl-Glauert compressibility factor, which has
    the right curvature in the Mach direction to make a wrong knot placement
    visible. Generating it here keeps the fixture reproducible without
    standing up a whole vehicle.
    """
    beta = np.sqrt(1.0 - np.square(mach))
    return 0.2 + np.outer(np.sin(aoa) * 5.0, 1.0 / beta)


def _case(name: str, x: list[float], y: list[float], z: np.ndarray) -> dict:
    spline = RectBivariateSpline(np.asarray(x), np.asarray(y), z)
    tx, ty = spline.tck[0], spline.tck[1]

    query_x, query_y = _query_points(x, y)
    evaluated = spline(query_x, query_y, grid=False)

    return {
        "name": name,
        "x": list(x),
        "y": list(y),
        "z": z.tolist(),
        "tx": tx.tolist(),
        "ty": ty.tolist(),
        "coefficients": spline.get_coeffs().reshape(len(x), len(y)).tolist(),
        "query_x": query_x.tolist(),
        "query_y": query_y.tolist(),
        "evaluated": evaluated.tolist(),
    }


def _query_points(x: list[float], y: list[float]) -> tuple[np.ndarray, np.ndarray]:
    """Every grid node, every cell centre, and points off all four sides.

    The grid nodes are what interpolation must reproduce exactly; the cell
    centres are where a wrong knot vector shows up as a smooth, plausible and
    wrong surface; the outside points are where the clamping described in the
    module docstring either happens or does not.
    """
    span_x = x[-1] - x[0]
    span_y = y[-1] - y[0]

    nodes_x, nodes_y = np.meshgrid(x, y, indexing="ij")
    inner_x = 0.5 * (np.asarray(x[:-1]) + np.asarray(x[1:]))
    inner_y = 0.5 * (np.asarray(y[:-1]) + np.asarray(y[1:]))
    centres_x, centres_y = np.meshgrid(inner_x, inner_y, indexing="ij")

    middle_x = 0.5 * (x[0] + x[-1])
    middle_y = 0.5 * (y[0] + y[-1])
    outside = [
        (x[0] - 0.3 * span_x, middle_y),
        (x[-1] + 0.3 * span_x, middle_y),
        (middle_x, y[0] - 0.3 * span_y),
        (middle_x, y[-1] + 0.3 * span_y),
        (x[0] - 0.3 * span_x, y[0] - 0.3 * span_y),
        (x[-1] + 0.3 * span_x, y[-1] + 0.3 * span_y),
    ]

    query_x = np.concatenate(
        [nodes_x.ravel(), centres_x.ravel(), [point[0] for point in outside]]
    )
    query_y = np.concatenate(
        [nodes_y.ravel(), centres_y.ravel(), [point[1] for point in outside]]
    )
    return query_x, query_y


def main() -> None:
    cases = []

    # The smallest grid a bicubic admits: four points a side leaves no
    # interior knot at all, so the whole surface is a single polynomial patch
    # and the knot-placement loop runs zero times.
    minimal_x = [0.0, 1.0, 2.0, 3.0]
    minimal_y = [-1.0, 0.5, 1.0, 2.5]
    minimal_z = np.outer(
        np.asarray(minimal_x) ** 2 - 1.0, np.exp(0.4 * np.asarray(minimal_y))
    )
    cases.append(_case("minimal", minimal_x, minimal_y, minimal_z))

    # Unequal counts in the two directions, unequally spaced in both, so an
    # implementation that transposed the grid or reused one direction's knots
    # for the other cannot pass by symmetry.
    rect_x = [0.0, 0.5, 1.0, 2.0, 3.5, 4.0]
    rect_y = [0.0, 1.0, 2.0, 4.0, 5.0]
    rect_z = np.outer(np.sin(rect_x), np.cos(np.asarray(rect_y) * 0.5)) + 0.1 * np.outer(
        np.asarray(rect_x) ** 2, rect_y
    )
    cases.append(_case("rectangular", rect_x, rect_y, rect_z))

    # The same asymmetry the other way round, which catches the half of a
    # swapped-axis mistake the case above would let through.
    tall_x = [0.0, 0.25, 0.75, 1.0]
    tall_y = [-2.0, -1.5, -0.5, 0.0, 1.0, 2.5, 3.0]
    tall_z = np.outer(1.0 + np.asarray(tall_x) * 3.0, np.tanh(np.asarray(tall_y)))
    cases.append(_case("tall", tall_x, tall_y, tall_z))

    aoa = np.radians(SUAVE_AOA_DEG)
    mach = np.asarray(SUAVE_MACH_SUB)
    cases.append(
        _case(
            "suave_lift_surrogate",
            aoa.tolist(),
            mach.tolist(),
            _suave_lift_surface(aoa, mach),
        )
    )

    _framework.write(
        "math",
        "bicubic",
        {"cases": cases},
        description=(
            "scipy.interpolate.RectBivariateSpline with kx=ky=3 and s=0: "
            "FITPACK knot placement, tensor-product coefficients and values "
            "at the nodes, at cell centres and beyond every side, for grids "
            "from the 4x4 minimum up to SUAVE's own 10x8 surrogate training "
            "grid"
        ),
    )


if __name__ == "__main__":
    main()
