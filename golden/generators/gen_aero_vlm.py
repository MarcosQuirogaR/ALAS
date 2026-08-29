# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-aero::asb_vlm``: AeroSandbox's ``VortexLatticeMethod``, scoped to
what ``alas/physics/aerodynamics.py``'s ``AeroAnalysis._run_vlm`` and
``alas/physics/stability.py``'s ``static_margin``/``autobalance`` reach --
constructing one with ``airplane``, ``op_point``, ``spanwise_resolution``,
``chordwise_resolution``, ``verbose=False``, then calling only ``.run()``.

A small, hand-built 3-wing airplane (main wing, horizontal stabilizer,
vertical stabilizer -- shaped like, but far smaller than,
``aircraft_builder.py``'s default aircraft, to keep the panel count and this
fixture small) stands in for the real geometry, which
``alas-geom::builder``'s own fixture already covers. ``s_ref``/``c_ref``/
``b_ref`` are read off the main wing's own ``area``/``mean_aerodynamic_chord``/
``span`` rather than given as separate literals, so this fixture cannot drift
from what those methods (already ``green`` in ``alas-geom::asb::wing``)
compute.

Cases mirror ``gen_aero_operating_point.py``'s list -- a plain positive-alpha
condition, a sideslipping one, one with all three rotation rates nonzero, and
a negative-alpha/negative-beta/negative-rate condition -- plus one further
case, ``fine_spanwise``, at ``spanwise_resolution=2``. That is the only case
that exercises ``Wing.subdivide_sections``' ``spacing_function=np.cosspace``
branch (``VortexLatticeMethod.run`` only calls ``subdivide_sections`` when
``spanwise_resolution > 1``); every other case uses ``spanwise_resolution=1``
to keep the panel count down, per the branch note in
``alas-geom::asb::wing``'s module doc.

