# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Longitudinal stability and balance.

Provides the autobalance routine: it measures the aircraft's static margin from a
two-point VLM Cm-vs-CL slope and shifts the CG reference (``xyz_ref``) so the
trimmed static margin matches a target. This replaces the duplicated
``autobalance_aircraft`` / ``autobalance`` functions in the reference scripts.
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Optional, Tuple

import aerosandbox as asb
import numpy as np

from ..config.analysis_config import AnalysisConfig


def static_margin(
    airplane: asb.Airplane, analysis: AnalysisConfig | None = None
) -> float:
    """Estimate static margin SM = -dCm/dCL from two VLM operating points."""
    analysis = analysis or AnalysisConfig()
    v = analysis.autobalance_velocity_m_s
    op_lo = asb.OperatingPoint(velocity=v, alpha=analysis.autobalance_alpha_low_deg)
    op_hi = asb.OperatingPoint(velocity=v, alpha=analysis.autobalance_alpha_high_deg)

    def vlm(op):
        return asb.VortexLatticeMethod(
            airplane=airplane,
            op_point=op,
            spanwise_resolution=analysis.spanwise_resolution,
            chordwise_resolution=analysis.chordwise_resolution,
            verbose=False,
        ).run()

    r_lo, r_hi = vlm(op_lo), vlm(op_hi)
    d_cm = r_hi["Cm"] - r_lo["Cm"]
    d_cl = r_hi["CL"] - r_lo["CL"]
    if abs(d_cl) < 1e-9:
        return float("nan")
    return float(-d_cm / d_cl)


def autobalance(
    airplane: asb.Airplane,
    target_static_margin: float = 0.10,
    analysis: AnalysisConfig | None = None,
) -> Tuple[asb.Airplane, float]:
    """Shift the CG so the aircraft's static margin equals the target.

    Modifies ``airplane.xyz_ref[0]`` in place and returns ``(airplane, sm_before)``,
    where ``sm_before`` is the static margin measured *before* the correction.
    This byproduct is free -- it comes from the VLM calls already made inside
    this function -- so callers can use it for penalty terms without extra cost.

    A positive shift moves the CG aft (reducing SM); the relation
    ``dx = (SM_current - SM_target) * c_ref`` follows from SM being measured in
    fractions of the mean aerodynamic chord.
    """
    sm_current = static_margin(airplane, analysis)
    if sm_current != sm_current:  # NaN guard (degenerate dCL)
        return airplane, float("nan")
    shift = (sm_current - target_static_margin) * airplane.c_ref
    airplane.xyz_ref[0] += shift
    return airplane, sm_current


# ---------------------------------------------------------------------------
# Neutral point -- physically-anchored stability (CG = mass CG)
# ---------------------------------------------------------------------------
def _xsec_width(xs) -> float:
    """Cross-section width, from an explicit ``width`` or a circular ``radius``.

    Also imported by :mod:`alas.physics.payload` (``CabinGeometry``),
    which needs the same fuselage cross-section sampling -- kept here as the
    single definition rather than duplicated.
    """
    w = getattr(xs, "width", None)
    if w is not None:
        return float(w)
    r = getattr(xs, "radius", None)
    return float(2.0 * r) if r is not None else 0.0


def _xsec_height(xs) -> float:
    h = getattr(xs, "height", None)
    if h is not None:
        return float(h)
    return _xsec_width(xs)


def munk_apparent_mass_factor(fineness: float) -> float:
    """Munk (k2 - k1) apparent-mass factor vs. fuselage fineness ratio L/d.

    Standard correlation (Munk; tabulated in Roskam / USAF DATCOM). Approaches 1
    for very slender bodies; lower for stubby ones.
    """
    f = [4.0, 6.0, 8.0, 10.0, 12.0, 16.0, 20.0]
    k = [0.77, 0.86, 0.91, 0.94, 0.955, 0.97, 0.98]
    return float(np.interp(max(4.0, min(20.0, fineness)), f, k))


