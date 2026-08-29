# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-aero::vorlax``: SUAVE's VORLAX-derived vortex lattice method.

This is the second vortex lattice in the port. ``alas-aero::asb_vlm`` is
AeroSandbox's, reached from ``alas/physics/aerodynamics.py``; this one is
SUAVE's, and the only thing in the whole reference that reaches it is the
mission runner attaching ``SUAVE.Analyses.Aerodynamics.Fidelity_Zero()`` to an
assembled vehicle. ``Fidelity_Zero`` owns a ``Vortex_Lattice`` sub-analysis
whose ``sample_training`` calls ``VLM()`` once, on a grid of angles of attack
and Mach numbers, and every mission number is a spline through that grid.

So the fixture drives the real objects. It builds the vehicle the runner
builds, runs ``configs_setup``/``simple_sizing``, reaches into the finalized
``Fidelity_Zero`` for the ``Vortex_Lattice`` sub-analysis, and calls
``generate_vortex_distribution`` and ``VLM`` with *its* settings rather than
with a hand-written stand-in -- the mistake ``gen_aero_drag_buildup.py``'s own
docstring records having made.

What it records, and why each part is needed:

* **The settings**, at ``exact``. Seven of them decide which branches exist at
  all: ``model_fuselage`` and ``model_nacelle`` are what make
  ``generate_fuselage_and_nacelle_vortex_distribution`` a no-op,
  ``discretize_control_surfaces`` is what keeps the three control surfaces on
  the main wing from becoming six more lifting surfaces, and
  ``use_VORLAX_matrix_calculation`` selects the boundary condition. A port
  written against the wrong value of any of them agrees with itself and with
  nothing else.

* **The whole vortex distribution.** Panel corners, horseshoe legs, control
  points, trailing-edge coordinates, per-strip chord and incidence, the strip
  and wing break indices, the panel normals and areas. This is where a
  panelization bug lives, and a bug there still integrates to a plausible
  total: recording only ``CL`` would let a wing meshed a panel out of place
  pass.

* **Every intermediate of one solve.** The right-hand side, the solved vortex
  strengths, the panel pressure coefficients and the per-strip loads, not only
  the eight reported coefficients. `gamma` in particular: a wrong influence
  matrix can still produce a coincidentally close lift.

* **The full training grid.** Ten angles of attack against the eight subsonic
  Mach numbers ``Fidelity_Zero`` overrides ``Vortex_Lattice``'s default
  sixteen with -- the exact call ``sample_training`` makes, vectorized into
  one 80-row solve. That is the call the mission actually depends on, and it
  is also the input to ``alas-aero::lift_surrogate``.

The generator refuses to write a fixture in which the supersonic branch has
become reachable (it is not, and the ``supersonic`` kernel is deliberately
untranslated), in which any wing has grown ``Segments`` or a discretized
control surface, or in which the vehicle has stopped having a wing whose
dihedral, twist and sweep are all non-zero -- a planar untwisted wing would
agree with a port that dropped the twist rotation entirely.

Usage::

    & ".venv/Scripts/python.exe"       golden/generators/gen_aero_vorlax.py --stage=build
    & ".suave-venv/Scripts/python.exe" golden/generators/gen_aero_vorlax.py --stage=solve