For each case, the full ``run()`` output dict is recorded, along with
``vortex_strengths`` -- the solved circulation vector, which lives on the
solved ``VortexLatticeMethod`` instance (``self.vortex_strengths``) rather
than in the returned dict, and is the strongest available check that the AIC
assembly and the linear solve are both right, since a wrong assembly can
still integrate to a coincidentally close total force.
"""

from __future__ import annotations

import _framework
import numpy as np
from aerosandbox.aerodynamics.aero_3D.vortex_lattice_method import VortexLatticeMethod
from aerosandbox.atmosphere import Atmosphere
from aerosandbox.geometry.airfoil.airfoil import Airfoil
from aerosandbox.geometry.airplane import Airplane
from aerosandbox.geometry.wing import Wing, WingXSec
from aerosandbox.performance.operating_point import OperatingPoint

XYZ_REF = [1.0, 0.0, 0.1]

CASES = {
    "baseline": dict(
        altitude_m=0.0,
        velocity=60.0,
        alpha=5.0,
        beta=0.0,
        p=0.0,
        q=0.0,
        r=0.0,
        spanwise_resolution=1,
        chordwise_resolution=3,
    ),
    "sideslip": dict(
        altitude_m=2000.0,
        velocity=70.0,
        alpha=3.0,
        beta=4.0,
        p=0.0,
        q=0.0,
        r=0.0,
        spanwise_resolution=1,
        chordwise_resolution=3,
    ),
    "rotation_rates": dict(
        altitude_m=5000.0,
        velocity=55.0,
        alpha=2.0,
        beta=-3.0,
        p=0.02,
        q=0.03,
        r=-0.01,
        spanwise_resolution=1,
        chordwise_resolution=3,
    ),
    "negative": dict(
        altitude_m=8000.0,
        velocity=90.0,
        alpha=-4.0,
        beta=-5.0,
        p=-0.01,
        q=0.02,
        r=-0.02,
        spanwise_resolution=1,
        chordwise_resolution=3,
    ),
    # The only case with spanwise_resolution > 1, so this is the one that
    # exercises Wing.subdivide_sections' spacing_function=np.cosspace branch
    # -- see the module doc.
    "fine_spanwise": dict(
        altitude_m=1000.0,
        velocity=65.0,
        alpha=4.0,
        beta=2.0,
        p=0.01,
        q=-0.02,
        r=0.015,
        spanwise_resolution=2,
        chordwise_resolution=2,
    ),
}


def _build_airplane() -> Airplane:
    naca4412 = Airfoil("naca4412")
    naca0012 = Airfoil("naca0012")

    main_wing = Wing(
        name="Main Wing",
        symmetric=True,
        xsecs=[
            WingXSec(xyz_le=[0.0, 0.0, 0.0], chord=2.0, twist=2.0, airfoil=naca4412),
            WingXSec(xyz_le=[0.5, 4.0, 0.2], chord=1.0, twist=0.0, airfoil=naca4412),
        ],
    )
    hstab = Wing(
        name="Horizontal Stabilizer",
        symmetric=True,
        xsecs=[
            WingXSec(xyz_le=[6.0, 0.0, 0.0], chord=1.0, twist=-1.0, airfoil=naca0012),
            WingXSec(xyz_le=[6.2, 1.5, 0.05], chord=0.6, twist=-1.0, airfoil=naca0012),
        ],
    )
    vstab = Wing(
        name="Vertical Stabilizer",
        symmetric=False,
        xsecs=[
            WingXSec(xyz_le=[6.0, 0.0, 0.0], chord=1.2, twist=0.0, airfoil=naca0012),
            WingXSec(xyz_le=[6.3, 0.0, 1.4], chord=0.7, twist=0.0, airfoil=naca0012),
        ],
    )

    return Airplane(
        name="ALAS VLM Probe",
        xyz_ref=XYZ_REF,
        wings=[main_wing, hstab, vstab],
        fuselages=[],
        s_ref=float(main_wing.area()),
        c_ref=float(main_wing.mean_aerodynamic_chord()),
        b_ref=float(main_wing.span()),
    )


# One further operating point, at which run_with_stability_derivatives is
# swept. alpha and beta are both nonzero so the lateral-directional
# derivatives (CYb, Cnb, Clb, and the yaw-rate set) are structurally nonzero
# rather than machine-epsilon noise a relative tier could not frame.
STABILITY_CASE = dict(
    altitude_m=3000.0,
    velocity=75.0,
    alpha=4.0,
    beta=3.0,
    p=0.0,
    q=0.0,
    r=0.0,
    spanwise_resolution=1,
    chordwise_resolution=3,
)


def _vec3(v) -> list[float]:
    return [float(v[0]), float(v[1]), float(v[2])]


def _result_dict(result: dict) -> dict:
    return {
        "force_geometry": _vec3(result["F_g"]),
        "force_body": _vec3(result["F_b"]),
        "force_wind": _vec3(result["F_w"]),
        "moment_geometry": _vec3(result["M_g"]),
        "moment_body": _vec3(result["M_b"]),
        "moment_wind": _vec3(result["M_w"]),
        "lift": float(result["L"]),
        "drag": float(result["D"]),
        "side_force": float(result["Y"]),
        "roll_moment": float(result["l_b"]),
        "pitch_moment": float(result["m_b"]),
        "yaw_moment": float(result["n_b"]),
        "cl_lift": float(result["CL"]),
        "cd_drag": float(result["CD"]),
        "cy_side": float(result["CY"]),
        "cl_roll": float(result["Cl"]),
        "cm_pitch": float(result["Cm"]),
        "cn_yaw": float(result["Cn"]),
    }


def _derivatives(result: dict, suffix: str) -> dict:
    """The six coefficient derivatives with one denominator abbreviation --
    e.g. suffix "a" reads CLa/CDa/CYa/Cla/Cma/Cna."""
    return {
        "cl_lift": float(result[f"CL{suffix}"]),
        "cd_drag": float(result[f"CD{suffix}"]),
        "cy_side": float(result[f"CY{suffix}"]),
        "cl_roll": float(result[f"Cl{suffix}"]),
        "cm_pitch": float(result[f"Cm{suffix}"]),
        "cn_yaw": float(result[f"Cn{suffix}"]),
    }


def _run_stability_case(airplane: Airplane, params: dict) -> dict:
    atmo = Atmosphere(altitude=params["altitude_m"])
    op_point = OperatingPoint(
        atmosphere=atmo,
        velocity=params["velocity"],
        alpha=params["alpha"],
        beta=params["beta"],
        p=params["p"],
        q=params["q"],
        r=params["r"],
    )
    vlm = VortexLatticeMethod(
        airplane=airplane,
        op_point=op_point,
        spanwise_resolution=params["spanwise_resolution"],
        chordwise_resolution=params["chordwise_resolution"],
        verbose=False,
    )
    result = vlm.run_with_stability_derivatives(
        alpha=True, beta=True, p=True, q=True, r=True
    )
    return {
        "inputs": params,
        "base": _result_dict(result),
        "d_alpha": _derivatives(result, "a"),
        "d_beta": _derivatives(result, "b"),
        "d_p": _derivatives(result, "p"),
        "d_q": _derivatives(result, "q"),
        "d_r": _derivatives(result, "r"),
        "x_np": float(result["x_np"]),
        "x_np_lateral": float(result["x_np_lateral"]),
    }


def _run_case(airplane: Airplane, params: dict) -> dict:
    atmo = Atmosphere(altitude=params["altitude_m"])
    op_point = OperatingPoint(
        atmosphere=atmo,
        velocity=params["velocity"],
        alpha=params["alpha"],
        beta=params["beta"],
        p=params["p"],
        q=params["q"],
        r=params["r"],
    )
    vlm = VortexLatticeMethod(
        airplane=airplane,
        op_point=op_point,
        spanwise_resolution=params["spanwise_resolution"],
        chordwise_resolution=params["chordwise_resolution"],
        verbose=False,
    )
    result = vlm.run()

    return {
        "inputs": params,
        "vortex_strengths": [float(x) for x in np.asarray(vlm.vortex_strengths)],
        "result": _result_dict(result),
    }


def main() -> None:
    airplane = _build_airplane()

    cases = {name: _run_case(airplane, params) for name, params in CASES.items()}
    stability = _run_stability_case(airplane, STABILITY_CASE)

    # Sanity check on the generator's own extraction: every case's panel
    # count (the length of vortex_strengths) should match the mesh this
    # program's spanwise/chordwise resolution produces -- 1 spanwise interval
    # per wing unless subdivided, times chordwise_resolution, doubled for
    # each symmetric wing.
    for name, params in CASES.items():
        spanwise = params["spanwise_resolution"]
        chordwise = params["chordwise_resolution"]
        # main_wing and hstab are symmetric (2x), vstab is not (1x); every
        # wing here has exactly 2 xsecs, i.e. 1 lofted interval before any
        # subdivision.
        panels_per_side = spanwise * chordwise
        expected = 2 * panels_per_side + 2 * panels_per_side + panels_per_side
        actual = len(cases[name]["vortex_strengths"])
        if actual != expected:
            raise SystemExit(
                f"case {name}: expected {expected} panels from the mesh geometry, got {actual}"
            )

    if not any(params["spanwise_resolution"] > 1 for params in CASES.values()):
        raise SystemExit(
            "no case exercises spanwise_resolution > 1; this fixture would not "
            "catch a wrong Wing.subdivide_sections spacing_function"
        )

    _framework.write(
        "aero",
        "asb_vlm",
        {
            "xyz_ref": XYZ_REF,
            "s_ref": float(airplane.s_ref),
            "c_ref": float(airplane.c_ref),
            "b_ref": float(airplane.b_ref),
            "cases": cases,
            "stability_derivatives": stability,
        },
        description=(
            "aerosandbox.aerodynamics.aero_3D.vortex_lattice_method.VortexLatticeMethod.run() "
            "on a small hand-built 3-wing airplane, across five operating points -- one of "
            "them at spanwise_resolution=2 to exercise Wing.subdivide_sections' cosspace "
            "branch -- recording the full result dict plus the solved vortex_strengths, plus "
            "one run_with_stability_derivatives sweep recording every force/moment "
            "coefficient derivative and both neutral points"
        ),
    )


if __name__ == "__main__":
    main()
