# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-aero::singularities``: AeroSandbox's
``calculate_induced_velocity_horseshoe``, the horseshoe-vortex kernel
``VortexLatticeMethod.run`` calls to assemble its AIC matrix and its
near-field velocities.

Every case uses ``vortex_core_radius=1e-8``, the one value this program's
call sites ever reach (``VortexLatticeMethod``'s constructor default, never
overridden by ``alas/physics/aerodynamics.py`` or ``alas/physics/stability.py``
-- see ``alas-aero::asb_vlm``'s module doc for the grep that confirms it).

``single_vortex`` cases probe the kernel's four singular terms directly:

* ``away_from_legs`` -- the reference implementation's own ``__main__`` demo
  point, nowhere near either leg, as a baseline sanity check.
* ``near_bound_leg`` -- a field point essentially on the bound leg's own
  line (between its two vertices), which drives ``term1``'s denominator
  (``norm_a * norm_b + a_dot_b``) toward zero.
* ``on_trailing_leg_from_left`` -- a field point on the extension of the
  left vertex's trailing leg, which drives ``term2``'s denominator
  (``norm_a - a_dot_u``) to exactly zero.
* ``near_right_vertex`` -- a field point a millionth of a unit from the
  right vertex, which drives ``norm_b`` itself toward zero (the direct
  smoothing every term depends on through ``norm_b_inv``).

``multi_vortex_broadcast`` sums two horseshoes of different strength at
three field points -- the broadcast-and-sum pattern
``VortexLatticeMethod.run``'s AIC assembly and ``get_velocity_at_points``
both rely on, translated in the Rust port as an explicit loop rather than
NumPy broadcasting (see that module's own doc for why).
"""

from __future__ import annotations

import _framework
import numpy as np
from aerosandbox.aerodynamics.aero_3D.singularities.uniform_strength_horseshoe_singularities import (
    calculate_induced_velocity_horseshoe,
)

VORTEX_CORE_RADIUS = 1e-8


def _uvw(u, v, w) -> list[float]:
    return [float(u), float(v), float(w)]


SINGLE_VORTEX_CASES = {
    "away_from_legs": dict(
        field=[0.0, 0.0, 0.0],
        left=[-1.0, -1.0, 0.0],
        right=[-1.0, 1.0, 0.0],
        gamma=1.0,
    ),
    "near_bound_leg": dict(
        field=[-1.0, 0.0, 1e-6],
        left=[-1.0, -1.0, 0.0],
        right=[-1.0, 1.0, 0.0],
        gamma=1.0,
    ),
    "on_trailing_leg_from_left": dict(
        field=[4.0, -1.0, 0.0],
        left=[-1.0, -1.0, 0.0],
        right=[-1.0, 1.0, 0.0],
        gamma=1.0,
    ),
    "near_right_vertex": dict(
        field=[-1.0 + 1e-7, 1.0, 0.0],
        left=[-1.0, -1.0, 0.0],
        right=[-1.0, 1.0, 0.0],
        gamma=2.5,
    ),
}

TRAILING_DIRECTION = [1.0, 0.0, 0.0]

MULTI_VORTEX_LEFTS = [[0.0, -1.0, 0.0], [0.0, 0.0, 0.0]]
MULTI_VORTEX_RIGHTS = [[0.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
MULTI_VORTEX_GAMMAS = [2.0, 1.0]
MULTI_VORTEX_FIELD_POINTS = [
    [0.5, 0.3, 0.2],
    [0.0, 0.0, 0.01],
    [-1.0, 0.0, 0.5],
]


def _single_vortex_payload() -> dict:
    cases = {}
    for name, params in SINGLE_VORTEX_CASES.items():
        u, v, w = calculate_induced_velocity_horseshoe(
            x_field=params["field"][0],
            y_field=params["field"][1],
            z_field=params["field"][2],
            x_left=params["left"][0],
            y_left=params["left"][1],
            z_left=params["left"][2],
            x_right=params["right"][0],
            y_right=params["right"][1],
            z_right=params["right"][2],
            gamma=params["gamma"],
            trailing_vortex_direction=np.array(TRAILING_DIRECTION),
            vortex_core_radius=VORTEX_CORE_RADIUS,
        )
        cases[name] = {
            "field": params["field"],
            "left": params["left"],
            "right": params["right"],
            "gamma": params["gamma"],
            "vortex_core_radius": VORTEX_CORE_RADIUS,
            "trailing_vortex_direction": TRAILING_DIRECTION,
            "result": _uvw(u, v, w),
        }
    return cases


def _multi_vortex_payload() -> dict:
    lefts = np.array(MULTI_VORTEX_LEFTS)
    rights = np.array(MULTI_VORTEX_RIGHTS)
    gammas = np.array(MULTI_VORTEX_GAMMAS)

    def tall(array):
        return np.reshape(array, (-1, 1))

    def wide(array):
        return np.reshape(array, (1, -1))

    field = np.array(MULTI_VORTEX_FIELD_POINTS)

    u_each, v_each, w_each = calculate_induced_velocity_horseshoe(
        x_field=wide(field[:, 0]),
        y_field=wide(field[:, 1]),
        z_field=wide(field[:, 2]),
        x_left=tall(lefts[:, 0]),
        y_left=tall(lefts[:, 1]),
        z_left=tall(lefts[:, 2]),
        x_right=tall(rights[:, 0]),
        y_right=tall(rights[:, 1]),
        z_right=tall(rights[:, 2]),
        gamma=tall(gammas),
        trailing_vortex_direction=np.array(TRAILING_DIRECTION),
        vortex_core_radius=VORTEX_CORE_RADIUS,
    )
    u_sum = np.sum(u_each, axis=0)
    v_sum = np.sum(v_each, axis=0)
    w_sum = np.sum(w_each, axis=0)

    return {
        "lefts": MULTI_VORTEX_LEFTS,
        "rights": MULTI_VORTEX_RIGHTS,
        "gammas": MULTI_VORTEX_GAMMAS,
        "vortex_core_radius": VORTEX_CORE_RADIUS,
        "trailing_vortex_direction": TRAILING_DIRECTION,
        "field_points": MULTI_VORTEX_FIELD_POINTS,
        "summed_results": [
            _uvw(u_sum[i], v_sum[i], w_sum[i]) for i in range(len(MULTI_VORTEX_FIELD_POINTS))
        ],
    }


def main() -> None:
    payload = {
        "single_vortex": _single_vortex_payload(),
        "multi_vortex_broadcast": _multi_vortex_payload(),
    }

    _framework.write(
        "aero",
        "singularities",
        payload,
        description=(
            "aerosandbox.aerodynamics.aero_3D.singularities."
            "uniform_strength_horseshoe_singularities.calculate_induced_velocity_horseshoe "
            "at vortex_core_radius=1e-8, single-vortex cases probing each smoothed term "
            "plus a multi-vortex broadcast-and-sum case"
        ),
    )


if __name__ == "__main__":
    main()
