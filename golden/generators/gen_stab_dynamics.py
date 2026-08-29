# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-stab::dynamics``: ``alas/physics/dynamics.py`` -- the gross-geometry
inertia estimate and the dynamic-mode analysis.

Like ``gen_stab_trim.py``, this runs on **the actual nominal aircraft**
``AircraftBuilder(GeometryConfig()).build()``. It has to: ``estimate_inertia``
reads the fuselage's end stations and the main wing's span, and
``compute_dynamic_modes`` runs a vortex-lattice stability-derivative sweep on
the built geometry.

Two kinds of output, and they sit at two tiers -- ``docs/PORTING.md`` records
the split:

* ``estimate_inertia`` is closed-form geometry (radii of gyration times mass),
  compared at ``closed``. Two masses are recorded so a port that dropped the
  ``mass_kg`` factor, or squared the wrong radius, is caught.
* ``compute_dynamic_modes`` runs ``run_with_stability_derivatives`` (six VLM
  solves) and feeds ``get_modes``. Every eigenvalue it reports is a function of
  finite differences of dense AIC solves, so this half is compared at
  ``linalg`` -- the same tier ``alas-aero::asb_vlm`` and ``alas-stab::trim``
  carry -- not ``closed``. It reproduces the production call exactly: the mass
  properties fed to it come from ``estimate_inertia`` on the same aircraft, the
  chain ``reporting/visualization.py``'s ``figure_dynamic_modes`` uses.

The op-point is a cruise condition: 11 km, Mach 0.82 (velocity recorded as the
exact float so the Rust side does not recompute the speed of sound), 2 degrees
angle of attack. ``compute_dynamic_modes`` leaves ``spanwise_resolution=1`` and
``chordwise_resolution`` at the ``VortexLatticeMethod`` default of 10.

Each mode records ``eigenvalue_real``/``eigenvalue_imag``/``damping_ratio``/
``period_s``/``stable`` -- the whole ``DynamicMode``. ``period_s`` is ``0.0``
for a purely real (aperiodic) mode, so the roll and spiral modes exercise the
``wn > 0`` guard's false branch while the oscillatory modes exercise its true
branch.
"""

from __future__ import annotations

import math

import aerosandbox as asb

import _framework

_framework.add_alas_to_path()

from alas.config.geometry_config import GeometryConfig  # noqa: E402
from alas.geometry.aircraft_builder import AircraftBuilder  # noqa: E402
from alas.physics.dynamics import (  # noqa: E402
    compute_dynamic_modes,
    estimate_inertia,
)

MODE_NAMES = ("phugoid", "short_period", "roll_subsidence", "dutch_roll", "spiral")

INERTIA_CASES = {
    "heavy": 250000.0,
    "light": 180000.0,
}

CRUISE = dict(altitude_m=11000.0, mach=0.82, alpha=2.0, mass_kg=250000.0)


def main() -> None:
    builder = AircraftBuilder(GeometryConfig())
    plane = builder.build()

    inertia = {}
    for name, mass_kg in INERTIA_CASES.items():
        ixx, iyy, izz = estimate_inertia(plane, mass_kg)
        inertia[name] = {
            "inputs": {"mass_kg": mass_kg},
            "ixx": float(ixx),
            "iyy": float(iyy),
            "izz": float(izz),
        }

    atmo = asb.Atmosphere(altitude=CRUISE["altitude_m"])
    velocity = CRUISE["mach"] * atmo.speed_of_sound()
    op_point = asb.OperatingPoint(atmosphere=atmo, velocity=velocity, alpha=CRUISE["alpha"])

    mass_kg = CRUISE["mass_kg"]
    ixx, iyy, izz = estimate_inertia(plane, mass_kg)
    mass_props = asb.MassProperties(mass=mass_kg, Ixx=ixx, Iyy=iyy, Izz=izz)

    raw = compute_dynamic_modes(plane, op_point, mass_props)
    modes = {}
    for name in MODE_NAMES:
        m = raw[name]
        modes[name] = {
            "eigenvalue_real": float(m.eigenvalue_real),
            "eigenvalue_imag": float(m.eigenvalue_imag),
            "damping_ratio": float(m.damping_ratio),
            "period_s": float(m.period_s),
            "stable": bool(m.stable),
        }

    dynamic = {
        "inputs": {
            "altitude_m": float(CRUISE["altitude_m"]),
            "velocity": float(velocity),
            "alpha": float(CRUISE["alpha"]),
            "mass_kg": float(mass_kg),
        },
        "modes": modes,
    }

    # The aircraft should produce a mix of oscillatory modes (nonzero
    # imaginary part) and aperiodic ones (zero imaginary part), so the
    # eigenvalue split is exercised in both directions. (The period's own
    # `wn > 0` false branch needs an exactly-zero eigenvalue, which no real
    # geometry reaches; a unit test covers it instead.)
    oscillatory = [n for n in MODE_NAMES if modes[n]["eigenvalue_imag"] != 0.0]
    aperiodic = [n for n in MODE_NAMES if modes[n]["eigenvalue_imag"] == 0.0]
    if not oscillatory:
        raise SystemExit("no oscillatory mode; expected a nonzero imaginary part somewhere")
    if not aperiodic:
        raise SystemExit("no aperiodic mode; expected a zero imaginary part somewhere")
    # Inertia must scale with mass: a port dropping the mass factor would pass a
    # single-mass fixture.
    ratio = INERTIA_CASES["heavy"] / INERTIA_CASES["light"]
    if not math.isclose(inertia["heavy"]["ixx"] / inertia["light"]["ixx"], ratio, rel_tol=1e-12):
        raise SystemExit("estimate_inertia does not scale linearly with mass")

    _framework.write(
        "stab",
        "dynamics",
        {
            "airplane": {
                "s_ref": float(plane.s_ref),
                "c_ref": float(plane.c_ref),
                "b_ref": float(plane.b_ref),
                "wing_names": [w.name for w in plane.wings],
                "fuselage_names": [f.name for f in plane.fuselages],
            },
            "estimate_inertia": inertia,
            "compute_dynamic_modes": dynamic,
        },
        description=(
            "alas.physics.dynamics on the nominal AircraftBuilder aircraft: "
            "estimate_inertia at two masses (closed-form), and "
            "compute_dynamic_modes at a Mach 0.82 / 11 km cruise point -- a "
            "run_with_stability_derivatives sweep fed to get_modes, wrapped "
            "with each mode's period and stability flag"
        ),
    )


if __name__ == "__main__":
    main()
