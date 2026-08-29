# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-stab::modes``: AeroSandbox's
``dynamics.flight_dynamics.airplane.get_modes`` -- the closed-form
small-perturbation eigenmode approximations (phugoid, short-period, roll
subsidence, dutch roll, spiral) from a stability-derivative set.

``get_modes`` takes the derivatives as *data*: it runs no vortex-lattice solve
of its own (that is ``compute_dynamic_modes``'s job, one level up). So unlike
every other ``alas-stab`` fixture, this one feeds **fixed** derivative sets
rather than deriving them from a VLM run on a built aircraft -- which is what
lets this row sit at the ``closed`` tier. The only floating-point input that is
not a literal is the atmospheric density behind ``op_point.dynamic_pressure()``
and ``op_point.atmosphere.density()``; ``asb.Atmosphere`` here is the default
differentiable (B-spline) model, the same fitted atmosphere ``alas-prop::cycle``
already rides at ``closed`` against ``alas-atmo::Atmosphere::new``.

Two cases, chosen to reach both branches of ``get_mode_info``:

* ``b737`` -- the Boeing 737-800 numbers from ``get_modes``'s own ``__main__``
  (Caughey, "Introduction to Aircraft Stability and Control", 2011). A
  conventional, statically- and directionally-stable aircraft: phugoid,
  short-period and dutch-roll all oscillatory (nonzero imaginary part), roll
  and spiral aperiodic.
* ``unstable`` -- a statically unstable (``Cma`` > 0) and directionally
  unstable (``Cnb`` < 0) variant, which drives the short-period and dutch-roll
  characteristic ``omega_squared`` negative so both degrade to the aperiodic
  branch (zero imaginary part, the root folded into the real part). Without it
  a port that dropped ``get_mode_info``'s aperiodic branch would still pass.

The generator refuses to write a fixture in which no case reaches each branch.
"""

from __future__ import annotations

import math

import _framework

_framework.add_alas_to_path()

import aerosandbox as asb  # noqa: E402
from aerosandbox.dynamics.flight_dynamics.airplane import get_modes  # noqa: E402
from aerosandbox.tools import units as u  # noqa: E402

MODE_NAMES = ("phugoid", "short_period", "roll_subsidence", "dutch_roll", "spiral")

# The eleven coefficients get_modes reads off the aero dict; recorded so the
# Rust StabilityAero is fed exactly what the reference eigenmode formulas saw.
AERO_KEYS = {
    "cl": "CL",
    "cd": "CD",
    "cma": "Cma",
    "cmq": "Cmq",
    "clp": "Clp",
    "cyb": "CYb",
    "cnb": "Cnb",
    "cyr": "CYr",
    "cnr": "Cnr",
    "clb": "Clb",
    "clr": "Clr",
}

# Boeing 737-800, from get_modes's __main__ (Caughey 2011).
B737_AIRPLANE = dict(
    s_ref=1260 * u.foot**2,
    c_ref=11 * u.foot,
    b_ref=113 * u.foot,
)
B737_OP_POINT = dict(altitude_m=2438.399975619396, velocity=85.64176936131635)
B737_MASS = dict(mass=77146.0, ixx=706684.0, iyy=0.270824e7, izz=0.330763e7)
B737_AERO = dict(
    CL=1.83443,
    CD=0.13037,
    Cma=-2.044696,
    CYb=-1.103873,
    Clb=-0.374933,
    Cnb=0.239877,
    Clp=-0.449404,
    Cmq=-74.997742,
    CYr=0.796001,
    Clr=0.364638,
    Cnr=-0.434410,
)

# A statically- and directionally-unstable variant: Cma flipped positive and
# Cnb flipped negative push the short-period and dutch-roll omega_squared below
# zero, reaching get_mode_info's aperiodic branch.
UNSTABLE_AIRPLANE = dict(s_ref=120.0, c_ref=4.2, b_ref=34.0)
UNSTABLE_OP_POINT = dict(altitude_m=8000.0, velocity=180.0)
UNSTABLE_MASS = dict(mass=60000.0, ixx=1.2e6, iyy=3.0e6, izz=3.8e6)
UNSTABLE_AERO = dict(
    CL=0.45,
    CD=0.028,
    Cma=0.6,
    CYb=-0.9,
    Clb=-0.18,
    Cnb=-0.05,
    Clp=-0.52,
    Cmq=-18.0,
    CYr=0.55,
    Clr=0.09,
    Cnr=-0.21,
)


def _aero_subset(aero: dict) -> dict:
    return {rust_key: float(aero[asb_key]) for rust_key, asb_key in AERO_KEYS.items()}


def _run_case(airplane_spec: dict, op_spec: dict, mass_spec: dict, aero: dict) -> dict:
    airplane = asb.Airplane(
        s_ref=airplane_spec["s_ref"],
        c_ref=airplane_spec["c_ref"],
        b_ref=airplane_spec["b_ref"],
    )
    op_point = asb.OperatingPoint(
        atmosphere=asb.Atmosphere(altitude=op_spec["altitude_m"]),
        velocity=op_spec["velocity"],
    )
    mass_props = asb.MassProperties(
        mass=mass_spec["mass"],
        Ixx=mass_spec["ixx"],
        Iyy=mass_spec["iyy"],
        Izz=mass_spec["izz"],
    )
    raw = get_modes(
        airplane=airplane, op_point=op_point, mass_props=mass_props, aero=aero
    )

    modes = {}
    for name in MODE_NAMES:
        m = raw[name]
        modes[name] = {
            "eigenvalue_real": float(m["eigenvalue_real"]),
            "eigenvalue_imag": float(m["eigenvalue_imag"]),
            "damping_ratio": float(m["damping_ratio"]),
        }

    return {
        "airplane": {
            "s_ref": float(airplane.s_ref),
            "c_ref": float(airplane.c_ref),
            "b_ref": float(airplane.b_ref),
        },
        "op_point": {
            "altitude_m": float(op_spec["altitude_m"]),
            "velocity": float(op_spec["velocity"]),
        },
        "mass": {k: float(v) for k, v in mass_spec.items()},
        "aero": _aero_subset(aero),
        "modes": modes,
    }


def main() -> None:
    cases = {
        "b737": _run_case(B737_AIRPLANE, B737_OP_POINT, B737_MASS, B737_AERO),
        "unstable": _run_case(
            UNSTABLE_AIRPLANE, UNSTABLE_OP_POINT, UNSTABLE_MASS, UNSTABLE_AERO
        ),
    }

    # get_mode_info has an oscillatory branch (nonzero imaginary part) and an
    # aperiodic one (zero imaginary part, root folded into the real part). A
    # fixture reaching only one of them would pass against a port missing the
    # other.
    b737_sp = cases["b737"]["modes"]["short_period"]
    if b737_sp["eigenvalue_imag"] == 0.0:
        raise SystemExit("b737 short-period did not reach the oscillatory branch")
    unstable_sp = cases["unstable"]["modes"]["short_period"]
    if unstable_sp["eigenvalue_imag"] != 0.0:
        raise SystemExit("unstable short-period did not reach the aperiodic branch")
    unstable_dr = cases["unstable"]["modes"]["dutch_roll"]
    if unstable_dr["eigenvalue_imag"] != 0.0:
        raise SystemExit("unstable dutch-roll did not reach the aperiodic branch")
    # A damping ratio should be finite everywhere these cases reach.
    for name, case in cases.items():
        for mode_name, mode in case["modes"].items():
            if not math.isfinite(mode["damping_ratio"]):
                raise SystemExit(f"{name}.{mode_name} has a non-finite damping ratio")

    _framework.write(
        "stab",
        "modes",
        {"cases": cases},
        description=(
            "aerosandbox.dynamics.flight_dynamics.airplane.get_modes on fixed "
            "stability-derivative sets -- the Boeing 737-800 from its own "
            "__main__ (all oscillatory modes) and a statically/directionally "
            "unstable variant that drives short-period and dutch-roll to the "
            "aperiodic branch of get_mode_info"
        ),
    )


if __name__ == "__main__":
    main()
