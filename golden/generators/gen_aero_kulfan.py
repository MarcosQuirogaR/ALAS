# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-aero::kulfan``: AeroSandbox's Kulfan (CST with LEM) airfoil
parameterization -- the least-squares fit from coordinates to shape weights,
and the evaluation back from weights to coordinates.

This is the airfoil surrogate's front half. ``alas/analysis/airfoil_screening.py``
and ``alas/reporting/visualization.py`` both reach NeuralFoil, and both reach it
the same way: ``Airfoil.normalize(return_dict=True)``, then
``to_kulfan_airfoil(n_weights_per_side=8, normalize_coordinates=False)``, then
the network. Everything before the network is this fixture.

**Every case's coordinates are already normalized**, because that is the shape
the fit is actually handed: both call sites normalize first and then pass
``normalize_coordinates=False`` so the fit does not redo it. The fixture stores
those normalized coordinates verbatim as the fit's input, so the parity test
checks the fit alone rather than the fit composed with a coordinate source that
has its own fixture (``golden/geom/asb_airfoil.json``,
``golden/geom/selig.json``) and its own tier.

That matters for more than tidiness. ``get_kulfan_parameters``'s class function
is ``x**N1``, with ``N1 = 0.5``, and several airfoils in this program's own
inputs carry vertices at slightly negative ``x`` before normalization
(``naca4412`` reaches -2.98e-4) -- which makes ``x**0.5`` NaN and poisons the
whole fit. Normalizing first is what keeps that from happening upstream, so it
is what the fixture must feed in. ``_assert_normalized`` below refuses to write
a fixture in which it stopped being true.

The case set is chosen for the branches, not for coverage of the airfoil
catalog:

* ``naca0012_symmetric`` -- a symmetric section, whose leading-edge weight is
  exactly zero in closed form and machine noise in floating point.
* ``naca2410_alas`` / ``sc2_0714_alas`` -- the default aircraft's tip and root
  sections, read through ``AirfoilLibrary.get`` so they are the coordinates
  this program actually fits.
* ``naca4412_thick_camber`` -- a strongly cambered section, where the LEM term
  carries real weight.
* ``e63_sharp_te`` / ``sd7037_sharp_te`` -- database sections with a closed
  trailing edge, which drive the unconstrained fit to a *negative* trailing-edge
  thickness and so take the re-solve branch that drops the last column and
  pins the thickness to zero. ``_assert_branches`` refuses a fixture in which
  no case reaches it.
* ``dae11_high_lem`` -- a high-camber section whose leading-edge weight is
  around 0.5, two orders above the noise floor the symmetric case sits at.
* ``ag41d_repaneled`` -- a cosine-repaneled section, the point distribution
  that arrives when something upstream has repaneled before fitting.
* ``naca2410_four_weights`` -- the same section at ``n_weights_per_side=4``.
  Nothing in this program passes anything but 8, but the count also sets the
  LEM exponent (``n_weights_per_side + 0.5``), so a port that hardcoded 8.5
  while taking the count as a parameter would agree everywhere except here.

