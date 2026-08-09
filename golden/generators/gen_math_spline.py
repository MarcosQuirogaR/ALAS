# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""Cubic spline interpolation, read out of SciPy's ``CubicSpline``.

``alas-math::spline`` has no third-party provenance -- it solves the
classical second-derivative ("moment") tridiagonal system rather than
translating SciPy's own first-derivative formulation -- but the two describe
the same unique spline for the same knots, values and boundary conditions, so
this fixture is SciPy's numbers used as an independent check on a
from-scratch implementation, not a translation target.

Cases cover every combination of a first- or second-derivative condition at
each end (matching what ``Airfoil.repanel`` actually asks for: a
second-derivative-zero condition at one end and a first-derivative condition
at the other), a vector-valued (2-D) dataset like an airfoil coordinate list,
and evaluation both at the knots themselves and at points requiring
extrapolation beyond the data's range.
"""

from __future__ import annotations

import _framework
import numpy as np
from scipy.interpolate import CubicSpline

# A non-uniform knot spacing, so a mistake that only shows up with unequal
# segment lengths (a wrong `h[i-1]` vs. `h[i]` in the tridiagonal row, say)
# has somewhere to appear.
X = [0.0, 0.3, 0.8, 1.5, 2.4, 3.0]

# Scalar dataset.
Y_SCALAR = [0.5, 1.2, 0.9, -0.3, 0.4, 0.1]

# Vector-valued (2-D) dataset, the shape `Airfoil.repanel` actually uses.
Y_VECTOR = [
    [0.0, 0.0],
    [0.05, 0.03],
    [0.2, 0.05],
    [0.55, 0.02],
    [0.9, -0.01],
    [1.0, 0.0],
]

# Query points: every knot exactly (interpolation should reproduce the
# source data), points strictly inside each segment, and points outside
# `[X[0], X[-1]]` on both sides (extrapolation).
QUERY_X = [-0.5, *X, 0.15, 0.5, 1.1, 2.0, 2.7, 3.6]

BOUNDARY_CASES = [
    ("natural", "natural", (2, 0.0), (2, 0.0)),
    ("clamped", "natural", (1, -1.0), (2, 0.0)),
    ("natural", "clamped", (2, 0.0), (1, 2.0)),
    ("clamped", "clamped", (1, 0.7), (1, -0.4)),
]


def _bc_value(order_value, dimension: int):
    """Broadcast a scalar boundary derivative to every dimension of `y`.

    `Airfoil.repanel` uses a different derivative vector per dimension (e.g.
    `(0, -1)`), but a single scalar repeated across dimensions is enough to
    exercise both the 1-D and 2-D cases here without hand-writing every
    per-dimension combination; the linear system treats each dimension
    independently regardless (see `alas-math::spline`'s module doc), so a
    repeated scalar and a genuinely distinct per-dimension vector exercise
    the same code path.
    """

    order, value = order_value
    if dimension == 1:
        return order, value
    return order, [value] * dimension


def main() -> None:
    _framework.add_alas_to_path()

    cases = []
    for y_name, y_data in [("scalar", Y_SCALAR), ("vector", Y_VECTOR)]:
        y = np.array(y_data, dtype=float)
        dimension = 1 if y.ndim == 1 else y.shape[1]

        for lower_name, upper_name, lower_bc, upper_bc in BOUNDARY_CASES:
            # `_bc_value` returns a bare scalar for the 1-D case, which is
            # what SciPy's `CubicSpline` requires there -- but the fixture's
            # JSON schema stays uniform (always a per-dimension list) so the
            # Rust side does not need to special-case scalar vs. vector.
            lower = _bc_value(lower_bc, dimension)
            upper = _bc_value(upper_bc, dimension)
            spline = CubicSpline(X, y, bc_type=(lower, upper))

            evaluated = spline(QUERY_X)
            if evaluated.ndim == 1:
                evaluated = evaluated[:, None]

            cases.append(
                {
                    "y_shape": y_name,
                    "lower": lower_name,
                    "upper": upper_name,
                    "y": y.tolist() if y.ndim > 1 else [[v] for v in y.tolist()],
                    "lower_value": [lower_bc[1]] * dimension,
                    "upper_value": [upper_bc[1]] * dimension,
                    "query_x": QUERY_X,
                    "evaluated": evaluated.tolist(),
                }
            )

    _framework.write(
        "math",
        "spline",
        {"x": X, "cases": cases},
        description=(
            "scipy.interpolate.CubicSpline: every combination of a first- or "
            "second-derivative boundary condition at each end, for a scalar "
            "and a 2-D dataset, evaluated at the knots and beyond both ends"
        ),
    )


if __name__ == "__main__":
    main()
