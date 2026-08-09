# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""Not-a-knot cubic B-spline interpolation, read out of CasADi and SciPy.

``alas-math::bspline`` has no third-party provenance -- it solves the
collocation system directly rather than translating anyone's routine -- but
the object it builds has to be the one AeroSandbox's default atmosphere is
made of, and that one is built by CasADi's ``interpolant(..., "bspline", ...)``
reached through ``aerosandbox.numpy.interpn``. Two independent things are
therefore recorded per case:

* ``evaluated`` -- what CasADi's interpolant returns. This is the upstream
  object itself, and is what the port has to reproduce.

* ``knots`` and ``coefficients`` -- SciPy's ``make_interp_spline(x, y, k=3)``,
  which builds the same not-a-knot spline and, unlike CasADi, hands back its
  B-spline representation. The knot placement is a choice rather than a
  computation: a cubic through the same points with the knots one data point
  over still reproduces every data value and disagrees everywhere between
  them, which is most of where anything samples. Comparing values alone would
  not distinguish the two, so the structure is compared as well -- the same
  reasoning ``gen_math_bicubic.py`` records for FITPACK's knots.

That the two agree at all is itself the finding this fixture rests on, so it
is checked here rather than assumed: CasADi's evaluated values and SciPy's
spline agree to about 1e-15 of the data's scale on every case below.

The last case is the one that matters -- the thirty-eight altitudes
``aerosandbox/atmosphere/_diff_atmo_functions.py`` fits the ISA at, spanning
-5,000 km to 2,087 km, with knot spacings three orders of magnitude apart. A
spline construction that is accurate on a tidy uniform grid and loses digits
on that one would pass a prettier fixture and fail the only case in the
program.
"""

from __future__ import annotations

import _framework
import casadi as cas
import numpy as np
from scipy.interpolate import make_interp_spline

# A non-uniform spacing, so a mistake that only shows up with unequal knot
# spans has somewhere to appear.
GENERIC_X = [0.0, 0.3, 0.8, 1.5, 2.4, 3.0, 4.1]
GENERIC_Y = [0.5, 1.2, 0.9, -0.3, 0.4, 0.1, 0.8]

# Exactly four points: the smallest a cubic admits, and the case with no
# interior knots at all, where the whole range is one polynomial patch.
SMALLEST_X = [1.0, 2.0, 3.5, 4.0]
SMALLEST_Y = [-1.0, 0.5, 0.25, 2.0]


def _query_points(x: list[float]) -> list[float]:
    """Every data point, plus two interior points of each span.

    The data points check interpolation; the interior points are where a
    wrong knot vector shows up, since a spline with any knot placement
    reproduces the data it was built from.
    """
    points = list(x)
    for lower, upper in zip(x, x[1:]):
        points.append(lower + 0.25 * (upper - lower))
        points.append(lower + 0.75 * (upper - lower))
    return sorted(points)


def _case(name: str, x, y, description: str) -> dict:
    x = np.asarray(x, dtype=float)
    y = np.asarray(y, dtype=float)

    interpolant = cas.interpolant("Interpolator", "bspline", [x], list(y))
    spline = make_interp_spline(x, y, k=3)

    query = _query_points(list(x))
    evaluated = [float(interpolant(float(point))) for point in query]

    # The premise of the fixture, checked rather than asserted in prose: the
    # two implementations describe the same spline.
    scipy_values = spline(query)
    scale = max(float(np.abs(scipy_values).max()), 1e-300)
    disagreement = float(np.abs(np.array(evaluated) - scipy_values).max()) / scale
    if disagreement > 1e-12:
        raise SystemExit(
            f"{name}: CasADi and SciPy disagree by {disagreement:.3e} of the "
            "data's scale, so they are not building the same spline and this "
            "fixture's two halves cannot both be the reference"
        )

    return {
        "name": name,
        "description": description,
        "x": x.tolist(),
        "y": y.tolist(),
        "query_x": query,
        "evaluated": evaluated,
        "knots": spline.t.tolist(),
        "coefficients": spline.c.tolist(),
        "casadi_scipy_disagreement": disagreement,
    }


def main() -> None:
    _framework.add_alas_to_path()

    from aerosandbox.atmosphere._diff_atmo_functions import (
        altitude_knot_points,
        pressure_knot_points,
        temperature_knot_points,
    )

    cases = [
        _case(
            "generic",
            GENERIC_X,
            GENERIC_Y,
            "seven non-uniformly spaced points with no structure to lean on",
        ),
        _case(
            "smallest",
            SMALLEST_X,
            SMALLEST_Y,
            "four points: no interior knots, one polynomial patch",
        ),
        _case(
            "atmosphere_temperature",
            altitude_knot_points,
            temperature_knot_points,
            "the ISA temperature at the thirty-eight altitudes AeroSandbox's "
            "differentiable atmosphere is fitted at",
        ),
        _case(
            "atmosphere_log_pressure",
            altitude_knot_points,
            np.log(pressure_knot_points),
            "the log of the ISA pressure at the same thirty-eight altitudes; "
            "the fit is built in log space and exponentiated on evaluation",
        ),
    ]

    _framework.write(
        "math",
        "bspline",
        {"cases": cases},
        description=(
            "CasADi's 1-D 'bspline' interpolant, with SciPy's not-a-knot knot "
            "vector and coefficients for the same data as a structural check; "
            "includes the altitude grid AeroSandbox's default atmosphere uses"
        ),
    )


if __name__ == "__main__":
    main()
