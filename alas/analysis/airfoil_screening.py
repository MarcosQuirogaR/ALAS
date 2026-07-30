# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Airfoil-database batch screening.

Scores every airfoil in :class:`~alas.geometry.airfoils.AirfoilLibrary`
(~1665 entries) against the CURRENT design's cruise condition, so a user can
ask "which airfoil in the database best fits this aircraft" instead of
hand-picking one. Optional and entirely additive: nothing in the normal
Run/Analyze-baseline pipeline calls into this module, and it never mutates
the config it's given (every candidate is scored against an isolated
``dataclasses.replace`` copy).

Scoring strategy (see the docstring on :func:`run_airfoil_screening` for the
full rationale): a fast 2-D NeuralFoil proxy at the design's required cruise
CL, not a full 1665x re-run of the 3-D VLM pipeline -- the latter would take
on the order of tens of minutes at best (see the design notes in code
review/CONTEXT for the per-VLM-call timing this was checked against),
whereas NeuralFoil's neural-net forward pass makes the full sweep a matter of
seconds. This trades exact 3-D induced-drag/trim numbers for something that's
actually fast enough to run interactively, which is the whole point of a
*screening* tool: it's meant to produce a shortlist a user then verifies
properly (a real Run with the candidate swapped in, optionally MSES for a
transonic design) -- not a final answer.
"""

from __future__ import annotations

import dataclasses
import fnmatch
import math
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Callable, Dict, List, Optional

import numpy as np
import aerosandbox as asb

from ..config.design_variables import DesignVector
from ..config.settings import ALASConfig
from ..geometry.aircraft_builder import AircraftBuilder
from ..geometry.airfoils import AirfoilLibrary
from ..physics.performance import wing_fuel_volume_m3

if TYPE_CHECKING:
    from ..physics.mses_analysis import MSESPressureResult

# Cruise Mach at/above which panel-method (VLM) and NeuralFoil-based 2-D
# results stop being trustworthy for ranking purposes: neither models wave
# drag, so a real supercritical section's transonic advantage (delayed/softer
# drag rise near its design Mach) is invisible to both methods -- they'll
# happily rank a thin conventional section above a supercritical one that
# would actually perform far better at the real cruise Mach. There's no
# reliable way to detect "is this airfoil supercritical" from coordinates
# alone without risking a confidently-wrong heuristic, so this is surfaced as
# an explicit caveat on the result instead: the ranking is still computed and
# still useful as a shortlist, but the frontend must show this warning
# prominently rather than presenting the top candidate as a final answer.
TRANSONIC_MACH_CAVEAT = 0.75

# Real, wind-tunnel-validated transonic/supercritical sections -- genuine
# "underlying truth" reference points, not algorithmic candidates competing on
# a proxy score. NASA SC(2)-06xx/07xx/10xx (the Whitcomb-derived integral
# supercritical family transport wings actually use derivatives of --
# ALAS's own AVE/A380/A220 presets already use SC2-0714 as their root
# section), the original NASA "whitcomb" supercritical airfoil, and RAE 2822
# (the classic AGARD transonic CFD validation case, representative of
# early-generation transport technology). Deliberately excludes the
# differently-prefixed "sc10xx"/"sc1094r8"/"sc1095"/"sc1095r8"/"SC2-0714"/
# "nasasc2-0714" database entries: the "sc10xx" family is the Sikorsky
# SC1095/SC1094-R8 HELICOPTER ROTOR series (wrong domain entirely, not a
# fixed-wing transport section despite the superficially similar name), and
# "SC2-0714"/"nasasc2-0714" are alternate-source duplicates of "sc20714"
# already in this list. These always get the full 3-stage treatment
# (never gated out by Stage-1's cheap proxy score) and are flagged
# ``is_reference=True`` in the result so the UI can pin them as a physical
# sanity anchor: "how does the algorithm's pick compare to sections we KNOW
# work on real jets", not just another row fighting for a top-N slot.
REFERENCE_AIRFOILS = [
    "whitcomb",
    "rae2822",
    "sc20406",
    "sc20410",
    "sc20412",
    "sc20414",
    "sc20606",
    "sc20610",
    "sc20612",
    "sc20614",
    "sc20706",
    "sc20710",
    "sc20712",
    "sc20714",
    "sc21006",
    "sc21010",
]


@dataclass
class AirfoilCandidateResult:
    """One screened airfoil's outcome. ``status="error"`` candidates carry no
    aerodynamic fields -- they were excluded from ranking, not ranked last.

    The ``*_3d`` fields are populated only for the shortlist re-simulated as an
    actual 3-D wing in Stage 2 (see :func:`run_airfoil_screening`): the fast
    2-D NeuralFoil proxy (``l_over_d``/``cd``/``alpha_deg``) ranks the whole
    database, then the top ``refine_top_n`` are rebuilt into this design's real
    wing and evaluated with the same VLM + Raymer/Korn drag build-up the main
    pipeline uses -- which is what actually demotes a 2-D-flattering low-Reynolds
    section once induced + wave drag on this planform are counted."""

    name: str
    status: str  # "ok" | "error"
    error: Optional[str] = None
    l_over_d: Optional[float] = None
    cl: Optional[float] = None
    cd: Optional[float] = None
    alpha_deg: Optional[float] = None
    max_thickness_frac: Optional[float] = None
    tank_volume_m3: Optional[float] = None
    tank_capacity_kg: Optional[float] = None
    score: Optional[float] = None  # Stage-1 (2-D) blended score
    # Off-design robustness: mean L/D across cl_target +/- cl_band, divided by
    # the at-target L/D. ~1.0 means a flat drag bucket (L/D holds as the real
    # operating CL wanders with weight/altitude); < 1 means a peaky section
    # that only shines at exactly the design CL. Always computed (cheap -- it
    # reuses the Stage-1 NeuralFoil polar already in hand), but only folded
    # into the ranking score when ``robustness_weight > 0``.
    robustness: Optional[float] = None
    # Stage-2 (3-D wing) fields, present only when this candidate was refined.
    # Come from a REAL closed-form trim solve + one non-linear VLM point at the
    # solved condition (alas.physics.stability.stability_and_trim +
    # AeroAnalysis.trimmed_performance) -- the same rigor full_analysis.py uses
    # for the app's trusted, reported numbers -- not a cheap linear proxy.
    refined: bool = False
    l_over_d_3d: Optional[float] = None
    cd_3d: Optional[float] = None
    alpha_3d_deg: Optional[float] = None
    cm_residual_3d: Optional[float] = (
        None  # real (nonlinear) Cm at the trim point; ~0 confirms the trim solve held
    )
    static_margin_3d: Optional[float] = (
        None  # flags if this airfoil swap would break longitudinal stability
    )
    score_3d: Optional[float] = None  # Stage-2 blended score (drives final rank)
    refine_error: Optional[str] = None
    # Stage-3 (MSES) fields, present only when this candidate was MSES-verified.
    mses_verified: bool = False
    l_over_d_mses: Optional[float] = None
    cd_mses: Optional[float] = None
    cdw_mses: Optional[float] = (
        None  # wave drag -- the shock/transonic-mismatch indicator
    )
    mses_status: Optional[str] = None
    mses_error: Optional[str] = None
    # True for a curated real, wind-tunnel-validated section (REFERENCE_AIRFOILS)
    # -- always carried through every stage regardless of its raw score, as a
    # physical "this is known to actually work" anchor against the algorithmic
    # picks, not a candidate competing for a top-N slot.
    is_reference: bool = False
    # Populated only for an MSES-verified candidate: the same per-station
    # surface Cp/Mach + 2-D flowfield MSES already computes for the main
    # Run's Model Comparison tab (alas.physics.mses_analysis.
    # MSESPressureResult), reused here so each screened candidate can get its
    # own real shock/pressure-distribution figures, not just a scalar L/D.
    mses_pressure: Optional["MSESPressureResult"] = None


@dataclass
class AirfoilScreeningResult:
    """JSON-safe summary of a full screening run."""

    baseline_airfoil: str
    cruise_mach: float
    cruise_reynolds: float
    cruise_altitude_m: float
    cl_target: float
    transonic_caveat: bool
    refined_3d: bool  # whether Stage-2 3-D re-ranking ran
    n_total: int
    n_ok: int
    n_error: int
    n_refined: int = 0
    n_mses_verified: int = 0  # how many Stage-2 survivors got a real Stage-3 MSES solve
    # True when the user cancelled the sweep mid-run: the candidates below are
    # whatever had been ranked so far (a partial, still-useful shortlist), not
    # a complete database screen. The frontend labels the result accordingly.
    cancelled: bool = False
    # Top `top_n` "ok" candidates, best score first.
    candidates: List[AirfoilCandidateResult] = field(default_factory=list)
    # A capped sample of failures, for the "which airfoils are broken and why"
    # debugging the user asked for -- not every one of potentially hundreds,
    # just enough to spot a pattern (e.g. "every 3xx-series multi-element
    # entry fails the same way").
    errors: List[Dict[str, str]] = field(default_factory=list)


def _cruise_condition(config: ALASConfig, dv: DesignVector):
    """This design's cruise Mach/Reynolds/target-CL from basic level-flight
    equations (L = W at cruise), plus the geometry needed to compute them --
    self-contained, so screening works directly from whatever's on the Inputs
    page right now, without requiring a prior Run/Analyze-baseline first."""
    req = config.requirements
    plane = AircraftBuilder(config.geometry).build(dv, include_engines=False)
    wing = plane.wings[0]
    mac = float(wing.mean_aerodynamic_chord())
    s = float(wing.area())

    atmo = asb.Atmosphere(altitude=req.cruise_altitude_m)
    v = req.cruise_mach * atmo.speed_of_sound()
    q = 0.5 * atmo.density() * v**2
    weight_n = req.mtow_kg * req.gravity_m_s2
    cl_target = weight_n / (q * s)
    reynolds = atmo.density() * v * mac / atmo.dynamic_viscosity()
    return (
        float(req.cruise_mach),
        float(reynolds),
        float(cl_target),
        float(req.cruise_altitude_m),
    )


def _score_candidate(
    name: str,
    config: ALASConfig,
    dv: DesignVector,
    mach: float,
    reynolds: float,
    cl_target: float,
    usable_fraction: float,
    alphas_deg: np.ndarray,
    model_size: str,
    min_tc: float = 0.005,
    max_tc: float = 0.25,
    cl_band: float = 0.05,
) -> AirfoilCandidateResult:
    """Build a wing with ``name`` as the root airfoil (an isolated config
    copy -- ``config`` itself is never mutated) and score it. Any exception
    anywhere in this chain (airfoil load, geometry build, the NeuralFoil CST
    fit, a degenerate/multi-element database entry) is caught here and
    reported as an ``"error"`` candidate rather than aborting the sweep --
    this is the "discard airfoils that error out" requirement."""
    try:
        new_wing = dataclasses.replace(config.geometry.wing, root_airfoil=name)
        new_geometry = dataclasses.replace(config.geometry, wing=new_wing)
        cfg2 = dataclasses.replace(config, geometry=new_geometry)
        plane = AircraftBuilder(cfg2.geometry).build(dv, include_engines=False)
        wing = plane.wings[0]
        airfoil = wing.xsecs[0].airfoil  # the morphed section actually used

        aero = airfoil.get_aero_from_neuralfoil(
            alpha=alphas_deg, Re=reynolds, mach=mach, model_size=model_size
        )
        cl = np.asarray(aero["CL"], dtype=float)
        cd = np.asarray(aero["CD"], dtype=float)

        # np.interp needs a non-decreasing x array; CL vs alpha is only
        # monotonic pre-stall, so sort rather than assume the sweep's given
        # order already is (handles a post-stall wiggle at the extremes).
        order = np.argsort(cl)
        cl_sorted = cl[order]
        if cl_target < cl_sorted[0] or cl_target > cl_sorted[-1]:
            return AirfoilCandidateResult(
                name=name,
                status="error",
                error=(
                    f"target CL {cl_target:.3f} outside this airfoil's swept range "
                    f"[{cl_sorted[0]:.3f}, {cl_sorted[-1]:.3f}] (alpha {alphas_deg[0]:.1f}..{alphas_deg[-1]:.1f} deg)"
                ),
            )
        cd_sorted = cd[order]
        alpha_sorted = alphas_deg[order]
        cd_at_target = float(np.interp(cl_target, cl_sorted, cd_sorted))
        alpha_at_target = float(np.interp(cl_target, cl_sorted, alpha_sorted))
        if not (cd_at_target > 0.0):
            return AirfoilCandidateResult(
                name=name, status="error", error="non-physical CD <= 0 at target CL"
            )

        max_t = float(airfoil.max_thickness())
        tank_vol = wing_fuel_volume_m3(wing, usable_fraction)
        tank_cap = tank_vol * config.mass_model.fuel_density_kg_m3
        l_over_d = cl_target / cd_at_target

        # NeuralFoil is a trained model, not a solver with hard convergence
        # guarantees -- a NaN/inf slipping through for a pathological
        # coordinate set is exactly the kind of "error" this sweep is meant
        # to catch, not silently forward as a candidate (JSON has no NaN/
        # Infinity literal either -- Python's json module would emit the
        # non-standard token anyway, breaking strict JSON parsers downstream).
        values = (
            l_over_d,
            cl_target,
            cd_at_target,
            alpha_at_target,
            max_t,
            tank_vol,
            tank_cap,
        )
        if not all(math.isfinite(v) for v in values):
            return AirfoilCandidateResult(
                name=name, status="error", error="non-finite result (NaN/inf)"
            )

        # Plausibility gate: the UIUC database (coord_seligFmt.zip) mixes in a
        # handful of entries that AREN'T standalone wing sections at all (e.g.
        # "30p-30n-slat"/"30p-30n-flap" -- individual ELEMENTS of a 3-element
        # high-lift system) alongside a few CST-fit edge cases. These don't
        # raise -- NeuralFoil still returns a number -- but `max_thickness()`
        # comes back ~0 for a thin high-lift-device sliver, and separately a
        # bad fit can produce a triple-digit-percent "thickness" with CD near
        # 1.0 (no attached 2-D flow at these Reynolds numbers is anywhere near
        # that draggy). Left unfiltered, either can rank as a top candidate on
        # score alone despite being an implausible result that exploits a
        # blind spot in the method rather than a genuinely good section.
        # The lower bound only needs to catch a *degenerate* (effectively
        # zero) thickness -- the real slat/flap entries measure exactly 0.0%.
        # It's deliberately NOT raised to "typical transport wing" territory
        # (~6-18%): the database legitimately includes plenty of thin
        # racing/glider/high-speed sections in the low single digits (e379 at
        # 2.2%, e376 at 2.5%, etc., all real single-element sections) that a
        # stricter floor would wrongly discard. The upper bound catches the
        # opposite failure (a bad CST fit inflating thickness into obviously-
        # unusable territory).
        # Hard sanity gate (always applied): catch degenerate/broken database
        # entries -- a thin high-lift-device sliver reads ~0% t/c, a bad CST
        # fit can invent a triple-digit "thickness". Independent of the user's
        # thickness window below, so tightening that window never mislabels a
        # legitimate thin section as "degenerate".
        if not (0.005 <= max_t <= 0.30):
            return AirfoilCandidateResult(
                name=name,
                status="error",
                error=(
                    f"implausible t/c={max_t * 100:.1f}% (outside 0.5-30% realistic range) -- likely a "
                    f"multi-element/degenerate database entry, not a usable wing section"
                ),
            )
        # User thickness window (structural spar depth / fuel volume / wave
        # drag). Defaults (0.5-25%) span the full realistic range so an
        # untouched sweep keeps every real section; tightening it is how a user
        # restricts to, say, a transport-like 10-16% band.
        if not (min_tc <= max_t <= max_tc):
            return AirfoilCandidateResult(
                name=name,
                status="error",
                error=(
                    f"t/c={max_t * 100:.1f}% is outside the requested thickness window "
                    f"[{min_tc * 100:.1f}%, {max_tc * 100:.1f}%]"
                ),
            )
        # CD ceiling deliberately wide (0.30 = 3000 drag counts): this gate only
        # exists to drop numerically-broken CST fits, NOT to pre-judge whether a
        # section suits this aircraft -- that's Stage 2's (3-D) job, and a real
        # section rejected here never reaches it. The database's thin low-
        # Reynolds sections land around CD ~0.03-0.09 here, while a genuine thick
        # transport/transonic section at a high cruise CL legitimately reaches
        # CD ~0.15 even after the sweep-Mach correction above (SC2-0714, rae2822,
        # naca23012 all measured ~0.14-0.16 at this design's condition). A tighter
        # ceiling risks silently excluding exactly those transport sections. A
        # known bad CST fit, fx79w660a, comes back at CD=1.11 -- still ~3.7x this
        # ceiling -- so the gap between "legitimately draggy for this use case"
        # and "numerically broken" stays wide enough that 0.30 catches the
        # latter without punishing the former.
        if not (0.0 < cd_at_target <= 0.30) or l_over_d > 150.0:
            return AirfoilCandidateResult(
                name=name,
                status="error",
                error=(
                    f"implausible 2-D result at target CL (CD={cd_at_target:.4f}, L/D={l_over_d:.1f}) -- "
                    f"likely a NeuralFoil CST-fit breakdown for this coordinate set, not a real polar"
                ),
            )

        # Off-design robustness (drag-bucket flatness): reuse the polar already
        # in hand to sample L/D a little either side of the design CL. np.interp
        # clamps at the swept-range ends, so clip the off-design CLs into range
        # first rather than mistaking a clamped endpoint for a real off-design
        # point. Nearly free -- no extra NeuralFoil call.
        cl_lo = max(float(cl_sorted[0]), cl_target - cl_band)
        cl_hi = min(float(cl_sorted[-1]), cl_target + cl_band)
        cd_lo = float(np.interp(cl_lo, cl_sorted, cd_sorted))
        cd_hi = float(np.interp(cl_hi, cl_sorted, cd_sorted))
        robustness: Optional[float] = None
        if cd_lo > 0.0 and cd_hi > 0.0:
            ld_off = 0.5 * (cl_lo / cd_lo + cl_hi / cd_hi)
            ratio = ld_off / l_over_d
            if math.isfinite(ratio):
                robustness = float(min(max(ratio, 0.0), 1.5))

        return AirfoilCandidateResult(
            name=name,
            status="ok",
            l_over_d=l_over_d,
            cl=cl_target,
            cd=cd_at_target,
            alpha_deg=alpha_at_target,
            max_thickness_frac=max_t,
            tank_volume_m3=tank_vol,
            tank_capacity_kg=tank_cap,
            robustness=robustness,
        )
    except Exception as exc:
        return AirfoilCandidateResult(name=name, status="error", error=str(exc)[:300])


# A candidate's real trimmed CL must land within this fraction of cl_target --
# otherwise the trim solve found an alpha/i_h that doesn't actually sustain
# level cruise flight for this airfoil (L != W in practice), so it's not a
# usable candidate regardless of how good its drag numbers look.
_CL_FEASIBILITY_TOL = 0.02
# A trim solve landing more than this far outside the probe window
# (AnalysisConfig.probe_alpha_low/high_deg) is extrapolating the closed-form
# linear trim theory into territory it was never validated for -- a strong
# sign the candidate's lift-curve behaviour is too different from the
# baseline's for this cheap trim solve to be trustworthy.
_TRIM_ALPHA_SLACK_DEG = 12.0


def _refine_candidate_3d(
    candidate: AirfoilCandidateResult,
    config: ALASConfig,
    dv: DesignVector,
    mach: float,
    altitude: float,
    cl_target: float,
    min_static_margin: Optional[float] = None,
) -> None:
    """Stage 2: re-simulate ``candidate`` as this design's ACTUAL 3-D wing and
    fill its ``*_3d`` fields in place.

    Rebuilds the aircraft with ``candidate.name`` as the root airfoil (an
    isolated config copy -- ``config`` is never mutated), then runs the SAME
    real evaluation :mod:`alas.analysis.full_analysis` uses for the app's
    trusted, reported numbers -- not a cheap linear proxy:

    1. A real (closed-form weight build-up) mass analysis, anchoring the
       aerodynamic moment reference to the actual physical CG -- static margin
       and trim are CG-sensitive, so evaluating about a geometry-default
       reference (the previous behaviour here) understates how much swapping
       the root airfoil can shift the trimmed condition.
    2. :func:`alas.physics.stability.stability_and_trim` -- the closed-
       form alpha + horizontal-stabilizer-incidence trim solve for
       ``CL=cl_target``, ``Cm=0``.
    3. :meth:`AeroAnalysis.trimmed_performance` -- ONE real, non-linear VLM
       point at the solved trim condition (not a 2-probe-point linear
       extrapolation), giving physically real CL/CD/L-over-D at that point.

    **Feasibility gate (L must actually equal Weight):** the closed-form trim
    solve is itself a linearization: for a candidate whose lift-curve slope or
    zero-lift angle differs a lot from the baseline design's, it can land on an
    alpha/CL that the real (non-linear) aircraft doesn't actually achieve.
    Demotes (leaves ``refined=False``) if either the REAL trimmed CL misses
    ``cl_target`` by more than :data:`_CL_FEASIBILITY_TOL`, or the solved trim
    alpha falls more than :data:`_TRIM_ALPHA_SLACK_DEG` outside the probed
    window -- i.e. this candidate's own physics couldn't sustain level cruise
    at the required CL, not merely "produced a worse L/D".

    Any failure demotes the candidate (``refined`` stays False, a short reason
    is recorded) rather than aborting the sweep -- same contract as
    :func:`_score_candidate`."""
    try:
        from ..physics.aerodynamics import AeroAnalysis
        from ..physics.mass import run_mass_analysis
        from ..physics.stability import stability_and_trim

        new_wing = dataclasses.replace(
            config.geometry.wing, root_airfoil=candidate.name
        )
        new_geometry = dataclasses.replace(config.geometry, wing=new_wing)
        cfg2 = dataclasses.replace(config, geometry=new_geometry)
        plane = AircraftBuilder(cfg2.geometry).build(dv, include_engines=False)

        # Anchor the aerodynamic moment reference to this candidate's own
        # physical CG -- same lumped-payload-first-pass approximation
        # full_analysis.py uses before its (optional, more detailed) payload
        # layout pass; precise enough for a screening tool, and cheap (a
        # closed-form weight build-up, not an iterative/VLM solve).
        _masses, _coords, cg = run_mass_analysis(
            plane, config.requirements, cfg2.geometry, cfg2.mass_model
        )
        plane.xyz_ref[0] = float(cg[0])

        aero = AeroAnalysis(
            plane,
            sweep_deg=dv.sweep_deg,
            geometry=cfg2.geometry,
            drag_model=cfg2.drag_model,
            analysis=cfg2.analysis,
        )
        trim = stability_and_trim(plane, cfg2.analysis, cl_target, mach, altitude)
        perf = aero.trimmed_performance(trim, mach, altitude)

        l_over_d_3d = float(perf["L/D"])
        cd_3d = float(perf["CD"])
        cl_3d = float(perf["CL"])
        alpha_3d = float(perf["alpha"])
        cm_residual = float(perf["Cm_residual"])
        if (
            not all(
                math.isfinite(v)
                for v in (l_over_d_3d, cd_3d, cl_3d, alpha_3d, cm_residual)
            )
            or cd_3d <= 0.0
        ):
            candidate.refine_error = "non-physical 3-D result"
            return

        if abs(cl_3d - cl_target) > _CL_FEASIBILITY_TOL * max(cl_target, 1e-6):
            candidate.refine_error = (
                f"trim solve could not sustain level cruise: trimmed CL={cl_3d:.3f}, "
                f"required CL={cl_target:.3f} (L != W for this airfoil on this design)"
            )
            return
        a_lo, a_hi = (
            cfg2.analysis.probe_alpha_low_deg,
            cfg2.analysis.probe_alpha_high_deg,
        )
        window_lo, window_hi = (
            min(a_lo, a_hi) - _TRIM_ALPHA_SLACK_DEG,
            max(a_lo, a_hi) + _TRIM_ALPHA_SLACK_DEG,
        )
        if not (window_lo <= trim.trim_alpha_deg <= window_hi):
            candidate.refine_error = (
                f"trim alpha {trim.trim_alpha_deg:.1f} deg is unrealistically far from the probe "
                f"window [{a_lo:.1f}, {a_hi:.1f}] deg -- this airfoil's lift behaviour is too different "
                f"from the baseline for the closed-form trim solve to be trustworthy"
            )
            return

        # Static-margin floor (optional): a great-L/D section that pushes the
        # trimmed static margin below the user's floor would leave the aircraft
        # too weakly (or negatively) stable in pitch once swapped in -- demote
        # it rather than let it top the ranking on drag alone. Off by default
        # (None), so an untouched sweep only *reports* static margin as before.
        if (
            min_static_margin is not None
            and math.isfinite(trim.static_margin)
            and trim.static_margin < min_static_margin
        ):
            candidate.static_margin_3d = float(trim.static_margin)
            candidate.refine_error = (
                f"static margin {trim.static_margin * 100:.1f}% is below the requested floor "
                f"{min_static_margin * 100:.1f}% -- this airfoil swap would leave the aircraft too "
                f"weakly stable in pitch"
            )
            return

        candidate.l_over_d_3d = l_over_d_3d
        candidate.cd_3d = cd_3d
        candidate.alpha_3d_deg = alpha_3d
        candidate.cm_residual_3d = cm_residual
        candidate.static_margin_3d = (
            float(trim.static_margin) if math.isfinite(trim.static_margin) else None
        )
        candidate.refined = True
    except Exception as exc:
        candidate.refine_error = str(exc)[:200]


def _verify_candidate_mses(
    candidate: AirfoilCandidateResult,
    config: ALASConfig,
    dv: DesignVector,
    mach: float,
    altitude: float,
    cl_target: float,
    should_cancel: Optional[Callable[[], bool]] = None,
) -> None:
    """Stage 3: verify a Stage-2 survivor with a real MSES coupled viscous/
    inviscid Euler + boundary-layer solve -- the accurate, shock-capturing
    check neither the 2-D NeuralFoil proxy nor the 3-D VLM+Korn model can do,
    which is exactly what separates "looks good on paper" from "actually
    handles this cruise Mach well" for a transonic design.

    Mirrors ``pipeline.py::_run_mses_analysis`` for apples-to-apples numbers
    app-wide: same sweep-theory effective section Mach (a swept wing's SECTION
    sees a lower Mach than freestream, ``M*cos(sweep)``), same chord-based
    Reynolds, same :func:`run_mses_polar` driver (already resolves the MSES
    executables via the frozen-build-aware ``alas.paths`` resolver).
    Brackets the sweep around this candidate's own **Stage-1 2-D alpha**
    (``candidate.alpha_deg``, the NeuralFoil-computed alpha at ``cl_target`` for
    THIS exact section at the same section-effective Mach) rather than the
    Stage-2 3-D trimmed alpha: the two are different physical quantities (the
    3-D value bakes in this wing's induced/downwash effects), and centering on
    the 3-D one was observed to miss the real MSES-converged CL range entirely
    for some candidates. If the first sweep doesn't bracket ``cl_target``
    (solver disagreement between NeuralFoil and MSES on the exact operating
    alpha for an unusual section), retries once with a doubled half-width
    before giving up -- a bounded safety net, not an unbounded search.

    Never raises and never aborts the sweep: a missing MSES install, a non-
    convergent geometry (MSES failing to converge at all for a given section/
    Mach/Re is an expected, real solver outcome, not a bug -- the same
    tolerance ``pipeline.py::_run_mses_analysis`` already has for the main
    Run), or any setup exception all leave ``candidate.mses_verified=False``
    with a short ``mses_status``/``mses_error`` -- the candidate simply keeps
    its Stage-2 standing, same degrade-gracefully contract as every other
    stage in this module.
    """
    try:
        import dataclasses as _dc
        import math as _math

        from ..geometry.airfoils import build_section
        from ..physics.mses_analysis import (
            run_mses_polar,
            run_mses_pressure_distribution,
        )
        from ..paths import app_root

        if not config.mses.enabled:
            candidate.mses_status = "error"
            candidate.mses_error = "MSES is disabled (Setup > External Tools)"
            return

        airfoil = AirfoilLibrary.get(candidate.name)
        section = build_section(dv, airfoil.coordinates)

        m_effective = float(mach * _math.cos(_math.radians(dv.sweep_deg)))
        atmo = asb.Atmosphere(altitude=altitude)
        v = mach * atmo.speed_of_sound()
        reynolds = float(
            atmo.density() * v * dv.root_chord_m / atmo.dynamic_viscosity()
        )
        bracket_alpha = (
            candidate.alpha_deg
            if candidate.alpha_deg is not None
            else (candidate.alpha_3d_deg or 0.0)
        )

        polar = None
        for widen in (1.0, 2.0):
            # Each MSES solve is a 10-30s subprocess, and the widened retry
            # doubles that -- so honour a cancel between attempts instead of
            # making the user wait out a candidate they've already abandoned.
            if should_cancel is not None and should_cancel():
                candidate.mses_status = "cancelled"
                return
            mses_config = (
                config.mses
                if widen == 1.0
                else _dc.replace(
                    config.mses,
                    alpha_sweep_halfwidth_deg=config.mses.alpha_sweep_halfwidth_deg
                    * widen,
                )
            )
            attempt = run_mses_polar(
                section,
                m_effective,
                reynolds,
                trim_alpha_deg=bracket_alpha,
                mses_config=mses_config,
                repo_root=app_root(),
            )
            if attempt.status != "ok":
                polar = attempt
                continue
            cl_check = np.asarray(attempt.CL, dtype=float)
            if cl_check.min() <= cl_target <= cl_check.max():
                polar = attempt
                break
            polar = attempt  # keep the widest attempt's error context if both miss

        candidate.mses_status = polar.status
        if polar.status != "ok":
            candidate.mses_error = polar.error
            return

        cl_arr = np.asarray(polar.CL, dtype=float)
        cd_arr = np.asarray(polar.CD, dtype=float)
        cdw_arr = np.asarray(polar.CDw, dtype=float)
        order = np.argsort(cl_arr)
        cl_sorted = cl_arr[order]
        if cl_target < cl_sorted[0] or cl_target > cl_sorted[-1]:
            candidate.mses_status = "error"
            candidate.mses_error = (
                f"target CL {cl_target:.3f} outside MSES's converged range "
                f"[{cl_sorted[0]:.3f}, {cl_sorted[-1]:.3f}] (tried a widened sweep too)"
            )
            return
        cd_at_target = float(np.interp(cl_target, cl_sorted, cd_arr[order]))
        cdw_at_target = float(np.interp(cl_target, cl_sorted, cdw_arr[order]))
        if (
            not (cd_at_target > 0.0)
            or not _math.isfinite(cd_at_target)
            or not _math.isfinite(cdw_at_target)
        ):
            candidate.mses_status = "error"
            candidate.mses_error = "non-physical MSES result at target CL"
            return

        candidate.l_over_d_mses = cl_target / cd_at_target
        candidate.cd_mses = cd_at_target
        candidate.cdw_mses = cdw_at_target
        candidate.mses_verified = True

        # Per-candidate surface Cp/Mach distribution + 2-D flowfield -- the
        # same real shock/pressure-distribution data the main Run's Model
        # Comparison tab shows for the optimized design's own root section
        # (see pipeline.py::_run_mses_analysis), computed here for THIS
        # screened candidate so its own dedicated figures (Cp vs x/c upper/
        # lower, Mach contours) are available, not just the scalar L/D/CDw
        # above. Non-fatal: a failure here leaves mses_pressure at its
        # default "not_run" status (figures.py's status=="ok" gate already
        # renders that as "no data" rather than a blank plot -- see the
        # figures.py fix), the scalar MSES verification above already
        # succeeded regardless.
        if should_cancel is not None and should_cancel():
            return  # scalar verification above already succeeded; skip the extra solve
        alpha_at_target = float(
            np.interp(
                cl_target, cl_sorted, np.asarray(polar.alpha_deg, dtype=float)[order]
            )
        )
        candidate.mses_pressure = run_mses_pressure_distribution(
            section,
            m_effective,
            reynolds,
            alpha_deg=alpha_at_target,
            mses_config=config.mses,
            repo_root=app_root(),
        )
    except Exception as exc:
        candidate.mses_status = "error"
        candidate.mses_error = str(exc)[:200]


def run_airfoil_screening(
    config: ALASConfig,
    dv: Optional[DesignVector] = None,
    *,
    ld_weight: float = 0.7,
    fuel_weight: float = 0.3,
    robustness_weight: float = 0.0,
    cl_band: float = 0.05,
    top_n: int = 50,
    model_size: str = "large",
    alpha_min_deg: float = -4.0,
    alpha_max_deg: float = 14.0,
    alpha_step_deg: float = 0.5,
    min_tc: float = 0.005,
    max_tc: float = 0.25,
    name_filter: str = "",
    refine_3d: bool = True,
    refine_top_n: int = 20,
    min_static_margin: Optional[float] = None,
    verify_mses: bool = True,
    mses_top_n: int = 5,
    progress_callback: Optional[Callable[[str], None]] = None,
    should_cancel: Optional[Callable[[], bool]] = None,
) -> AirfoilScreeningResult:
    """Screen every airfoil in :class:`AirfoilLibrary` against ``config``'s
    cruise condition and rank survivors by a weighted L/D + fuel-tank-
    capacity score (``ld_weight``/``fuel_weight``, both against min-max
    normalized values across the survivors so the two very different-scaled
    quantities combine sensibly).

    Three stages:

    * **Stage 1** -- the fast 2-D NeuralFoil proxy scores every database entry
      and ranks the survivors. This alone is prone to flattering low-Reynolds
      sections that win an isolated 2-D polar but wouldn't on this aircraft.
    * **Stage 2** (``refine_3d``) -- the top ``refine_top_n`` survivors are
      rebuilt into this design's real wing and re-evaluated with a real trim
      solve (VLM induced drag + Raymer parasite + Korn wave, closed-form trim
      + one non-linear VLM point -- see :func:`_refine_candidate_3d`), then
      re-ranked by a 3-D score. A candidate whose trim solve can't actually
      sustain ``CL=cl_target`` (L != W) is demoted, not just down-ranked. This
      is what actually reflects "how does this airfoil perform on MY wing at MY
      cruise condition", using the live MTOW/span/root-chord geometry.
    * **Stage 3** (``verify_mses``) -- the top ``mses_top_n`` Stage-2 survivors
      get a real MSES coupled viscous/inviscid solve (shock capture, true wave
      drag) at the sweep-corrected section Mach -- see
      :func:`_verify_candidate_mses`. This is the only stage that can actually
      reward a genuinely supercritical section's transonic advantage, which
      neither NeuralFoil nor VLM+Korn model. Silently skipped if MSES isn't
      configured (candidates simply keep their Stage-2 standing).

    Physics/scoping controls (all default to the previous behaviour, so an
    untouched sweep is unchanged):

    * ``robustness_weight`` / ``cl_band`` -- optionally reward a flat drag bucket
      (L/D held across ``cl_target +/- cl_band``), so a section that only shines
      at exactly the design CL loses ground to one robust to real CL variation.
    * ``min_tc`` / ``max_tc`` -- a thickness window (structural spar depth / fuel
      volume / wave drag); sections outside it are excluded in Stage 1.
    * ``min_static_margin`` -- a stability floor; a Stage-2 candidate whose real
      trim solve falls below it is demoted rather than allowed to rank on drag
      alone.
    * ``name_filter`` -- restrict the swept database to matching names/families
      (comma-separated substrings or globs, e.g. ``"sc2, naca23"``), which both
      focuses the comparison and speeds the run.
    * ``should_cancel`` -- a cheap predicate polled between candidates/stages; on
      True the sweep stops early and returns the partial ranking so far with
      ``AirfoilScreeningResult.cancelled = True``.

    Never raises for a bad individual airfoil -- see :func:`_score_candidate` /
    :func:`_refine_candidate_3d` / :func:`_verify_candidate_mses`. Does raise
    for a genuinely broken ``config`` (e.g. it can't build ANY wing at all),
    same as every other analysis entry point in this codebase; the caller (the
    sidecar route) catches that.
    """
    dv = dv or DesignVector.default()
    mach, reynolds, cl_target, altitude = _cruise_condition(config, dv)
    # Sweep theory: a swept wing's SECTION sees a lower effective Mach than the
    # freestream (M_section = M_inf * cos(sweep)). The 2-D NeuralFoil proxy is
    # an unswept-section model, so feeding it the raw freestream Mach at a
    # transonic cruise applies a far more severe compressibility/drag-rise
    # condition than the real swept section experiences -- which was rejecting
    # every legitimate thick transport section on its (correctly) high 2-D drag
    # and leaving only thin low-Reynolds shapes. This mirrors the identical
    # correction mses_analysis._run_mses_analysis already applies (see its
    # comment). Reynolds and target CL stay on the
    # freestream condition; only the section's compressibility Mach is reduced.
    section_mach = float(mach * math.cos(math.radians(dv.sweep_deg)))
    alphas_deg = np.arange(alpha_min_deg, alpha_max_deg + 1e-9, alpha_step_deg)
    usable_fraction = config.mass_model.fuel_tank_usable_fraction

    names = AirfoilLibrary.get_available_airfoils()
    if name_filter.strip():
        names = _filter_names(names, name_filter)
    n_total = len(names)
    results: List[AirfoilCandidateResult] = []
    errors: List[Dict[str, str]] = []
    cancelled = False

    for i, name in enumerate(names):
        if should_cancel is not None and should_cancel():
            cancelled = True
            break
        r = _score_candidate(
            name,
            config,
            dv,
            section_mach,
            reynolds,
            cl_target,
            usable_fraction,
            alphas_deg,
            model_size,
            min_tc=min_tc,
            max_tc=max_tc,
            cl_band=cl_band,
        )
        if r.status == "ok":
            results.append(r)
        else:
            errors.append({"name": name, "error": r.error or ""})
        if progress_callback is not None and (i % 25 == 0 or i == n_total - 1):
            progress_callback(
                f"Stage 1 (2-D): {i + 1}/{n_total} screened -- {len(results)} ok, {len(errors)} errors"
            )

    if results:
        _blend_scores(
            results,
            ld_weight,
            fuel_weight,
            robustness_weight,
            key=lambda r: r.l_over_d,
            attr="score",
        )
        results.sort(key=lambda r: r.score, reverse=True)

    reference_set = set(REFERENCE_AIRFOILS)
    for r in results:
        if r.name in reference_set:
            r.is_reference = True

    # Stage 2: re-simulate the shortlist as this design's actual 3-D wing.
    n_refined = 0
    if refine_3d and results and refine_top_n > 0 and not cancelled:
        # Real, wind-tunnel-validated sections (REFERENCE_AIRFOILS) are FORCED
        # into the 3-D shortlist regardless of their Stage-1 rank -- the whole
        # point is a physical "known good" anchor to compare the algorithmic
        # picks against, which only works if they always get evaluated rather
        # than betting on the cheap 2-D proxy score (which is exactly what can
        # be gamed by a low-Reynolds section unsuited to this aircraft) to
        # decide whether they're even worth a look. A reference airfoil that
        # already made the top refine_top_n on its own merits isn't added
        # twice.
        top_by_score = results[:refine_top_n]
        forced_references = [r for r in results[refine_top_n:] if r.is_reference]
        shortlist = top_by_score + forced_references
        for i, cand in enumerate(shortlist):
            if should_cancel is not None and should_cancel():
                cancelled = True
                break
            _refine_candidate_3d(
                cand,
                config,
                dv,
                mach,
                altitude,
                cl_target,
                min_static_margin=min_static_margin,
            )
            if cand.refined:
                n_refined += 1
            if progress_callback is not None:
                progress_callback(
                    f"Stage 2 (3-D wing): {i + 1}/{len(shortlist)} re-simulated -- {n_refined} ok"
                )

        refined = [r for r in shortlist if r.refined]
        if refined:
            _blend_scores(
                refined,
                ld_weight,
                fuel_weight,
                robustness_weight,
                key=lambda r: r.l_over_d_3d,
                attr="score_3d",
            )
        # Final ordering: 3-D-refined candidates first (by their 3-D score,
        # best-on-this-wing), then everything else by the 2-D score. A refine
        # failure keeps its 2-D standing rather than being dropped.
        results.sort(
            key=lambda r: (
                r.refined,
                r.score_3d if r.refined else -1.0,
                r.score or 0.0,
            ),
            reverse=True,
        )

    # Stage 3: MSES-verify the final few Stage-2 survivors (real shock/wave
    # drag -- see _verify_candidate_mses's docstring for why this is the only
    # stage that can properly credit a genuinely supercritical section).
    n_mses_verified = 0
    if verify_mses and results and mses_top_n > 0 and not cancelled:
        refined_results = [r for r in results if r.refined]
        # Same forcing rationale as Stage 2: a real reference section that
        # survived Stage 2 always gets MSES-verified too, regardless of its
        # 3-D rank, so the user always has a real-shock/real-viscous
        # comparison point against a section known to actually work.
        top_by_3d = refined_results[:mses_top_n]
        forced_references = [r for r in refined_results[mses_top_n:] if r.is_reference]
        mses_shortlist = top_by_3d + forced_references
        for i, cand in enumerate(mses_shortlist):
            if should_cancel is not None and should_cancel():
                cancelled = True
                break
            _verify_candidate_mses(
                cand, config, dv, mach, altitude, cl_target, should_cancel=should_cancel
            )
            if cand.mses_verified:
                n_mses_verified += 1
            if progress_callback is not None:
                progress_callback(
                    f"Stage 3 (MSES): {i + 1}/{len(mses_shortlist)} verified -- {n_mses_verified} ok"
                )

        verified = [r for r in mses_shortlist if r.mses_verified]
        if verified:
            # MSES L/D is the most physically trustworthy number available
            # (real viscous + wave drag) -- re-rank just this small verified
            # set above the rest, which keep their Stage-2 standing.
            verified.sort(key=lambda r: r.l_over_d_mses, reverse=True)
            verified_names = {r.name for r in verified}
            rest = [r for r in results if r.name not in verified_names]
            results = verified + rest

    return AirfoilScreeningResult(
        baseline_airfoil=config.geometry.wing.root_airfoil,
        cruise_mach=mach,
        cruise_reynolds=reynolds,
        cruise_altitude_m=altitude,
        cl_target=cl_target,
        transonic_caveat=mach >= TRANSONIC_MACH_CAVEAT,
        refined_3d=refine_3d and n_refined > 0,
        n_total=n_total,
        n_ok=len(results),
        n_error=len(errors),
        n_refined=n_refined,
        n_mses_verified=n_mses_verified,
        cancelled=cancelled,
        candidates=results[: max(0, top_n)],
        errors=errors[:100],
    )


def _filter_names(names: List[str], pattern: str) -> List[str]:
    """Restrict ``names`` to those matching ``pattern`` (case-insensitive).

    ``pattern`` is a comma-separated list of tokens; a token containing a glob
    metacharacter (``*`` ``?`` ``[``) is matched with :func:`fnmatch.fnmatch`,
    otherwise it's a plain substring match. A name matching ANY token is kept
    (union), original order preserved; empty/whitespace tokens are ignored. So
    ``"sc2, naca23"`` keeps every supercritical SC(2) section and the NACA 230xx
    family, and ``"e*"`` keeps everything starting with 'e'."""
    tokens = [t.strip().lower() for t in pattern.split(",") if t.strip()]
    if not tokens:
        return names
    out: List[str] = []
    for name in names:
        low = name.lower()
        for tok in tokens:
            hit = (
                fnmatch.fnmatch(low, tok)
                if any(c in tok for c in "*?[")
                else (tok in low)
            )
            if hit:
                out.append(name)
                break
    return out


def _blend_scores(
    candidates: List[AirfoilCandidateResult],
    ld_weight: float,
    fuel_weight: float,
    robustness_weight: float = 0.0,
    *,
    key: Callable[[AirfoilCandidateResult], float],
    attr: str,
) -> None:
    """Assign each candidate a min-max-normalized weighted blend to ``attr``:
    ``ld_weight*L/D + fuel_weight*fuel_capacity``, plus, when
    ``robustness_weight > 0``, ``robustness_weight*drag-bucket-flatness``
    (``candidate.robustness``). The terms live on very different scales, so each
    is min-max normalized across ``candidates`` first. With
    ``robustness_weight == 0`` this reduces exactly to the earlier two-term
    blend. Used for both the Stage-1 (2-D ``l_over_d``) and Stage-2 (3-D
    ``l_over_d_3d``) ranks."""

    def _normed(values: List[float]) -> np.ndarray:
        arr = np.array(values, dtype=float)
        lo = float(np.nanmin(arr))
        span = float(np.nanmax(arr) - lo) or 1.0
        return (arr - lo) / span

    norm_ld = _normed([key(r) for r in candidates])
    norm_fuel = _normed([r.tank_capacity_kg for r in candidates])
    use_robust = robustness_weight > 0.0 and any(
        r.robustness is not None for r in candidates
    )
    norm_robust = (
        _normed([r.robustness if r.robustness is not None else 0.0 for r in candidates])
        if use_robust
        else np.zeros(len(candidates))
    )
    for i, r in enumerate(candidates):
        score = ld_weight * float(norm_ld[i]) + fuel_weight * float(norm_fuel[i])
        if use_robust:
            score += robustness_weight * float(norm_robust[i])
        setattr(r, attr, score)