The forward map is recorded per case as ``upper_coordinates``/
``lower_coordinates`` sampled at a fixed ``x_over_c`` list that includes both
endpoints (where the class function vanishes), plus ``to_airfoil`` -- the
cosine-spaced reconstruction ``KulfanAirfoil.coordinates`` is defined as -- for
the two cases that reach the post-stall branch downstream.
"""

from __future__ import annotations

import _framework
import numpy as np

_framework.add_alas_to_path()

import aerosandbox as asb  # noqa: E402
from aerosandbox.geometry.airfoil.airfoil_families import (  # noqa: E402
    get_kulfan_parameters,
)
from aerosandbox.geometry.airfoil.kulfan_airfoil import KulfanAirfoil  # noqa: E402
from alas.geometry.airfoils import AirfoilLibrary  # noqa: E402

N1 = 0.5
N2 = 1.0

# Both endpoints are included on purpose: the class function is zero at each,
# so a port that folded a division into it would show up here and nowhere else.
X_SAMPLE = [
    0.0,
    1e-6,
    0.001,
    0.01,
    0.05,
    0.1,
    0.25,
    0.5,
    0.75,
    0.9,
    0.99,
    0.999,
    1.0,
]


def _from_asb(name: str, *, repanel: bool = False) -> asb.Airfoil:
    airfoil = asb.Airfoil(name)
    if repanel:
        airfoil = airfoil.repanel()
    return airfoil


def _from_alas(name: str) -> asb.Airfoil:
    return AirfoilLibrary.get(name)


CASES = {
    "naca0012_symmetric": dict(source=lambda: _from_asb("naca0012"), n_weights=8),
    "naca2410_alas": dict(source=lambda: _from_alas("naca2410"), n_weights=8),
    "sc2_0714_alas": dict(source=lambda: _from_alas("SC2-0714"), n_weights=8),
    "naca4412_thick_camber": dict(source=lambda: _from_asb("naca4412"), n_weights=8),
    "e63_sharp_te": dict(source=lambda: _from_asb("e63"), n_weights=8),
    "sd7037_sharp_te": dict(source=lambda: _from_asb("sd7037"), n_weights=8),
    "dae11_high_lem": dict(source=lambda: _from_asb("dae11"), n_weights=8),
    "ag41d_repaneled": dict(
        source=lambda: _from_asb("ag41d-02r", repanel=True), n_weights=8
    ),
    "naca2410_four_weights": dict(source=lambda: _from_alas("naca2410"), n_weights=4),
}

# The two whose reconstruction is exercised downstream: `airfoil_coefficients_post_stall`
# reads a `KulfanAirfoil`'s coordinates, which are `to_airfoil()`'s output.
TO_AIRFOIL_CASES = {"naca0012_symmetric": 200, "dae11_high_lem": 40}


def _assert_normalized(name: str, coordinates: np.ndarray) -> None:
    """Refuse a case whose coordinates would make the class function NaN.

    ``x ** 0.5`` is not real below zero, and the fit's class function is exactly
    that. The reached call sites normalize before fitting, which is what keeps
    every abscissa at or above zero; a case that stopped satisfying it would
    silently record a fixture full of NaNs rather than a fit.
    """
    x = coordinates[:, 0]
    if np.any(x < 0.0):
        raise SystemExit(
            f"{name}: {int(np.sum(x < 0.0))} normalized abscissae are negative "
            f"(min {x.min():.3e}); the class function x**0.5 is NaN there"
        )


def _assert_branches(cases: dict) -> None:
    """Refuse a fixture that does not reach both trailing-edge branches.

    The negative-thickness re-solve is a whole second least-squares problem on
    a different matrix, and it is the part of this module a port is most likely
    to leave out: without it every airfoil still fits, and the ones with a
    closed trailing edge come back with a small negative thickness that looks
    like rounding.
    """
    pinned = [n for n, c in cases.items() if c["te_thickness"] == 0.0]
    open_te = [n for n, c in cases.items() if c["te_thickness"] > 0.0]
    if not pinned:
        raise SystemExit(
            "no case reached the negative-trailing-edge-thickness re-solve; "
            "add a section with a closed trailing edge"
        )
    if not open_te:
        raise SystemExit(
            "no case kept a positive trailing-edge thickness; the fixture would "
            "not distinguish the re-solve from always pinning to zero"
        )


def _pairs(array: np.ndarray) -> list[list[float]]:
    return [[float(x), float(y)] for x, y in array]


def _case_payload(name: str, source, n_weights: int) -> dict:
    airfoil = source()

    # What both reached call sites do: normalize, then fit without redoing it.
    coordinates = airfoil.normalize().coordinates
    _assert_normalized(name, coordinates)

    parameters = get_kulfan_parameters(
        coordinates=coordinates,
        n_weights_per_side=n_weights,
        N1=N1,
        N2=N2,
        normalize_coordinates=False,
        use_leading_edge_modification=True,
    )

    kulfan = KulfanAirfoil(
        name=name,
        lower_weights=parameters["lower_weights"],
        upper_weights=parameters["upper_weights"],
        leading_edge_weight=parameters["leading_edge_weight"],
        TE_thickness=parameters["TE_thickness"],
        N1=N1,
        N2=N2,
    )

    payload = {
        "n_weights_per_side": n_weights,
        "N1": N1,
        "N2": N2,
        "coordinates": _pairs(coordinates),
        "lower_weights": [float(w) for w in parameters["lower_weights"]],
        "upper_weights": [float(w) for w in parameters["upper_weights"]],
        "leading_edge_weight": float(parameters["leading_edge_weight"]),
        "te_thickness": float(parameters["TE_thickness"]),
        "x_sample": X_SAMPLE,
        "upper_coordinates": _pairs(kulfan.upper_coordinates(x_over_c=np.array(X_SAMPLE))),
        "lower_coordinates": _pairs(kulfan.lower_coordinates(x_over_c=np.array(X_SAMPLE))),
        # `KulfanAirfoil` inherits `Airfoil.max_thickness` but overrides the
        # `local_thickness` underneath it, so this samples the two class/shape
        # surfaces analytically at `np.linspace(0, 1, 101)` rather than
        # interpolating a vertex list. `alas-aero::neuralfoil` reads it as the
        # `t/c` that sets the supersonic end of its wave-drag schedule.
        "max_thickness": float(kulfan.max_thickness()),
    }

    if name in TO_AIRFOIL_CASES:
        per_side = TO_AIRFOIL_CASES[name]
        payload["to_airfoil"] = {
            "n_coordinates_per_side": per_side,
            "coordinates": _pairs(
                kulfan.to_airfoil(n_coordinates_per_side=per_side).coordinates
            ),
        }

    return payload


def main() -> None:
    cases = {
        name: _case_payload(name, spec["source"], spec["n_weights"])
        for name, spec in CASES.items()
    }
    _assert_branches(cases)

    _framework.write(
        "aero",
        "kulfan",
        {"cases": cases},
        description=(
            "aerosandbox.geometry.airfoil.airfoil_families.get_kulfan_parameters "
            "(method='least_squares', normalize_coordinates=False on already-normalized "
            "coordinates, as both reached call sites pass it) and the KulfanAirfoil "
            "forward map (upper_coordinates/lower_coordinates/to_airfoil), over nine "
            "sections reaching both trailing-edge branches and a non-default "
            "n_weights_per_side"
        ),
    )


if __name__ == "__main__":
    main()