"""

from __future__ import annotations

import argparse
import json

import _framework

# What stage 1 writes and stage 2 reads. The two stages need different
# interpreters, so the vehicle request has to survive a process boundary.
_SCRATCH = _framework.GOLDEN_DIR / "aero" / "_vorlax_request.json"

# The single-condition solves. Chosen so that every term in the force
# integration is exercised by at least one of them: a zero-alpha case where
# the lift is nearly the camber contribution alone, a negative alpha, the two
# ends of the training Mach range, and two cases at non-zero sideslip and
# non-zero body rates, which are the only ones that reach the `ONSET`,
# `SICPLE` and rotation terms at all.
_CASES = [
    # (tag, alpha deg, mach, sideslip deg, pitch rate, roll rate, yaw rate)
    ("alpha_zero", 0.0, 0.20, 0.0, 0.0, 0.0, 0.0),
    ("alpha_negative", -5.0, 0.20, 0.0, 0.0, 0.0, 0.0),
    ("alpha_cruise", 2.0, 0.85, 0.0, 0.0, 0.0, 0.0),
    ("alpha_high", 10.0, 0.30, 0.0, 0.0, 0.0, 0.0),
    ("mach_zero", 5.0, 0.00, 0.0, 0.0, 0.0, 0.0),
    ("sideslip", 3.0, 0.50, 6.0, 0.0, 0.0, 0.0),
    ("body_rates", 3.0, 0.50, 0.0, 0.02, 0.03, 0.01),
    ("sideslip_and_rates", 4.0, 0.75, -4.0, 0.015, -0.02, 0.012),
]


# ======================================================================
#  Stage 1: build the vehicle request (run under .venv)
# ======================================================================


def _stage_build() -> None:
    """Build the vehicle request dict through the ALAS pipeline."""
    _framework.add_alas_to_path()

    from alas.analysis.full_analysis import FullAnalysis  # noqa: PLC0415
    from alas.config.design_variables import DesignVector  # noqa: PLC0415
    from alas.config.settings import ALASConfig  # noqa: PLC0415
    from alas.integration.suave_vehicle import build_vehicle_request  # noqa: PLC0415

    config = ALASConfig()
    report = FullAnalysis(config).run(DesignVector.default(), verbose=False)
    vehicle_request = build_vehicle_request(report, config)

    _SCRATCH.parent.mkdir(parents=True, exist_ok=True)
    with _SCRATCH.open("w", encoding="utf-8", newline="\n") as handle:
        json.dump(vehicle_request, handle, indent=2)
        handle.write("\n")
    print(f"wrote {_SCRATCH.relative_to(_framework.GOLDEN_DIR)}")


# ======================================================================
#  Stage 2: run the vortex lattice (run under .suave-venv)
# ======================================================================


def _flat(array) -> list:
    """One numpy array as a flat list of Python floats."""
    import numpy as np  # noqa: PLC0415

    return [float(v) for v in np.asarray(array, dtype=float).reshape(-1)]


def _flat_int(array) -> list:
    """One numpy array as a flat list of Python ints."""
    import numpy as np  # noqa: PLC0415

    return [int(v) for v in np.asarray(array).reshape(-1)]


def _record_geometry(geometry) -> dict:
    """Everything ``generate_vortex_distribution`` and ``VLM`` read.

    A parity test that rebuilt this from the vehicle request would be
    checking two vehicle builders against each other rather than checking the
    vortex lattice, and `alas-aero::drag_buildup`'s row records what that
    costs. So the wing geometry is handed over as data.
    """
    wings = []
    for wing in geometry.wings.values():
        wings.append(
            {
                "tag": wing.tag,
                "symmetric": bool(wing.symmetric),
                "vertical": bool(wing.vertical),
                "vortex_lift": bool(wing.vortex_lift),
                "span_projected_m": float(wing.spans.projected),
                "chord_root_m": float(wing.chords.root),
                "chord_tip_m": float(wing.chords.tip),
                "chord_mean_aerodynamic_m": float(wing.chords.mean_aerodynamic),
                "taper": float(wing.taper),
                "aspect_ratio": float(wing.aspect_ratio),
                "sweep_quarter_chord_rad": float(wing.sweeps.quarter_chord),
                # `None` here is what makes `make_VLM_wings` derive the
                # leading-edge sweep rather than copy one, so it is recorded
                # as `null` rather than coerced to a number.
                "sweep_leading_edge_rad": (
                    None
                    if wing.sweeps.leading_edge is None
                    else float(wing.sweeps.leading_edge)
                ),
                "twist_root_rad": float(wing.twists.root),
                "twist_tip_rad": float(wing.twists.tip),
                "dihedral_rad": float(wing.dihedral),
                "thickness_to_chord": float(wing.thickness_to_chord),
                "area_reference_m2": float(wing.areas.reference),
                "origin_m": [float(v) for v in wing.origin[0]],
                "n_segments": len(wing.Segments.keys()),
                "n_control_surfaces": len(wing.control_surfaces.keys()),
                "aerodynamic_center_m": [float(v) for v in wing.aerodynamic_center],
            }
        )

    return {
        "reference_area_m2": float(geometry.reference_area),
        "center_of_gravity_m": [
            float(v) for v in geometry.mass_properties.center_of_gravity[0]
        ],
        "wings": wings,
    }


def _record_distribution(vd) -> dict:
    """The whole vortex distribution, panel by panel."""
    panels = {
        name: _flat(getattr(vd, name))
        for name in (
            "XAH", "YAH", "ZAH",
            "XBH", "YBH", "ZBH",
            "XCH", "YCH", "ZCH",
            "XA1", "YA1", "ZA1",
            "XA2", "YA2", "ZA2",
            "XB1", "YB1", "ZB1",
            "XB2", "YB2", "ZB2",
            "XAC", "YAC", "ZAC",
            "XBC", "YBC", "ZBC",
            "XC", "YC", "ZC",
            "XA_TE", "YA_TE", "ZA_TE",
            "XB_TE", "YB_TE", "ZB_TE",
        )
    }

    return {
        "n_w": int(vd.n_w),
        "n_cp": int(vd.n_cp),
        "n_sw": _flat_int(vd.n_sw),
        "n_cw": _flat_int(vd.n_cw),
        "chordwise_breaks": _flat_int(vd.chordwise_breaks),
        "spanwise_breaks": _flat_int(vd.spanwise_breaks),
        "symmetric_wings": _flat_int(vd.symmetric_wings),
        "leading_edge_indices": _flat_int(vd.leading_edge_indices),
        "trailing_edge_indices": _flat_int(vd.trailing_edge_indices),
        "panels_per_strip": _flat_int(vd.panels_per_strip),
        "chordwise_panel_number": _flat_int(vd.chordwise_panel_number),
        "exposed_leading_edge_flag": _flat_int(vd.exposed_leading_edge_flag),
        "vortex_lift": [bool(v) for v in vd.vortex_lift],
        "wing_areas_m2": _flat(vd.wing_areas),
        "chord_lengths_m": _flat(vd.chord_lengths),
        "tangent_incidence_angle": _flat(vd.tangent_incidence_angle),
        "panel_areas_m2": _flat(vd.panel_areas),
        "normals": [[float(c) for c in row] for row in vd.normals],
        "SLOPE": _flat(vd.SLOPE),
        "SLE": _flat(vd.SLE),
        "D": _flat(vd.D),
        "panels": panels,
    }


def _build_conditions(cases):
    """One multi-row aerodynamic state holding every single-condition case."""
    import numpy as np  # noqa: PLC0415
    import SUAVE  # noqa: PLC0415
    from SUAVE.Core import Units  # noqa: PLC0415

    conditions = SUAVE.Analyses.Mission.Segments.Conditions.Aerodynamics()

    column = lambda values: np.atleast_2d(np.array(values, dtype=float)).T  # noqa: E731

    conditions.aerodynamics.angle_of_attack = column(
        [case[1] * Units.deg for case in cases]
    )
    conditions.freestream.mach_number = column([case[2] for case in cases])
    conditions.aerodynamics.side_slip_angle = column(
        [case[3] * Units.deg for case in cases]
    )
    conditions.stability.dynamic.pitch_rate = column([case[4] for case in cases])
    conditions.stability.dynamic.roll_rate = column([case[5] for case in cases])
    conditions.stability.dynamic.yaw_rate = column([case[6] for case in cases])

    # VLM divides the body rates by this, and substitutes 1e-6 for a zero
    # itself when `use_surrogate` is set. The single-condition cases run with
    # a real speed so that the rate terms are the ones a segment would see;
    # the training grid below is what reaches the zero-velocity substitution.
    speed_of_sound = 340.294
    conditions.freestream.velocity = column(
        [max(case[2], 1e-3) * speed_of_sound for case in cases]
    )

    return conditions


def _record_results(results, n_cases: int) -> list:
    """One entry per case, with the intermediates as well as the totals."""
    import numpy as np  # noqa: PLC0415

    entries = []
    for i in range(n_cases):
        entries.append(
            {
                "CL": float(results.CL[i, 0]),
                "CDi": float(results.CDi[i, 0]),
                "CM": float(results.CM[i, 0]),
                "CYTOT": float(results.CYTOT[i, 0]),
                "CRTOT": float(results.CRTOT[i, 0]),
                "CRMTOT": float(results.CRMTOT[i, 0]),
                "CNTOT": float(results.CNTOT[i, 0]),
                "CYMTOT": float(results.CYMTOT[i, 0]),
                "CL_wing": _flat(results.CL_wing[i, :]),
                "CDi_wing": _flat(results.CDi_wing[i, :]),
                "cl_y": _flat(results.cl_y[i, :]),
                "cdi_y": _flat(results.cdi_y[i, :]),
                "CP": _flat(np.asarray(results.CP)[i, :]),
                "gamma": _flat(np.asarray(results.gamma)[i, :]),
            }
        )
    return entries


def _stage_solve() -> None:
    """Run the real vortex lattice and record it."""
    _framework.add_suave_to_path()

    import numpy as np  # noqa: PLC0415
    import mission_builder  # noqa: PLC0415
    import vehicle_builder  # noqa: PLC0415
    from SUAVE.Methods.Aerodynamics.Common.Fidelity_Zero.Lift.VLM import (  # noqa: PLC0415
        VLM,
    )
    from SUAVE.Methods.Aerodynamics.Common.Fidelity_Zero.Lift.generate_vortex_distribution import (  # noqa: PLC0415,E501
        generate_vortex_distribution,
    )

    if not _SCRATCH.exists():
        raise SystemExit(
            f"{_SCRATCH} not found -- run stage 1 first:\n"
            '  & ".venv/Scripts/python.exe" '
            "golden/generators/gen_aero_vorlax.py --stage=build"
        )

    vehicle_request = json.loads(_SCRATCH.read_text(encoding="utf-8"))

    print("building the SUAVE vehicle...")
    vehicle = vehicle_builder.build_vehicle(vehicle_request)
    configs = mission_builder.configs_setup(vehicle)
    mission_builder.simple_sizing(configs)

    analyses = mission_builder.analyses_setup(configs)
    aerodynamics = analyses.base.aerodynamics
    vortex_lattice = aerodynamics.process.compute.lift.inviscid_wings
    vortex_lattice.geometry = aerodynamics.geometry

    settings = vortex_lattice.settings
    geometry = vortex_lattice.geometry

    # Read the settings off the analysis. `Fidelity_Zero.initialize` copies
    # five of them across from its own settings before it trains, so do that
    # here too rather than assuming the two agree.
    outer = aerodynamics.settings
    settings.propeller_wake_model = outer.propeller_wake_model
    settings.discretize_control_surfaces = outer.discretize_control_surfaces
    settings.model_fuselage = outer.model_fuselage
    settings.model_nacelle = outer.model_nacelle
    settings.use_surrogate = outer.use_surrogate
    if outer.number_spanwise_vortices is not None:
        settings.number_spanwise_vortices = outer.number_spanwise_vortices
    if outer.number_chordwise_vortices is not None:
        settings.number_chordwise_vortices = outer.number_chordwise_vortices

    recorded_settings = {
        "number_spanwise_vortices": int(settings.number_spanwise_vortices),
        "number_chordwise_vortices": int(settings.number_chordwise_vortices),
        "spanwise_cosine_spacing": bool(settings.spanwise_cosine_spacing),
        "model_fuselage": bool(settings.model_fuselage),
        "model_nacelle": bool(settings.model_nacelle),
        "discretize_control_surfaces": bool(settings.discretize_control_surfaces),
        "propeller_wake_model": bool(settings.propeller_wake_model),
        "use_VORLAX_matrix_calculation": bool(settings.use_VORLAX_matrix_calculation),
        "leading_edge_suction_multiplier": float(
            settings.leading_edge_suction_multiplier
        ),
        "use_surrogate": bool(settings.use_surrogate),
        "floating_point_precision": settings.floating_point_precision.__name__,
    }

    # --- Refusals: each of these would let a wrong port pass -------------
    if recorded_settings["model_fuselage"] or recorded_settings["model_nacelle"]:
        raise SystemExit(
            "model_fuselage/model_nacelle are set; this port translates only the "
            "wing branch of generate_vortex_distribution, which is all "
            "Fidelity_Zero reaches"
        )
    if recorded_settings["discretize_control_surfaces"]:
        raise SystemExit(
            "discretize_control_surfaces is set; the control-surface half of "
            "make_VLM_wings is deliberately untranslated"
        )
    if recorded_settings["use_VORLAX_matrix_calculation"]:
        raise SystemExit(
            "use_VORLAX_matrix_calculation is set; the port builds the "
            "influence matrix from the panel normals, as the default does"
        )
    if recorded_settings["floating_point_precision"] != "float32":
        raise SystemExit(
            "floating_point_precision is not float32; the whole reason this "
            "row carries the f32 tier is that the kernel runs in single "
            "precision upstream"
        )
    for wing in geometry.wings.values():
        if len(wing.Segments.keys()) > 0:
            raise SystemExit(
                f"wing '{wing.tag}' has Segments; this port translates the "
                "two-segment form convert_to_segmented_wing produces from an "
                "unsegmented trapezoidal wing, which is all the runner builds"
            )

    training_mach = np.asarray(
        vortex_lattice.training.Mach, dtype=float
    ).reshape(-1)
    if (training_mach >= 1.0).any():
        raise SystemExit(
            "the training grid reaches Mach 1; the supersonic horseshoe kernel "
            "is deliberately untranslated because Fidelity_Zero overrides "
            "Vortex_Lattice's default grid with a subsonic-only one"
        )

    main_wing = geometry.wings["main_wing"]
    if main_wing.twists.root == main_wing.twists.tip:
        raise SystemExit(
            "the main wing is untwisted; a fixture without spanwise twist "
            "would agree with a port that dropped the twist rotation"
        )
    if main_wing.sweeps.quarter_chord == 0.0:
        raise SystemExit("the main wing is unswept; the sweep conversion is untested")

    print("generating the vortex distribution...")
    vd = generate_vortex_distribution(geometry, settings)
    print(f"  {vd.n_cp} panels across {vd.n_w} lifting surfaces")

    # --- The single-condition cases ---------------------------------------
    print(f"running {len(_CASES)} single-condition solves...")
    conditions = _build_conditions(_CASES)
    results = VLM(conditions, settings, geometry)
    case_results = _record_results(results, len(_CASES))

    cases = [
        {
            "tag": tag,
            "angle_of_attack_deg": alpha,
            "mach": mach,
            "side_slip_angle_deg": psi,
            "pitch_rate_rad_s": q,
            "roll_rate_rad_s": p,
            "yaw_rate_rad_s": r,
            "velocity_m_s": float(conditions.freestream.velocity[i, 0]),
            "results": case_results[i],
        }
        for i, (tag, alpha, mach, psi, q, p, r) in enumerate(_CASES)
    ]

    # --- The training grid, exactly as sample_training builds it ----------
    # This is the call the mission depends on: one vectorized solve over the
    # outer product of the angle-of-attack and Mach vectors, at zero
    # velocity, which is the input that reaches VLM's 1e-6 substitution.
    print("running the training grid (sample_training's own call)...")
    import SUAVE  # noqa: PLC0415

    aoa = np.asarray(vortex_lattice.training.angle_of_attack, dtype=float)
    mach = np.asarray(vortex_lattice.training.Mach, dtype=float)
    len_aoa, len_mach = len(aoa), len(mach)
    aoas = np.atleast_2d(np.tile(aoa, len_mach).T.flatten()).T
    machs = np.atleast_2d(np.tile(mach, len_aoa).flatten()).T

    konditions = SUAVE.Analyses.Mission.Segments.Conditions.Aerodynamics()
    konditions.aerodynamics.angle_of_attack = aoas
    konditions.freestream.mach_number = machs
    konditions.freestream.velocity = np.zeros_like(machs)

    training_results = VLM(konditions, settings, geometry)

    wing_tags = list(geometry.wings.keys())
    dim_wing_lifts = training_results.CL_wing * vd.wing_areas
    dim_wing_drags = training_results.CDi_wing * vd.wing_areas

    # `calculate_VLM`'s own regrouping: a symmetric wing occupies two columns
    # of the per-surface arrays and they are summed back onto one tag.
    per_wing_cl, per_wing_cdi = {}, {}
    i = 0
    for wing in geometry.wings.values():
        ref = wing.areas.reference
        if wing.symmetric:
            per_wing_cl[wing.tag] = _flat(
                np.sum(dim_wing_lifts[:, i:(i + 2)], axis=1) / ref
            )
            per_wing_cdi[wing.tag] = _flat(
                np.sum(dim_wing_drags[:, i:(i + 2)], axis=1) / ref
            )
            i += 1
        else:
            per_wing_cl[wing.tag] = _flat(dim_wing_lifts[:, i] / ref)
            per_wing_cdi[wing.tag] = _flat(dim_wing_drags[:, i] / ref)
        i += 1

    training = {
        "angle_of_attack_rad": _flat(aoa),
        "mach": _flat(mach),
        # Row-major over (mach, aoa), which is the order sample_training
        # builds and then reshapes: the fixture keeps the flat form so that a
        # transposed reshape in the port fails loudly.
        "angle_of_attack_flat_rad": _flat(aoas),
        "mach_flat": _flat(machs),
        "CL": _flat(training_results.CL),
        "CDi": _flat(training_results.CDi),
        "wing_tags": wing_tags,
        "wing_CL": per_wing_cl,
        "wing_CDi": per_wing_cdi,
    }

    payload = {
        "settings": recorded_settings,
        "geometry": _record_geometry(geometry),
        "vortex_distribution": _record_distribution(vd),
        "cases": cases,
        "training": training,
    }

    _framework.write(
        "aero",
        "vorlax",
        payload,
        description=(
            "SUAVE's VORLAX-derived vortex lattice on the default ALAS "
            "aircraft: the whole vortex distribution, eight single-condition "
            "solves with their intermediates, and the ten-by-eight training "
            "grid Fidelity_Zero's lift surrogate is built from."
        ),
    )

    _SCRATCH.unlink(missing_ok=True)
    print("done")


# ======================================================================


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate the VORLAX VLM fixture")
    parser.add_argument(
        "--stage",
        choices=["build", "solve"],
        required=True,
        help="'build' under .venv, 'solve' under .suave-venv",
    )
    args = parser.parse_args()

    if args.stage == "build":
        _stage_build()
    else:
        _stage_solve()


if __name__ == "__main__":
    main()