def fuselage_cm_alpha(airplane: asb.Airplane, cl_alpha: float) -> float:
    """Fuselage pitching-moment slope dCm/dα [per rad] -- Munk/Multhopp method.

    Geometry-driven: it integrates the *actual* fuselage cross-section area
    distribution A(x) = (π/4)·w(x)·h(x) from the built fuselage stations, so it
    generalises to any aircraft with no per-aircraft tuning. Slender-body
    (Munk) result ``Cm_α = (k2−k1)·2·∫A·η_local dx / (S·c̄)`` with a local-flow
    factor ``η_local`` that reduces the destabilising contribution of the
    afterbody by the wing downwash. Positive = destabilising (moves NP forward).
    """
    fus = airplane.fuselages[0]
    xs = np.array([float(s.xyz_c[0]) for s in fus.xsecs])
    order = np.argsort(xs)
    xs = xs[order]
    ws = np.array([_xsec_width(s) for s in fus.xsecs])[order]
    hs = np.array([_xsec_height(s) for s in fus.xsecs])[order]
    if len(xs) < 2:
        return 0.0

    wing = next((w for w in airplane.wings if w.name == "Main Wing"), airplane.wings[0])
    x_le = float(wing.xsecs[0].xyz_le[0])
    x_te = x_le + float(wing.xsecs[0].chord)
    hstab = next((w for w in airplane.wings if w.name == "Horizontal Stabilizer"), None)
    x_h = (
        float(hstab.aerodynamic_center()[0])
        if hstab is not None
        else x_te + 3.0 * float(airplane.c_ref)
    )

    s_ref = max(1.0, float(airplane.s_ref))
    c_ref = max(0.1, float(airplane.c_ref))
    ar = max(1.0, float(wing.aspect_ratio()))
    d_eps_d_alpha = (
        2.0 * max(0.1, cl_alpha) / (math.pi * ar)
    )  # downwash gradient at tail

    l_f = float(xs[-1] - xs[0])
    d_f = max(0.1, float(ws.max()))
    k_fac = munk_apparent_mass_factor(l_f / d_f)

    accum = 0.0
    for i in range(len(xs) - 1):
        dx = float(xs[i + 1] - xs[i])
        if dx <= 0:
            continue
        x_mid = 0.5 * float(xs[i] + xs[i + 1])
        w_mid = 0.5 * float(ws[i] + ws[i + 1])
        h_mid = 0.5 * float(hs[i] + hs[i + 1])
        area = math.pi / 4.0 * w_mid * h_mid
        if x_mid <= x_te:
            eta_local = 1.0  # fore-body / over-wing: ~free-stream α
        else:
            frac = min(1.0, (x_mid - x_te) / max(x_h - x_te, 0.1))
            eta_local = 1.0 - d_eps_d_alpha * frac  # after-body: reduced by downwash
        accum += area * eta_local * dx

    return k_fac * 2.0 * accum / (s_ref * c_ref)


def neutral_point(
    airplane: asb.Airplane,
    analysis: AnalysisConfig | None = None,
) -> Tuple[float, float, float]:
    """Physically-anchored neutral point, static margin, and lift-curve slope.

    Computes the wing+tail neutral point from a two-point VLM ``Cm``-``CL`` slope
    (about the current ``xyz_ref``, which callers set to the *physical* CG),
    applies a tail dynamic-pressure efficiency ``η_t`` to the tail's stabilising
    contribution, then shifts the NP forward by the geometry-driven fuselage
    (Munk/Multhopp) contribution. Returns ``(x_np, static_margin, CL_alpha)`` with
    ``static_margin = (x_np - x_cg)/c̄`` measured against the real CG.
    """
    analysis = analysis or AnalysisConfig()
    v = analysis.autobalance_velocity_m_s
    op_lo = asb.OperatingPoint(velocity=v, alpha=analysis.autobalance_alpha_low_deg)
    op_hi = asb.OperatingPoint(velocity=v, alpha=analysis.autobalance_alpha_high_deg)

    def vlm(op):
        return asb.VortexLatticeMethod(
            airplane=airplane,
            op_point=op,
            spanwise_resolution=analysis.spanwise_resolution,
            chordwise_resolution=analysis.chordwise_resolution,
            verbose=False,
        ).run()

    r_lo, r_hi = vlm(op_lo), vlm(op_hi)
    d_cl = float(r_hi["CL"] - r_lo["CL"])
    d_cm = float(r_hi["Cm"] - r_lo["Cm"])
    d_alpha = math.radians(
        analysis.autobalance_alpha_high_deg - analysis.autobalance_alpha_low_deg
    )
    c_ref = max(0.1, float(airplane.c_ref))
    x_cg = float(airplane.xyz_ref[0])

    if abs(d_cl) < 1e-9 or abs(d_alpha) < 1e-9:
        return x_cg, float("nan"), float("nan")

    cl_alpha = d_cl / d_alpha
    sm_vlm = -d_cm / d_cl
    x_np_vlm = x_cg + sm_vlm * c_ref

    # Tail dynamic-pressure efficiency: scale the tail's (NP-aft) contribution.
    # Wing-alone NP ≈ wing aerodynamic centre; the rest is the tail's doing.
    wing = next((w for w in airplane.wings if w.name == "Main Wing"), airplane.wings[0])
    x_wing_ac = float(wing.aerodynamic_center()[0])
    eta_t = analysis.tail_efficiency
    x_np = x_wing_ac + eta_t * (x_np_vlm - x_wing_ac)

    # Fuselage destabilising contribution (moves NP forward) -- geometry-driven.
    if analysis.include_fuselage_stability and cl_alpha > 0.1:
        cm_a_fus = fuselage_cm_alpha(airplane, cl_alpha)
        x_np -= cm_a_fus * c_ref / cl_alpha

    sm = (x_np - x_cg) / c_ref
    return x_np, sm, cl_alpha


