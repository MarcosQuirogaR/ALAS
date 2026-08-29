# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-aero::analysis``: ``alas/physics/aerodynamics.py``'s hybrid engine --
the AeroSandbox VLM for lift, induced drag and pitching moment, plus this
program's own Raymer parasite-drag buildup and Korn wave-drag rise.

Unlike ``gen_aero_vlm.py``, which stands a small hand-built probe airplane in
for the real geometry, this fixture runs on **the actual nominal aircraft**:
``AircraftBuilder(GeometryConfig()).build()``, the same object
``golden/geom/builder.json`` already pins. It has to. ``parasite_drag`` reads
the fuselage's end stations, the nacelle count, every wing's wetted area and
the morphed root section's real thickness-to-chord, and none of those exists
on a probe geometry -- a hand-built stand-in would have exercised the formula
and not the aircraft. The default aircraft meshes to 50 panels at the default
``spanwise_resolution=1``/``chordwise_resolution=1``, so running it here is
cheap.

Cases are chosen for the branches, since the totals hide most of them:

* ``pg_beta`` reaches ``swept_pg_beta``'s ``M cos(sweep)`` clamp at 0.95 (the
  ``1e-3`` floor underneath it is unreachable while that clamp holds, and a
  Rust unit test states that rather than a case pretending to reach it), the
  sign-stripping on a negative Mach and a negative sweep, and the unclamped
  path.
* ``wave`` reaches all three of ``wave_drag``'s exits: below the onset Mach,
  above the onset but below the drag-divergence Mach, and the quartic rise.
  The nominal cruise point (M 0.82, CL 0.5) is the *middle* one -- with a
  32 deg sweep and a 14% section, M_dd lands at 0.844 -- so a fixture built
  only from the design point would never have seen the rise at all.
* ``parasite``/``components`` vary Mach, altitude and CL, and one case drops
  the engines (``include_engines=False``) so the nacelle loop's contribution
  is separable from the wing and fuselage terms.
* ``no_wings`` is the one case that reaches ``_section_thickness``' fallback:
  upstream wraps the root-section lookup in a bare ``except`` and answers
  0.12 when there is no wing to read. It is observable through ``wave_drag``,
  which is the only consumer of that thickness that is not itself a sum over
  wings.
* ``quick``/``trimmed``/``sweep`` are the three VLM-fed entry points.
  ``trimmed`` carries a case at a non-NaN incidence (the horizontal
  stabilizer's twist is overwritten before the solve), one at NaN (it is
  not), and one at ``cl_alpha = 0`` to reach the guard that reports the
  uncorrected angle. ``sweep`` carries a single-point case so the
  compressibility correction of the alpha axis is skipped, which the
  15-point default never does.

``trimmed_performance`` takes a ``StabilityTrimResult`` upstream -- a P7 type.
It reads exactly three fields off it (``trim_alpha_deg``, ``trim_ih_deg``,
``cl_alpha``), so those three are what this fixture passes and what the Rust
signature takes; see ``docs/PORTING.md``.

