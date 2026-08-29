# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-stab::trim``: ``alas/physics/stability.py``'s longitudinal stability
and balance -- the two-point VLM static-margin measurement, the geometry-driven
neutral point, the cruise-condition trim solve, and the closed-form tail and
Munk correlations underneath them.

Like ``gen_aero_analysis.py``, and for the same reason, this fixture runs on
**the actual nominal aircraft** ``AircraftBuilder(GeometryConfig()).build()``,
the same object ``golden/geom/builder.json`` already pins. It has to:
``fuselage_cm_alpha`` integrates the fuselage's real cross-section area
distribution, ``neutral_point`` needs the real wing and horizontal-stabilizer
aerodynamic centres, and the trim solve runs three VLM solves on the built
geometry. A probe airplane would have exercised the formulas and not the
aeroplane. The default aircraft meshes to 50 panels at the default
``spanwise_resolution=1``/``chordwise_resolution=1``, so each solve is cheap.

Cases are chosen for the branches, since the reported numbers hide most of
them:

* ``munk`` reaches ``munk_apparent_mass_factor``'s low clamp (fineness below
  4), high clamp (above 20), the two exact table endpoints, and an interior
  interpolation.
* ``fuselage_cm_alpha`` runs on the nominal aircraft, whose fuselage extends
  well past the wing trailing edge, so both the fore-body (``eta_local = 1``)
  and after-body (downwash-reduced) branches of the integral are reached in one
  call. A ``low_cl_alpha`` case reaches the ``max(0.1, cl_alpha)`` downwash
  floor, and a ``no_hstab`` case reaches the ``x_h = x_te + 3 c_ref`` fallback
  for the tail station when there is no horizontal stabilizer to read one off.
* ``static_margin`` and ``neutral_point`` are the two low-speed reference
  probes; ``neutral_point`` also reports the fuselage- and tail-efficiency-
  corrected neutral point and lift-curve slope.
* ``autobalance`` shifts the CG to two different target static margins and
  records the static margin measured *before* the shift (the free byproduct
  callers use for penalties) and the resulting ``xyz_ref[0]``.
* ``stability_and_trim`` is the cruise-condition three-point probe. It carries
  cases at two flight conditions with the horizontal stabilizer present (the
  full 2x2 alpha/incidence trim), and a ``no_hstab`` case that degrades to the
  pure-alpha trim -- ``trim_ih_deg`` NaN, ``cl_ih``/``cm_ih`` zero.
* ``tail_volume`` reaches all three of ``tail_volume_coefficients``' exits:
  three wings (both Vh and Vv defined), two wings (Vh only), one wing (neither).

