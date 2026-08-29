# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-aero::operating_point``: AeroSandbox's ``OperatingPoint``, scoped to
the surface ``vortex_lattice_method.py``'s ``VortexLatticeMethod.run`` (the
only consumer besides ``alas/physics/aerodynamics.py``, which only
constructs one and reads no method off it) actually reaches: the seven
constructor fields, ``dynamic_pressure``, ``compute_freestream_velocity_geometry_axes``,
``compute_rotation_velocity_geometry_axes`` and ``convert_axes``.

Cases cover a plain positive-alpha condition, a sideslipping one, one with
all three rotation rates nonzero (the only way ``compute_rotation_velocity_geometry_axes``
does anything but return the freestream-velocity's own negation), and a
negative-alpha/negative-beta/negative-rate condition so a flipped sign in any
of the trig terms would be caught rather than cancelled by symmetry.

``convert_axes`` is exercised only for the axis pairs
``vortex_lattice_method.py`` actually calls it with -- geometry -> body and
body -> wind, confirmed by grepping every ``op_point.convert_axes(...)`` call
in that file (there are exactly four, two of each pair, on forces and on
moments). ``from_axes="stability"``/``to_axes="stability"`` is never reached
from that call site; the Rust port still translates all four branches (cheap,
symmetric algebra), and their correctness is checked by unit tests on
properties that hold everywhere -- round-tripping through stability axes is
the identity, and every rotation preserves vector length -- rather than by
this fixture.
"""

from __future__ import annotations

import aerosandbox as asb
import numpy as np

import _framework


def _vec3(v) -> list[float]:
    return [float(v[0]), float(v[1]), float(v[2])]


# A handful of points, none on an axis of symmetry, so a rotation-rate cross
# product that dropped a term would still show up in every component.
POINTS = [
    [0.0, 0.0, 0.0],
    [2.0, 0.5, -1.0],
    [-3.5, 4.0, 0.75],
]

# geometry -> body and body -> wind, the two pairs `VortexLatticeMethod.run`
# calls `convert_axes` with (on forces and on moments); see the module doc.
CONVERT_AXES_PAIRS = [("geometry", "body"), ("body", "wind")]

CONVERT_AXES_VECTORS = [
    [1.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, 0.0, 1.0],
    [2.0, -3.0, 5.5],
]

CASES = {
    "baseline": dict(altitude_m=0.0, velocity=100.0, alpha=5.0, beta=0.0, p=0.0, q=0.0, r=0.0),
    "sideslip": dict(altitude_m=3000.0, velocity=120.0, alpha=3.0, beta=4.0, p=0.0, q=0.0, r=0.0),
    "rotation_rates": dict(
        altitude_m=8000.0, velocity=80.0, alpha=2.0, beta=-3.0, p=0.01, q=0.02, r=0.03
    ),
    "negative": dict(
        altitude_m=11000.0, velocity=250.0, alpha=-5.0, beta=-6.0, p=-0.02, q=0.05, r=-0.01
    ),
}


def _case_payload(params: dict) -> dict:
    atmo = asb.Atmosphere(altitude=params["altitude_m"])
    op = asb.OperatingPoint(
        atmosphere=atmo,
        velocity=params["velocity"],
        alpha=params["alpha"],
        beta=params["beta"],
        p=params["p"],
        q=params["q"],
        r=params["r"],
    )

    rotation_velocities = op.compute_rotation_velocity_geometry_axes(np.array(POINTS))

    convert_axes_results = []
    for from_axes, to_axes in CONVERT_AXES_PAIRS:
        for vector in CONVERT_AXES_VECTORS:
            x, y, z = op.convert_axes(
                vector[0], vector[1], vector[2], from_axes=from_axes, to_axes=to_axes
            )
            convert_axes_results.append(
                {
                    "from_axes": from_axes,
                    "to_axes": to_axes,
                    "vector": vector,
                    "result": [float(x), float(y), float(z)],
                }
            )

    return {
        "inputs": params,
        "dynamic_pressure": float(op.dynamic_pressure()),
        "freestream_velocity_geometry_axes": _vec3(
            op.compute_freestream_velocity_geometry_axes()
        ),
        "rotation_velocity_geometry_axes": [
            _vec3(row) for row in rotation_velocities
        ],
        "convert_axes": convert_axes_results,
    }


def main() -> None:
    cases = {name: _case_payload(params) for name, params in CASES.items()}

    # Sanity check on the generator's own extraction: the freestream velocity
    # is the freestream direction (a unit vector) times the true airspeed, so
    # its norm should recover `velocity` regardless of alpha/beta.
    for name, params in CASES.items():
        v = cases[name]["freestream_velocity_geometry_axes"]
        norm = (v[0] ** 2 + v[1] ** 2 + v[2] ** 2) ** 0.5
        if abs(norm - params["velocity"]) > 1e-9 * params["velocity"]:
            raise SystemExit(
                f"case {name}: freestream velocity norm {norm!r} does not recover "
                f"velocity {params['velocity']!r}"
            )

    _framework.write(
        "aero",
        "operating_point",
        {"cases": cases, "points": POINTS},
        description=(
            "aerosandbox.performance.OperatingPoint, scoped to dynamic_pressure, "
            "compute_freestream_velocity_geometry_axes, "
            "compute_rotation_velocity_geometry_axes and convert_axes (geometry->body, "
            "body->wind) across four flight conditions"
        ),
    )


if __name__ == "__main__":
    main()