A NaN incidence is written as JSON ``null`` and mapped back to a NaN on the
Rust side, the convention ``gen_prop_cycle.py`` already set -- NaN has no
portable JSON spelling.
"""

from __future__ import annotations

import copy
import math
from types import SimpleNamespace

import numpy as np

import _framework

_framework.add_alas_to_path()

from alas.config.analysis_config import AnalysisConfig  # noqa: E402
from alas.config.geometry_config import GeometryConfig  # noqa: E402
from alas.config.physics_config import DragModelConfig  # noqa: E402
from alas.geometry.aircraft_builder import AircraftBuilder  # noqa: E402
from alas.physics.aerodynamics import (  # noqa: E402
    AeroAnalysis,
    compressible_report_alpha,
    swept_pg_beta,
)

# The nominal aircraft's own quarter-chord sweep, which is what every call
# site passes (`dv.sweep_deg`); the cases that vary it do so to reach a
# branch, not because anything configures it away from here.
SWEEP_DEG = 32.0

PG_BETA_CASES = {
    "subsonic": dict(mach=0.5, sweep_deg=32.0),
    "unswept": dict(mach=0.7, sweep_deg=0.0),
    # M cos(sweep) = 0.98 > 0.95, so this is the clamped branch.
    "clamped": dict(mach=0.98, sweep_deg=0.0),
    "negative_mach": dict(mach=-0.6, sweep_deg=-25.0),
    "high_sweep": dict(mach=0.85, sweep_deg=45.0),
}

REPORT_ALPHA_CASES = {
    "cruise": dict(alpha_inc_deg=4.5, alpha_0L_deg=-2.0, mach=0.82, sweep_deg=32.0),
    "at_zero_lift": dict(alpha_inc_deg=-2.0, alpha_0L_deg=-2.0, mach=0.82, sweep_deg=32.0),
    "incompressible": dict(alpha_inc_deg=6.0, alpha_0L_deg=-1.0, mach=0.0, sweep_deg=32.0),
    "negative": dict(alpha_inc_deg=-5.0, alpha_0L_deg=1.5, mach=0.7, sweep_deg=20.0),
}

PARASITE_CASES = {
    "cruise": dict(mach=0.82, altitude=11000.0, cl=0.5, include_engines=True),
    "low_and_slow": dict(mach=0.25, altitude=0.0, cl=1.2, include_engines=True),
    "high_and_fast": dict(mach=0.88, altitude=13000.0, cl=0.35, include_engines=True),
    # Isolates the nacelle loop: the same condition as `cruise` on an
    # airplane whose fuselage list is the body alone.
    "no_engines": dict(mach=0.82, altitude=11000.0, cl=0.5, include_engines=False),
}

WAVE_CASES = {
    # Below DragModelConfig.wave_drag_onset_mach (0.6): the early return.
    "below_onset": dict(mach=0.5, cl=0.5),
    # Above the onset Mach but below M_dd (0.844 here): the second zero.
    "below_divergence": dict(mach=0.82, cl=0.5),
    # Above M_dd: the quartic rise.
    "diverged": dict(mach=0.86, cl=0.9),
    "deep_rise": dict(mach=0.92, cl=1.0),
}

COMPONENT_CASES = {
    "cruise": dict(mach=0.82, altitude=11000.0, cl=0.5, cd_induced=0.0125),
    "with_wave": dict(mach=0.88, altitude=11000.0, cl=0.9, cd_induced=0.04),
    "sea_level": dict(mach=0.3, altitude=0.0, cl=0.8, cd_induced=0.02),
}

QUICK_CASES = {
    "cruise": dict(cl_target=0.5, mach=0.82, altitude=11000.0, spanwise=1, chordwise=1),
    "climb": dict(cl_target=0.75, mach=0.6, altitude=6000.0, spanwise=1, chordwise=1),
    # The fidelity-preset mesh the full-analysis path uses, at a reduced
    # chordwise count so the panel matrix stays a few hundred rows.
    "fine_mesh": dict(cl_target=0.55, mach=0.82, altitude=11000.0, spanwise=2, chordwise=4),
}

TRIMMED_CASES = {
    "nominal": dict(
        trim_alpha_deg=2.4,
        trim_ih_deg=-1.5,
        cl_alpha=0.11,
        mach=0.82,
        altitude=11000.0,
    ),
    # trim_ih_deg is NaN, so the horizontal stabilizer's twist is left alone.
    "untrimmed_incidence": dict(
        trim_alpha_deg=3.0,
        trim_ih_deg=float("nan"),
        cl_alpha=0.1,
        mach=0.82,
        altitude=11000.0,
    ),
    # cl_alpha is zero, so the compressibility correction is skipped and the
    # trim alpha is reported as it stands.
    "zero_cl_alpha": dict(
        trim_alpha_deg=1.8,
        trim_ih_deg=-2.5,
        cl_alpha=0.0,
        mach=0.82,
        altitude=11000.0,
    ),
}

SWEEP_CASES = {
    "cruise": dict(mach=0.82, altitude=11000.0, n_points=15, alpha_min=-4.0, alpha_max=10.0),
    "sea_level": dict(mach=0.3, altitude=0.0, n_points=7, alpha_min=-2.0, alpha_max=8.0),
    # One point, so `cl_arr.size >= 2` fails and the alpha axis is reported
    # uncorrected -- the branch the 15-point default never takes.
    "single_point": dict(mach=0.82, altitude=11000.0, n_points=1, alpha_min=2.0, alpha_max=2.0),
}


def _analysis(spanwise: int = 1, chordwise: int = 1, **overrides) -> AnalysisConfig:
    config = AnalysisConfig()
    config.spanwise_resolution = spanwise
    config.chordwise_resolution = chordwise
    for name, value in overrides.items():
        setattr(config, name, value)
    return config


def _aero(plane, analysis: AnalysisConfig) -> AeroAnalysis:
    return AeroAnalysis(
        plane,
        sweep_deg=SWEEP_DEG,
        geometry=GeometryConfig(),
        drag_model=DragModelConfig(),
        analysis=analysis,
    )


def _scalar(x) -> float | None:
    """A scalar for the fixture: NaN becomes ``null`` (mapped back to NaN)."""
    value = float(x)
    return None if math.isnan(value) else value


def _wingless(plane):
    """The same aircraft with its wings removed -- see the module doc."""
    stripped = copy.deepcopy(plane)
    stripped.wings = []
    return stripped


def main() -> None:
    builder = AircraftBuilder(GeometryConfig())
    plane = builder.build()
    plane_no_engines = builder.build(include_engines=False)

    aero = _aero(plane, _analysis())
    section_thickness = float(plane.wings[0].xsecs[0].airfoil.max_thickness())

    pg_beta = {
        name: {"inputs": params, "beta": float(swept_pg_beta(**params))}
        for name, params in PG_BETA_CASES.items()
    }
    report_alpha = {
        name: {"inputs": params, "alpha_deg": float(compressible_report_alpha(**params))}
        for name, params in REPORT_ALPHA_CASES.items()
    }

    parasite = {}
    for name, params in PARASITE_CASES.items():
        target = plane if params["include_engines"] else plane_no_engines
        value = _aero(target, _analysis()).parasite_drag(
            params["mach"], params["altitude"], params["cl"]
        )
        parasite[name] = {"inputs": params, "cd_parasite": float(value)}

    wave = {
        name: {
            "inputs": params,
            "cd_wave": float(aero.wave_drag(params["mach"], params["cl"])),
        }
        for name, params in WAVE_CASES.items()
    }

    wingless = _aero(_wingless(plane), _analysis())
    no_wings = {
        "section_thickness": 0.12,
        "cd_wave": float(wingless.wave_drag(0.86, 0.9)),
        "cd_parasite": float(wingless.parasite_drag(0.82, 11000.0, 0.5)),
    }

    components = {}
    for name, params in COMPONENT_CASES.items():
        comps = aero.drag_components(
            params["mach"], params["altitude"], params["cl"], params["cd_induced"]
        )
        components[name] = {
            "inputs": params,
            "cd_parasite": float(comps.cd_parasite),
            "cd_induced": float(comps.cd_induced),
            "cd_wave": float(comps.cd_wave),
            "cd_total": float(comps.cd_total),
        }

    quick = {}
    for name, params in QUICK_CASES.items():
        out = _aero(plane, _analysis(params["spanwise"], params["chordwise"])).quick_performance(
            params["cl_target"], params["mach"], params["altitude"]
        )
        quick[name] = {
            "inputs": params,
            "l_over_d": float(out["L/D"]),
            "alpha": float(out["alpha"]),
            "cd": float(out["CD"]),
            "cl": float(out["CL"]),
        }

    trimmed = {}
    for name, params in TRIMMED_CASES.items():
        trim = SimpleNamespace(
            trim_alpha_deg=params["trim_alpha_deg"],
            trim_ih_deg=params["trim_ih_deg"],
            cl_alpha=params["cl_alpha"],
        )
        out = aero.trimmed_performance(trim, params["mach"], params["altitude"])
        trimmed[name] = {
            "inputs": {**params, "trim_ih_deg": _scalar(params["trim_ih_deg"])},
            "l_over_d": float(out["L/D"]),
            "alpha": float(out["alpha"]),
            "i_h": _scalar(out["i_h"]),
            "cd": float(out["CD"]),
            "cl": float(out["CL"]),
            "cm_residual": float(out["Cm_residual"]),
        }
        # The perturb/restore round trip must leave the aircraft as it was,
        # or every case after this one would be running on a different
        # geometry than the one this fixture claims.
        hstab = next(w for w in plane.wings if w.name == "Horizontal Stabilizer")
        if any(float(xs.twist) != -2.0 for xs in hstab.xsecs):
            raise SystemExit(
                f"case {name}: trimmed_performance left the horizontal stabilizer's "
                "twist altered; every later case would run on a different aircraft"
            )

    sweep = {}
    for name, params in SWEEP_CASES.items():
        analysis = _analysis(
            sweep_n_points=params["n_points"],
            sweep_alpha_min_deg=params["alpha_min"],
            sweep_alpha_max_deg=params["alpha_max"],
        )
        out = _aero(plane, analysis).run_sweep(params["mach"], params["altitude"])
        sweep[name] = {
            "inputs": params,
            **{
                key: [float(v) for v in np.asarray(out[fixture_key])]
                for key, fixture_key in (
                    ("alpha", "alpha"),
                    ("cl", "CL"),
                    ("cd", "CD"),
                    ("cd_induced", "CD_induced"),
                    ("cd_wave", "CD_wave"),
                    ("cd_parasite", "CD_parasite"),
                    ("cm", "Cm"),
                    ("l_over_d", "L/D"),
                )
            },
        }

    # A fixture in which no case reaches the wave-drag rise would pass against
    # a port that returned zero unconditionally, which is most of what this
    # module's transonic half does.
    if not any(case["cd_wave"] > 0.0 for case in wave.values()):
        raise SystemExit("no case reaches the Korn wave-drag rise")
    if not any(case["cd_wave"] == 0.0 for case in wave.values()):
        raise SystemExit("no case reaches a zero wave drag")
    # Likewise for the clamp: swept_pg_beta's 0.95 ceiling is invisible in a
    # fixture whose Mach-normal components all sit below it.
    if not any(
        abs(p["mach"]) * abs(math.cos(math.radians(p["sweep_deg"]))) > 0.95
        for p in PG_BETA_CASES.values()
    ):
        raise SystemExit("no swept_pg_beta case reaches the M cos(sweep) clamp")
    if not any(case["cd_wave"] > 0.0 for case in components.values()):
        raise SystemExit("no drag_components case carries a nonzero wave-drag term")

    _framework.write(
        "aero",
        "analysis",
        {
            "sweep_deg": SWEEP_DEG,
            "airplane": {
                "s_ref": float(plane.s_ref),
                "c_ref": float(plane.c_ref),
                "b_ref": float(plane.b_ref),
                "xyz_ref": [float(v) for v in np.asarray(plane.xyz_ref)],
                "wing_names": [w.name for w in plane.wings],
                "fuselage_names": [f.name for f in plane.fuselages],
                "section_thickness": section_thickness,
            },
            "pg_beta": pg_beta,
            "report_alpha": report_alpha,
            "parasite": parasite,
            "wave": wave,
            "no_wings": no_wings,
            "components": components,
            "quick": quick,
            "trimmed": trimmed,
            "sweep": sweep,
        },
        description=(
            "alas.physics.aerodynamics on the nominal AircraftBuilder aircraft: "
            "swept_pg_beta, compressible_report_alpha, parasite_drag, wave_drag, "
            "drag_components, quick_performance, trimmed_performance and run_sweep, "
            "with cases chosen to reach the Prandtl-Glauert clamp, all three "
            "wave-drag exits, the wingless thickness fallback and the "
            "single-point sweep"
        ),
    )


if __name__ == "__main__":
    main()