A NaN incidence is written as JSON ``null`` and mapped back to a NaN on the
Rust side, the convention ``gen_aero_analysis.py`` and ``gen_prop_cycle.py``
already set -- NaN has no portable JSON spelling.
"""

from __future__ import annotations

import copy
import math

import numpy as np

import _framework

_framework.add_alas_to_path()

from alas.config.analysis_config import AnalysisConfig  # noqa: E402
from alas.config.geometry_config import GeometryConfig  # noqa: E402
from alas.geometry.aircraft_builder import AircraftBuilder  # noqa: E402
from alas.physics.stability import (  # noqa: E402
    autobalance,
    fuselage_cm_alpha,
    munk_apparent_mass_factor,
    neutral_point,
    stability_and_trim,
    static_margin,
    tail_volume_coefficients,
)

HSTAB = "Horizontal Stabilizer"
VSTAB = "Vertical Stabilizer"

MUNK_CASES = {
    "below_clamp": 2.0,
    "at_low_endpoint": 4.0,
    "interior": 9.0,
    "at_high_endpoint": 20.0,
    "above_clamp": 25.0,
}

# dCL/dalpha per radian -- what neutral_point/stability_and_trim pass in.
FUSELAGE_CM_ALPHA_CASES = {
    "typical": 5.5,
    "shallow": 4.0,
    "steep": 7.0,
    # Below the 0.1 downwash floor inside fuselage_cm_alpha.
    "low_cl_alpha": 0.05,
}

AUTOBALANCE_CASES = {
    "target_10pct": 0.10,
    "target_05pct": 0.05,
}

TRIM_CASES = {
    "cruise": dict(cl_target=0.5, mach=0.82, altitude=11000.0, has_hstab=True),
    "climb": dict(cl_target=0.75, mach=0.6, altitude=6000.0, has_hstab=True),
    # No horizontal stabilizer: degrades to the pure-alpha trim fallback.
    "no_hstab": dict(cl_target=0.5, mach=0.82, altitude=11000.0, has_hstab=False),
}


def _scalar(x) -> float | None:
    """A scalar for the fixture: NaN becomes ``null`` (mapped back to NaN)."""
    value = float(x)
    return None if math.isnan(value) else value


def _without_wing(plane, name: str):
    """A deep copy of the aircraft with the wing named ``name`` removed."""
    stripped = copy.deepcopy(plane)
    stripped.wings = [w for w in stripped.wings if w.name != name]
    return stripped


def _first_wings(plane, count: int):
    """A deep copy keeping only the first ``count`` wings, in order -- what
    ``tail_volume_coefficients`` indexes by position."""
    stripped = copy.deepcopy(plane)
    stripped.wings = stripped.wings[:count]
    return stripped


def main() -> None:
    builder = AircraftBuilder(GeometryConfig())
    plane = builder.build()
    analysis = AnalysisConfig()

    plane_no_hstab = _without_wing(plane, HSTAB)

    munk = {
        name: {"inputs": {"fineness": fineness}, "factor": float(munk_apparent_mass_factor(fineness))}
        for name, fineness in MUNK_CASES.items()
    }

    fuselage = {}
    for name, cl_alpha in FUSELAGE_CM_ALPHA_CASES.items():
        fuselage[name] = {
            "inputs": {"cl_alpha": cl_alpha, "has_hstab": True},
            "cm_alpha": float(fuselage_cm_alpha(plane, cl_alpha)),
        }
    fuselage["no_hstab"] = {
        "inputs": {"cl_alpha": 5.5, "has_hstab": False},
        "cm_alpha": float(fuselage_cm_alpha(plane_no_hstab, 5.5)),
    }

    sm = {"static_margin": float(static_margin(plane, analysis))}

    x_np, np_sm, np_cl_alpha = neutral_point(plane, analysis)
    neutral = {
        "x_np": float(x_np),
        "static_margin": float(np_sm),
        "cl_alpha": float(np_cl_alpha),
    }

    autobalanced = {}
    for name, target in AUTOBALANCE_CASES.items():
        # autobalance mutates xyz_ref[0] in place, so give it a fresh copy or
        # every case after the first would balance an already-shifted aircraft.
        candidate = copy.deepcopy(plane)
        x_ref_before = float(candidate.xyz_ref[0])
        _, sm_before = autobalance(candidate, target, analysis)
        autobalanced[name] = {
            "inputs": {"target_static_margin": target},
            "sm_before": _scalar(sm_before),
            "xyz_ref_x_before": x_ref_before,
            "xyz_ref_x_after": float(candidate.xyz_ref[0]),
        }

    trim = {}
    for name, params in TRIM_CASES.items():
        target = plane if params["has_hstab"] else plane_no_hstab
        result = stability_and_trim(
            target, analysis, params["cl_target"], params["mach"], params["altitude"]
        )
        trim[name] = {
            "inputs": params,
            "x_np": float(result.x_np),
            "static_margin": float(result.static_margin),
            "cl_alpha": float(result.cl_alpha),
            "trim_alpha_deg": float(result.trim_alpha_deg),
            "trim_ih_deg": _scalar(result.trim_ih_deg),
            "cl_ih": float(result.cl_ih),
            "cm_ih": float(result.cm_ih),
        }

    def _tail_volume(target, n_wings: int):
        vh, vv = tail_volume_coefficients(target)
        return {
            "inputs": {"n_wings": n_wings},
            "vh": None if vh is None else float(vh),
            "vv": None if vv is None else float(vv),
        }

    tail_volume = {
        "three_wings": _tail_volume(plane, 3),
        "two_wings": _tail_volume(_first_wings(plane, 2), 2),
        "one_wing": _tail_volume(_first_wings(plane, 1), 1),
    }

    # A fixture in which no munk case reaches either clamp would pass against a
    # port that dropped the max/min guards entirely.
    if munk["below_clamp"]["factor"] != munk["at_low_endpoint"]["factor"]:
        raise SystemExit("munk below-clamp case does not reach the low clamp")
    if munk["above_clamp"]["factor"] != munk["at_high_endpoint"]["factor"]:
        raise SystemExit("munk above-clamp case does not reach the high clamp")
    # The pure-alpha fallback is the whole reason `has_hstab=False` exists.
    if trim["no_hstab"]["trim_ih_deg"] is not None:
        raise SystemExit("no_hstab trim case did not degrade to a NaN incidence")
    if trim["cruise"]["trim_ih_deg"] is None:
        raise SystemExit("the cruise trim case produced no incidence to trim with")
    # tail_volume must reach each of its three exits, or a port that returned a
    # value where it should return None would pass.
    if tail_volume["two_wings"]["vv"] is not None:
        raise SystemExit("two-wing tail-volume case still reported a Vv")
    if tail_volume["one_wing"]["vh"] is not None:
        raise SystemExit("one-wing tail-volume case still reported a Vh")

    _framework.write(
        "stab",
        "trim",
        {
            "airplane": {
                "s_ref": float(plane.s_ref),
                "c_ref": float(plane.c_ref),
                "b_ref": float(plane.b_ref),
                "xyz_ref": [float(v) for v in np.asarray(plane.xyz_ref)],
                "wing_names": [w.name for w in plane.wings],
                "fuselage_names": [f.name for f in plane.fuselages],
            },
            "munk": munk,
            "fuselage_cm_alpha": fuselage,
            "static_margin": sm,
            "neutral_point": neutral,
            "autobalance": autobalanced,
            "stability_and_trim": trim,
            "tail_volume": tail_volume,
        },
        description=(
            "alas.physics.stability on the nominal AircraftBuilder aircraft: "
            "munk_apparent_mass_factor, fuselage_cm_alpha, static_margin, "
            "neutral_point, autobalance, stability_and_trim and "
            "tail_volume_coefficients, with cases chosen to reach both Munk "
            "clamps, the downwash floor, the no-horizontal-stabilizer trim and "
            "fuselage fallbacks, and all three tail-volume exits"
        ),
    )


if __name__ == "__main__":
    main()