# ---------------------------------------------------------------------------
# Longitudinal trim solve -- cruise-condition 3-point VLM probe
# ---------------------------------------------------------------------------
@dataclass
class StabilityTrimResult:
    """Cruise-condition static margin + closed-form longitudinal trim solve.

    See methods.md Sec 9f for the full derivation. ``trim_ih_deg``/
    ``cl_ih``/``cm_ih`` are ``nan``/``0.0`` if the airplane has no horizontal
    stabilizer (pure-alpha trim fallback).
    """

    x_np: float
    static_margin: float
    cl_alpha: float
    trim_alpha_deg: float
    trim_ih_deg: float
    cl_ih: float
    cm_ih: float


def stability_and_trim(
    airplane: asb.Airplane,
    analysis: AnalysisConfig,
    cl_target: float,
    mach: float,
    altitude_m: float,
) -> StabilityTrimResult:
    """Cruise-condition 3-point VLM probe: static margin + closed-form trim.

    Unlike :func:`neutral_point` (which probes at a fixed low-speed reference
    condition, ``analysis.autobalance_velocity_m_s``), this probes at the
    actual cruise Mach/altitude so the same three VLM solves serve both the
    static-margin measurement AND a genuine trimmed-condition solve -- alpha
    and horizontal-stabilizer incidence jointly satisfying ``CL = cl_target``
    and ``Cm = 0`` (see methods.md Sec 9f). Used by the optimizer's
    inner loop (``optimization.objective``) and the final full analysis
    (``analysis.full_analysis``); NOT used by :func:`neutral_point` itself,
    whose callers (the deliberately cheap Stage-0 baseline pass) keep the old
    fixed-condition probe unchanged.

    Three VLM evaluations, all at (mach, altitude_m):
      1. (alpha_lo, i_h0)                                            -- baseline
      2. (alpha_hi, i_h0)                                            -- alpha probe
      3. (alpha_lo, i_h0 + analysis.trim_incidence_probe_delta_deg)  -- incidence probe
    where alpha_lo/alpha_hi are ``analysis.probe_alpha_low/high_deg`` and
    ``i_h0`` is the current root-station twist of the "Horizontal Stabilizer"
    wing, perturbed rigidly (every cross-section by the same delta, modelling
    a trimmable stabilizer) and restored afterward -- ``WingXSec.twist`` is a
    plain mutable float, perturbed in place exactly like :func:`autobalance`
    already mutates ``airplane.xyz_ref[0]``.

    Static margin uses the same formula as :func:`neutral_point` (tail
    efficiency + fuselage Munk/Multhopp correction), from probes 1 & 2.

    If the airplane has no horizontal stabilizer, degrades to a pure-alpha
    trim (solving only ``CL(alpha) = cl_target`` from probes 1 & 2) -- never
    raises for that case.
    """
    atmo = asb.Atmosphere(altitude=altitude_m)
    v = mach * atmo.speed_of_sound()
    a_lo = analysis.probe_alpha_low_deg
    a_hi = analysis.probe_alpha_high_deg
    delta_ih = analysis.trim_incidence_probe_delta_deg

    hstab = next((w for w in airplane.wings if w.name == "Horizontal Stabilizer"), None)
    has_ih = hstab is not None and len(hstab.xsecs) > 0

    def vlm(alpha):
        return asb.VortexLatticeMethod(
            airplane=airplane,
            op_point=asb.OperatingPoint(atmosphere=atmo, velocity=v, alpha=alpha),
            spanwise_resolution=analysis.spanwise_resolution,
            chordwise_resolution=analysis.chordwise_resolution,
            verbose=False,
        ).run()

    r1 = vlm(a_lo)
    r2 = vlm(a_hi)

    d_alpha = a_hi - a_lo
    cl_alpha = (
        float(r2["CL"] - r1["CL"]) / d_alpha if abs(d_alpha) > 1e-9 else float("nan")
    )
    cm_alpha = (
        float(r2["Cm"] - r1["Cm"]) / d_alpha if abs(d_alpha) > 1e-9 else float("nan")
    )

    x_cg = float(airplane.xyz_ref[0])
    c_ref = max(0.1, float(airplane.c_ref))

    if cl_alpha != cl_alpha or abs(cl_alpha) < 1e-9:  # NaN or degenerate guard
        x_np, sm = x_cg, float("nan")
    else:
        sm_vlm = -cm_alpha / cl_alpha
        x_np_vlm = x_cg + sm_vlm * c_ref
        wing = next(
            (w for w in airplane.wings if w.name == "Main Wing"), airplane.wings[0]
        )
        x_wing_ac = float(wing.aerodynamic_center()[0])
        eta_t = analysis.tail_efficiency
        x_np = x_wing_ac + eta_t * (x_np_vlm - x_wing_ac)

        cl_alpha_per_rad = cl_alpha / math.radians(1.0)
        if analysis.include_fuselage_stability and cl_alpha_per_rad > 0.1:
            cm_a_fus = fuselage_cm_alpha(airplane, cl_alpha_per_rad)
            x_np -= cm_a_fus * c_ref / cl_alpha_per_rad
        sm = (x_np - x_cg) / c_ref

    if not has_ih:
        if abs(cl_alpha) > 1e-9:
            trim_alpha = a_lo + (cl_target - float(r1["CL"])) / cl_alpha
        else:
            trim_alpha = a_lo
        return StabilityTrimResult(
            x_np=x_np,
            static_margin=sm,
            cl_alpha=cl_alpha,
            trim_alpha_deg=trim_alpha,
            trim_ih_deg=float("nan"),
            cl_ih=0.0,
            cm_ih=0.0,
        )

    i_h0 = float(hstab.xsecs[0].twist)
    for xs in hstab.xsecs:
        xs.twist = i_h0 + delta_ih
    try:
        r3 = vlm(a_lo)
    finally:
        for xs in hstab.xsecs:
            xs.twist = i_h0

    cl_ih = float(r3["CL"] - r1["CL"]) / delta_ih
    cm_ih = float(r3["Cm"] - r1["Cm"]) / delta_ih

    # Closed-form 2x2 solve: [[CL_alpha, CL_ih], [Cm_alpha, Cm_ih]] @ [d_a, d_ih]
    #                        = [cl_target - CL_1, -Cm_1]  (all slopes per degree)
    jac = np.array([[cl_alpha, cl_ih], [cm_alpha, cm_ih]])
    rhs = np.array([cl_target - float(r1["CL"]), -float(r1["Cm"])])
    try:
        d_a, d_ih = np.linalg.solve(jac, rhs)
    except np.linalg.LinAlgError:
        d_a = (cl_target - float(r1["CL"])) / cl_alpha if abs(cl_alpha) > 1e-9 else 0.0
        d_ih = 0.0

    return StabilityTrimResult(
        x_np=x_np,
        static_margin=sm,
        cl_alpha=cl_alpha,
        trim_alpha_deg=a_lo + float(d_a),
        trim_ih_deg=i_h0 + float(d_ih),
        cl_ih=cl_ih,
        cm_ih=cm_ih,
    )


