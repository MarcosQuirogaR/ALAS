# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""AeroSandbox's *default* atmosphere -- the fitted one, not the closed form.

``asb.Atmosphere(altitude=...)`` with no ``method`` argument evaluates a cubic
B-spline fitted through the ISA at thirty-eight altitudes, and that is what
every altitude-dependent module in the reference implementation but one
actually constructs. The fit and the closed form disagree by up to 1.1% in
temperature over the altitudes this program flies at, so ``golden/atmo/isa``
is not a stand-in for this fixture and this one is not a stand-in for that.

The case list is chosen so that a wrong knot vector cannot hide:

* every fitted altitude between -50 km and 200 km, where interpolation makes
  the fit reproduce the ISA exactly and therefore says nothing about the
  spline itself;
* points between consecutive fitted altitudes, which is where the fit departs
  from the ISA and where a spline with the knots one data point over would
  give a different, equally smooth, wrong answer;
* the flight envelope at a fine spacing, which is the band every consumer of
  this crate samples;
* both ends of the fitted band exactly, where the not-a-knot end condition
  decides the answer.

Two properties are checked here rather than recorded, because a fixture
cannot hold them: that the fit reproduces the ISA at its own knots, and that
outside the fitted band AeroSandbox returns NaN rather than extrapolating.
The second is ``InterpolatedModel``'s ``fill_value=np.nan`` default reaching
through ``interpn``, and it is the behaviour ``alas-atmo::differentiable``
reproduces; NaN has no portable JSON spelling, so the evidence for it lives
in this check.
"""

from __future__ import annotations

import _framework

# A characteristic length for the Knudsen number, matching `gen_atmo.py`.
KNUDSEN_LENGTH_M = 0.1

# Altitudes exercised with a nonzero temperature deviation, to pin down that
# it shifts temperature (and everything derived from it) without touching
# pressure. One warmer than the model, one colder.
DEVIATIONS_K = {
    0.0: 5.0,
    11000.0: -8.0,
}


def _altitudes(knots: list[float]) -> list[float]:
    """Every altitude the fixture evaluates, ascending and without repeats."""
    altitudes = set()

    # The flight envelope, finely: this is the band the turbofan cycle, the
    # performance envelope and the mission all sample.
    altitudes.update(float(step) * 250.0 for step in range(-4, 61))

    # The fitted altitudes themselves, and the midpoint of each span, over
    # the range where either is a physically meaningful altitude.
    for altitude_m in knots:
        if -50e3 <= altitude_m <= 200e3:
            altitudes.add(float(altitude_m))
    for lower, upper in zip(knots, knots[1:]):
        if -50e3 <= lower and upper <= 200e3:
            altitudes.add(float(lower + 0.5 * (upper - lower)))
            altitudes.add(float(lower + 0.25 * (upper - lower)))

    # The ISA layer boundaries, where the fit is smooth and the closed form
    # is not -- the clearest place for the two models to be confused.
    altitudes.update([0.0, 11000.0, 20000.0, 32000.0, 47000.0, 51000.0, 71000.0, 84852.0])

    # Both ends of the fitted band exactly, where the end condition decides
    # the answer and where an out-of-range test is one ulp away.
    altitudes.add(float(knots[0]))
    altitudes.add(float(knots[-1]))

    return sorted(altitudes)


def main() -> None:
    _framework.add_alas_to_path()
    import numpy as np
    from aerosandbox.atmosphere._diff_atmo_functions import altitude_knot_points
    from aerosandbox.atmosphere.atmosphere import Atmosphere

    knots = [float(v) for v in altitude_knot_points]
    if len(knots) != 38:
        raise SystemExit(f"expected 38 fitted altitudes, found {len(knots)}")

    # The default really is the fit, which is the premise of the whole
    # fixture. If AeroSandbox ever changed it, every number below would
    # silently become the closed form's.
    if Atmosphere(altitude=0.0).method != "differentiable":
        raise SystemExit("asb.Atmosphere no longer defaults to the differentiable model")

    # Interpolation, not smoothing: the fit passes through its own data.
    for altitude_m in knots:
        fitted = Atmosphere(altitude=altitude_m)
        exact = Atmosphere(altitude=altitude_m, method="isa")
        for name in ("pressure", "temperature"):
            a = float(getattr(fitted, name)())
            b = float(getattr(exact, name)())
            if abs(a - b) > 1e-9 * abs(b):
                raise SystemExit(
                    f"the fit does not reproduce the ISA at its own knot "
                    f"{altitude_m} m: {name} {a!r} against {b!r}"
                )

    # Outside the fitted band the answer is NaN, not an extrapolation.
    for altitude_m in (knots[0] - 1.0, knots[-1] + 1.0):
        outside = Atmosphere(altitude=altitude_m)
        if not (np.isnan(float(outside.pressure())) and np.isnan(float(outside.temperature()))):
            raise SystemExit(
                f"expected NaN outside the fitted band at {altitude_m} m, got "
                f"{float(outside.pressure())!r} / {float(outside.temperature())!r}"
            )

    cases = []
    for altitude_m in _altitudes(knots):
        temperature_deviation_k = DEVIATIONS_K.get(altitude_m, 0.0)
        atmo = Atmosphere(
            altitude=altitude_m,
            temperature_deviation=temperature_deviation_k,
        )
        cases.append(
            {
                "altitude_m": altitude_m,
                "temperature_deviation_k": temperature_deviation_k,
                "knudsen_length_m": KNUDSEN_LENGTH_M,
                "pressure_pa": float(atmo.pressure()),
                "temperature_k": float(atmo.temperature()),
                "density_kg_m3": float(atmo.density()),
                "speed_of_sound_m_s": float(atmo.speed_of_sound()),
                "dynamic_viscosity_pa_s": float(atmo.dynamic_viscosity()),
                "kinematic_viscosity_m2_s": float(atmo.kinematic_viscosity()),
                "ratio_of_specific_heats": float(atmo.ratio_of_specific_heats()),
                "mean_free_path_m": float(atmo.mean_free_path()),
                "knudsen_number": float(atmo.knudsen(KNUDSEN_LENGTH_M)),
                "density_altitude_m": float(atmo.density_altitude(method="approximate")),
            }
        )

    _framework.write(
        "atmo",
        "differentiable",
        {"altitude_knots_m": knots, "cases": cases},
        description=(
            "AeroSandbox Atmosphere() at its default 'differentiable' method: "
            "the fitted altitudes themselves, points between them, the flight "
            "envelope at 250 m spacing, and both ends of the fitted band"
        ),
    )


if __name__ == "__main__":
    main()
