# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-mission::numerics``: the pseudospectral scaffolding a segment solves on.

Three things the port has to reproduce, and this fixture records each from the
real SUAVE objects rather than a reimplementation:

``defaults``
    ``Numerics.__defaults__``: the control-point count, the solution tolerance,
    the Jacobian mode and the evaluation budget the container starts at, and its
    tag. Copied constants, checked at ``exact``.

``dimensionless``
    ``initialize_differentials_dimensionless`` at the default sixteen points:
    the Chebyshev node grid and the differentiation and integration operators on
    ``[0, 1]``. These ride ``chebyshev_data``'s matrix inverse, so they are
    ``linalg``.

``time_cases``
    ``update_differentials_time`` over two segment durations: the same operators
    rescaled onto a real time span (nodes by ``T``, differentiation by ``1/T``,
    integration by ``T``). Only the time array's endpoints are read, but the
    whole array is recorded so the Rust test feeds the identical input.

Runs under the SUAVE interpreter (``.suave-venv``): the functions are driven on
a minimal segment whose ``state.numerics`` is a real ``Numerics`` and whose
inertial time is set by hand, which is all the two functions touch.
"""

from __future__ import annotations

import types

import _framework

_framework.add_suave_to_path()

import numpy as np  # noqa: E402

from SUAVE.Analyses.Mission.Segments.Conditions.Numerics import Numerics  # noqa: E402
from SUAVE.Methods.Missions.Segments.Common.Numerics import (  # noqa: E402
    initialize_differentials_dimensionless,
    update_differentials_time,
)

# Two realistic segment durations, in seconds. Only the endpoints of each time
# array are read; the arrays span them over the sixteen control points so the
# recorded input is what a real segment would carry.
DURATIONS_S = [720.0, 3600.0]


def _segment(time=None) -> types.SimpleNamespace:
    """A minimal stand-in carrying just what the two functions read."""
    segment = types.SimpleNamespace()
    segment.state = types.SimpleNamespace()
    segment.state.numerics = Numerics()
    inertial = types.SimpleNamespace()
    inertial.time = time
    segment.state.conditions = types.SimpleNamespace(
        frames=types.SimpleNamespace(inertial=inertial)
    )
    return segment


def _flat(array) -> list:
    return [float(v) for v in np.asarray(array).reshape(-1)]


def _matrix(array) -> list:
    return [[float(v) for v in row] for row in np.asarray(array)]


def main() -> None:
    defaults_source = Numerics()
    defaults = {
        "number_control_points": int(defaults_source.number_control_points),
        "tolerance_solution": float(defaults_source.tolerance_solution),
        "solver_jacobian": str(defaults_source.solver_jacobian),
        "max_evaluations": float(defaults_source.max_evaluations),
        "tag": str(defaults_source.tag),
    }

    segment = _segment()
    initialize_differentials_dimensionless(segment)
    dimensionless = {
        "control_points": _flat(segment.state.numerics.dimensionless.control_points),
        "differentiate": _matrix(segment.state.numerics.dimensionless.differentiate),
        "integrate": _matrix(segment.state.numerics.dimensionless.integrate),
    }

    time_cases = []
    for duration in DURATIONS_S:
        seg = _segment()
        initialize_differentials_dimensionless(seg)
        node = seg.state.numerics.dimensionless.control_points  # column, in [0, 1]
        time = node * duration  # a real inertial-time column spanning the duration
        seg.state.conditions.frames.inertial.time = time
        update_differentials_time(seg)
        time_cases.append(
            {
                "time": _flat(time),
                "span": float(np.asarray(time)[-1] - np.asarray(time)[0]),
                "control_points": _flat(seg.state.numerics.time.control_points),
                "differentiate": _matrix(seg.state.numerics.time.differentiate),
                "integrate": _matrix(seg.state.numerics.time.integrate),
            }
        )

    _framework.write(
        "mission",
        "numerics",
        {
            "defaults": defaults,
            "dimensionless": dimensionless,
            "time_cases": time_cases,
        },
        description=(
            "SUAVE Numerics defaults and the Chebyshev differentiation/"
            "integration operators, dimensionless and rescaled onto two segment "
            "durations."
        ),
    )


if __name__ == "__main__":
    main()