def tail_volume_coefficients(
    airplane: asb.Airplane,
) -> Tuple[Optional[float], Optional[float]]:
    """Horizontal/vertical tail volume coefficients (Vh, Vv).

        Vh = Sh * Lh / (S * c_bar)      Vv = Sv * Lv / (S * b)

    ``None`` for either if ``airplane`` has no h-stab/v-stab (fewer than 2/3
    wings). The single source of truth for this formula -- both
    ``optimization/objective.py``'s penalty term and
    ``reporting/visualization.py::figure_control_surfaces`` call this rather
    than each inlining the formula, so the optimizer's constraint and the
    sizing diagram can never silently disagree on what Vh/Vv actually is.
    """
    s_ref = max(airplane.s_ref, 1.0)
    c_bar = max(airplane.c_ref, 0.1)
    b_ref = max(airplane.b_ref, 1.0)
    x_wing_ac = float(airplane.wings[0].aerodynamic_center()[0])

    vh = None
    if len(airplane.wings) > 1:
        hstab = airplane.wings[1]
        l_h = max(0.0, float(hstab.aerodynamic_center()[0]) - x_wing_ac)
        vh = float(hstab.area()) * l_h / (s_ref * c_bar)

    vv = None
    if len(airplane.wings) > 2:
        vstab = airplane.wings[2]
        l_v = max(0.0, float(vstab.aerodynamic_center()[0]) - x_wing_ac)
        vv = float(vstab.area()) * l_v / (s_ref * b_ref)

    return vh, vv
