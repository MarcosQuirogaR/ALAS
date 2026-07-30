# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Optimization objective.

Defines the scalar cost the design optimizer minimises for a candidate design
vector. The cost is structured so every term is *O(1-50)* in the vicinity of a
good design, so the optimizer can simultaneously see aerodynamic efficiency
(L/D) and all physical constraints.

Design philosophy
-----------------
* Primary reward: -ld_weight x L/D.  For a long-range transport L/D ~ 16-20,
  so the primary reward contributes roughly -16 to -20.
* All soft penalties are normalised so they are small (< 2) for a typical
  compliant candidate, and rise steeply outside feasible bounds -- kept well
  below the L/D reward's own magnitude so genuine aerodynamic improvements
  aren't crowded out by chasing small, easy-to-shrink penalty terms.
* Physical CG-envelope compliance is enforced separately and is NOT this
  section's target-tracking term: `cg_envelope_violation`/`_check_cg_envelope`
  (a hard-but-continuous penalty, `cg_envelope_penalty_scale`) plus
  `cg_envelope_reward` for compliance is the actual mechanism that keeps the
  physical CG inside the certified [fwd, aft] envelope.
* The alpha penalty uses a *window* [alpha_min, alpha_max] rather than a
  single target to avoid forcing the optimizer to find unrealistic trim states.
  Typical transport cruise is 3-7°; only extreme out-of-band alphas are
  penalised.
* Static-margin TARGET penalty: ``static_margin_penalty_scale x (SM_physical -
  SM_target)²`` -- a SOFT preference nudging compliant-but-suboptimal
  candidates toward `target_static_margin`, not a hard requirement (the
  physical stability floor, `min_physical_static_margin`, is enforced
  separately via `sm_floor_violation`). Deliberately the ONLY mechanism
  keying off static margin directly: `cg_penalty_scale` (a legacy,
  misleadingly-named weight for a (Δx_cg/MAC)² formula that was never
  actually implemented) is unused, so this term is never double-counted
  against a second, much larger weight -- a combined effective magnitude
  comparable to or larger than L/D's own range across the search space would
  let the optimizer's cost keep falling generation-over-generation while true
  (fine-resolution) L/D got WORSE than baseline.

Every weight is exposed in ObjectiveWeights (config/optimizer_config.py) and
therefore editable at runtime via Advanced Settings -> Optimizer & weights.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from typing import Dict, List, Tuple

import aerosandbox as asb
import aerosandbox.numpy as np

from ..config.design_variables import DesignVector
from ..config.settings import ALASConfig
from ..geometry.aircraft_builder import AircraftBuilder
from ..physics.aerodynamics import AeroAnalysis
from ..physics.mass import OEW_KEYS
from ..physics.stability import stability_and_trim


@dataclass
class OptimizationHistory:
    """Per-evaluation log, used for convergence plots and diagnostics."""

    design_vectors: List[np.ndarray] = field(default_factory=list)
    valid: List[bool] = field(default_factory=list)
    cost: List[float] = field(default_factory=list)
    l_over_d: List[float] = field(default_factory=list)
    span_m: List[float] = field(default_factory=list)
    alpha_deg: List[float] = field(default_factory=list)
    area_m2: List[float] = field(default_factory=list)
    trim_ih_deg: List[float] = field(default_factory=list)
    reject_reason: List[str] = field(default_factory=list)

    def record(
        self,
        x,
        valid,
        cost,
        *,
        ld=None,
        span=None,
        alpha=None,
        area=None,
        trim_ih=None,
        reason="",
    ):
        self.design_vectors.append(np.array(x, dtype=float))
        self.valid.append(valid)
        self.cost.append(cost)
        self.reject_reason.append(reason if not valid else "")
        if valid:
            self.l_over_d.append(ld)
            self.span_m.append(span)
            self.alpha_deg.append(alpha)
            self.area_m2.append(area)
            self.trim_ih_deg.append(trim_ih)

    @property
    def n_evaluations(self) -> int:
        return len(self.valid)

    @property
    def n_valid(self) -> int:
        return sum(self.valid)

    @property
    def reject_reason_counts(self) -> Dict[str, int]:
        """Tally of invalid evaluations by reason, e.g. {'geometry_build': 12,
        'cg_envelope': 5, 'static_margin': 3, 'stall_guard': 40}. Empty string
        (valid candidates) excluded."""
        from collections import Counter

        return dict(Counter(r for r, v in zip(self.reject_reason, self.valid) if not v))


def _check_cg_envelope(
    plane,
    masses,
    coords,
    cg_x: float,
    x_np: float,
    mac: float,
    config,
) -> Tuple[bool, float]:
    """Check the physical CG against the dynamic NP and gear-limited envelope at OEW/MZFW/MTOW.

    Limits are calculated relative to the aircraft neutral point (NP):
        aero_aft_lim = NP_pct - target_static_margin * 100  (the trimmed aero CG)
        aero_fwd_lim = aero_aft_lim - req.cg_range_pct_mac
    And then clipped dynamically by landing gear structural and steering loads.
    """
    req = config.requirements
    mm = config.mass_model
    if mm is None:
        from ..config.mass_config import MassModelConfig

        mm = MassModelConfig()

    oew_mass = sum(masses.get(k, 0.0) for k in OEW_KEYS)
    oew_cg_x = sum(
        masses.get(k, 0.0) * coords.get(k, [0.0])[0] for k in OEW_KEYS
    ) / max(oew_mass, 1.0)

    payload_mass = masses.get("Payload", 0.0)
    payload_cg_x = coords.get("Payload", [0.0])[0]
    mzfw_mass = oew_mass + payload_mass
    mzfw_cg_x = (oew_mass * oew_cg_x + payload_mass * payload_cg_x) / max(
        mzfw_mass, 1.0
    )

    fuel_mass = masses.get("Fuel", 0.0)
    mtow_mass = oew_mass + payload_mass + max(0.0, fuel_mass)

    # Convert to % MAC helper
    x_wing_ac_val = float(plane.wings[0].aerodynamic_center()[0])
    x_mac_le = x_wing_ac_val - 0.25 * mac

    def to_pct(x_val: float) -> float:
        return ((x_val - x_mac_le) / max(mac, 0.001)) * 100.0

    # Aerodynamic limits
    np_pct = to_pct(x_np)
    aero_aft_lim = np_pct - req.target_static_margin * 100.0
    aero_fwd_lim = aero_aft_lim - req.cg_range_pct_mac

    # Gear geometry
    nlg_x_frac = mm.nlg_x_fraction
    mlg_x_frac_mac = mm.mlg_x_fraction_mac
    pct_nlg_min = mm.pct_load_nlg_min

    fus = plane.fuselages[0]
    fus_start_x = fus.xsecs[0].xyz_c[0]
    fus_end_x = fus.xsecs[-1].xyz_c[0]
    fus_len = fus_end_x - fus_start_x

    x_nlg = fus_start_x + fus_len * nlg_x_frac
    x_mlg = x_mac_le + mlg_x_frac_mac * mac
    wheelbase = x_mlg - x_nlg

    # Gear strength limits: DERIVED from a real wheel/tire sizing at the
    # aerodynamic CG limits (see physics.landing_gear), not a fixed guess --
    # this is what makes gear sizing actually facilitate CG-envelope
    # compliance. Cheap (pure arithmetic, no VLM), safe to call every
    # candidate. Falls back to mm.pct_load_nlg_max/pct_load_mlg_max (kept in
    # MassModelConfig for exactly this) if sizing raises for any reason.
    try:
        from ..physics.landing_gear import size_landing_gear

        gear_cfg = getattr(config, "landing_gear", None)
        if gear_cfg is None:
            from ..config.landing_gear_config import LandingGearConfig

            gear_cfg = LandingGearConfig()
        aero_fwd_lim_x = x_mac_le + aero_fwd_lim / 100.0 * mac
        aero_aft_lim_x = x_mac_le + aero_aft_lim / 100.0 * mac
        fus_diam = getattr(config.geometry.fuselage, "diameter_m", 4.0)
        gear = size_landing_gear(
            mtow_mass,
            x_nlg,
            x_mlg,
            aero_fwd_lim_x,
            aero_aft_lim_x,
            fuselage_diameter_m=fus_diam,
            cg_height_estimate_m=fus_diam * 1.1,
            gear_config=gear_cfg,
        )
        pct_nlg_max = gear.pct_load_nlg_max
        pct_mlg_max = gear.pct_load_mlg_max
    except Exception:
        pct_nlg_max = mm.pct_load_nlg_max
        pct_mlg_max = mm.pct_load_mlg_max

    # Gear load limits in kg (based on MTOW)
    load_nlg_max = mtow_mass * pct_nlg_max
    load_mlg_max = mtow_mass * pct_mlg_max
    load_nlg_min = mtow_mass * pct_nlg_min

    worst_exc = 0.0
    violation = False

    loading_states = [(oew_cg_x, oew_mass), (mzfw_cg_x, mzfw_mass), (cg_x, mtow_mass)]

    for cg_val, w_state in loading_states:
        w_safe = max(w_state, 1.0)
        cg_pct = to_pct(cg_val)

        # Calculate gear-limited bounds at current weight
        nlg_strength_limit = to_pct(x_mlg - (load_nlg_max * wheelbase / w_safe))
        mlg_strength_limit = to_pct(x_nlg + (load_mlg_max * wheelbase / w_safe))
        min_nose_load_limit = to_pct(x_mlg - (load_nlg_min * wheelbase / w_safe))

        fwd_lim_dynamic = max(aero_fwd_lim, nlg_strength_limit)
        aft_lim_dynamic = min(aero_aft_lim, mlg_strength_limit, min_nose_load_limit)

        exc = 0.0
        if cg_pct < fwd_lim_dynamic - 0.01:
            exc = (fwd_lim_dynamic - cg_pct) / 100.0
        elif cg_pct > aft_lim_dynamic + 0.01:
            exc = (cg_pct - aft_lim_dynamic) / 100.0

        if exc > 0.0:
            violation = True
            worst_exc = max(worst_exc, exc)

    return violation, worst_exc


class DesignObjective:
    """Callable cost function for SciPy's optimizer.

    Construct once with a fully-populated :class:`ALASConfig`, then pass
    the instance as the objective to the optimizer.  Each call evaluates one
    candidate design and appends to :attr:`history`.
    """

    def __init__(self, config: ALASConfig):
        self.config = config
        self.builder = AircraftBuilder(config.geometry)
        self.history = OptimizationHistory()

        # Keep original targets so we don't lose them when mutating req in the loop
        self.target_num_passengers = int(config.requirements.num_passengers)
        self.target_cargo_payload_kg = float(config.requirements.cargo_payload_kg)

    def __call__(self, x) -> float:
        req = self.config.requirements
        w = self.config.optimizer.weights

        # --- build geometry (engines off for speed during the search) ---
        try:
            dv = DesignVector.from_array(x)

            # Dynamically compute and apply cabin layout preset counts and update requirements payload based on fuselage length
            from ..physics.payload import apply_cabin_preset

            apply_cabin_preset(self.config, dv)

            plane = self.builder.build(dv, include_engines=False)
        except Exception:
            self.history.record(
                x, valid=False, cost=w.failure_cost, reason="geometry_build"
            )
            return w.failure_cost

        # --- sizing hard-guards (reject before expensive VLM) ---
        if plane.s_ref <= 0 or plane.c_ref <= 0:
            self.history.record(
                x, valid=False, cost=w.failure_cost, reason="geometry_build"
            )
            return w.failure_cost

        # --- weight and balance / mass analysis ---
        # Two passes, mirroring FullAnalysis.run: a first pass with the lumped
        # payload model to get OEW/x_oew, then a second pass with the detailed
        # cabin/cargo layout (real seat classes, galley/lav/exit bays, ULD bag
        # positions) so the CG this objective checks against -- and therefore
        # optimizes toward -- is the same CG the final report/plot shows.
        # Scoring against the lumped model's CG alone (payload smeared
        # uniformly across the occupied cabin length) can differ enough from
        # the detailed model's CG that a candidate scored as compliant during
        # the search would land outside the plotted operational envelope once
        # the final analysis recomputes it with real geometry.
        m_fuel = req.mtow_kg * 0.20  # generous fallback; overwritten on success
        cg_x = plane.xyz_ref[0]  # fallback
        try:
            from ..physics.mass import run_mass_analysis

            masses, coords, cg = run_mass_analysis(
                plane, req, self.config.geometry, self.config.mass_model
            )
            try:
                from ..physics.payload import build_payload_layout, oew_and_cg

                oew, x_oew = oew_and_cg(masses, coords)
                payload_layout = build_payload_layout(plane, self.config, oew, x_oew)
                masses, coords, cg = run_mass_analysis(
                    plane,
                    req,
                    self.config.geometry,
                    self.config.mass_model,
                    payload_layout=payload_layout,
                )
            except Exception:
                pass  # non-fatal: fall back to the lumped payload CG
            m_fuel = masses.get("Fuel", 0.0)
            cg_x = cg[0]
        except Exception:
            self.history.record(
                x, valid=False, cost=w.failure_cost, reason="mass_analysis"
            )
            return w.failure_cost

        # Anchor aerodynamic moment reference to the actual physical CG
        plane.xyz_ref[0] = cg_x

        # --- cruise target CL + stall guard (cheap check on already-known
        # cl_target -- done before the VLM stability/trim solve so a design
        # that would fail this guard anyway doesn't spend VLM calls) ---
        try:
            atmo = asb.Atmosphere(altitude=req.cruise_altitude_m)
            q = 0.5 * atmo.density() * (req.cruise_mach * atmo.speed_of_sound()) ** 2
            cl_target = req.required_cruise_cl(q, plane.s_ref)
        except Exception:
            self.history.record(
                x, valid=False, cost=w.failure_cost, reason="stall_guard"
            )
            return w.failure_cost

        if cl_target > req.max_cruise_cl or cl_target <= 0.0:
            self.history.record(
                x, valid=False, cost=w.failure_cost, reason="stall_guard"
            )
            return w.failure_cost

        # --- stability + trim: cruise-condition 3-point VLM probe giving both
        # the physical static margin AND a closed-form trim solve (alpha,
        # horizontal-stabilizer incidence) -- see stability.stability_and_trim
        # / methods.md Sec 9f. A separate neutral_point() (2 VLM calls) +
        # quick_performance() (2 VLM calls, untrimmed) would cost one more
        # VLM solve per candidate for no extra information: this single
        # 3-VLM-call probe already captures real trim drag. ---
        try:
            trim = stability_and_trim(
                plane,
                self.config.analysis,
                cl_target,
                req.cruise_mach,
                req.cruise_altitude_m,
            )
            x_np, sm_physical, _cl_alpha = trim.x_np, trim.static_margin, trim.cl_alpha
        except Exception:
            self.history.record(
                x, valid=False, cost=w.failure_cost, reason="stability_solve"
            )
            return w.failure_cost

        # --- Physical-validity checks: minimum static margin + CG envelope.
        # These do NOT early-return -- see the "assemble the cost" section
        # below for why. Only the *flags*/exceedance are computed here (mass/
        # CG/NP are already known and don't need the aero result yet). ---
        sm_floor_violation = (
            sm_physical != sm_physical or sm_physical < req.min_physical_static_margin
        )
        mac = max(plane.c_ref, 0.1)
        cg_envelope_violation, cg_exc_max = _check_cg_envelope(
            plane,
            masses,
            coords,
            cg_x,
            x_np,
            mac,
            self.config,
        )

        # --- trimmed cruise performance: the one extra VLM solve at the
        # closed-form trimmed (alpha, i_h) -- genuine trim drag, no ad-hoc
        # formula bolted on (the VLM's own induced-drag response to the
        # tail's required lift/download at the trimmed point IS the trim
        # drag). See aerodynamics.AeroAnalysis.trimmed_performance.
        #
        # Deliberately run for EVERY candidate, even one that's already
        # known to violate the static-margin floor or CG envelope: an early
        # return for those cases, without computing L/D at all, would starve
        # SciPy's differential_evolution of any signal distinguishing an
        # aerodynamically-promising infeasible candidate from a poor one. A
        # compliant region DOES exist in the search space (e.g.
        # wing_x_shift_m <~ -3), but with L/D blanked out for every
        # infeasible candidate the solver has no way to smoothly co-optimize
        # "get into compliance" and "have good L/D" simultaneously, and
        # converges on an infeasible "best of a bad lot" design instead. See
        # docs/methods.md Sec 12/16. ---
        try:
            aero = AeroAnalysis(
                plane,
                sweep_deg=dv.sweep_deg,
                geometry=self.config.geometry,
                drag_model=self.config.drag_model,
                analysis=self.config.analysis,
            )
            perf = aero.trimmed_performance(
                trim, req.cruise_mach, req.cruise_altitude_m
            )
            ld = perf["L/D"]
            alpha = perf["alpha"]
            trim_ih = perf["i_h"]
            cd0 = aero.parasite_drag(req.cruise_mach, req.cruise_altitude_m, cl_target)

        except Exception:
            self.history.record(
                x, valid=False, cost=w.failure_cost, reason="trim_solve"
            )
            return w.failure_cost

        # --- assemble the cost ---
        cost = -w.ld_weight * ld

        # Payload / Seating shortfall penalty
        shortfall_pct = 0.0
        if req.aircraft_type == "cargo":
            if req.cargo_payload_kg < self.target_cargo_payload_kg:
                shortfall_pct = (
                    self.target_cargo_payload_kg - req.cargo_payload_kg
                ) / max(1.0, self.target_cargo_payload_kg)
        else:
            if (
                req.cabin_preset != "Custom"
                and req.num_passengers < self.target_num_passengers
            ):
                shortfall_pct = (self.target_num_passengers - req.num_passengers) / max(
                    1.0, self.target_num_passengers
                )

        if shortfall_pct > 0.0:
            # Steep quadratic penalty to guide the optimizer to make fuselage long enough
            cost += shortfall_pct**2 * w.payload_shortfall_penalty_scale

        # Tail-area fractions -- prevent structurally degenerate tails
        s_wing_ref = max(1.0, plane.s_ref)
        if len(plane.wings) > 1:
            ratio_h = plane.wings[1].area() / s_wing_ref
            if ratio_h < w.min_hstab_area_fraction:
                deficit = (
                    w.min_hstab_area_fraction - ratio_h
                ) / w.min_hstab_area_fraction
                cost += deficit**2 * w.tail_area_penalty_scale
        if len(plane.wings) > 2:
            ratio_v = plane.wings[2].area() / s_wing_ref
            if ratio_v < w.min_vstab_area_fraction:
                deficit = (
                    w.min_vstab_area_fraction - ratio_v
                ) / w.min_vstab_area_fraction
                cost += deficit**2 * w.tail_area_penalty_scale

        # Tail volume coefficients -- prevent stretched-fuselage / tiny-tail exploits.
        # Vh = Sh * Lh / (S * c_bar), Vv = Sv * Lv / (S * b) -- shared with
        # figure_control_surfaces via stability.tail_volume_coefficients, so
        # the diagram shows the exact number this penalty enforces.
        # Penalise both deficiency (undersized tail) and excess (artificially long arm).
        try:
            from ..physics.stability import tail_volume_coefficients

            vh, vv = tail_volume_coefficients(plane)

            if vh is not None:
                if vh < w.min_hstab_volume_coef:
                    deficit = (w.min_hstab_volume_coef - vh) / max(
                        w.min_hstab_volume_coef, 1e-6
                    )
                    cost += deficit**2 * w.tail_volume_penalty_scale
                elif vh > w.max_hstab_volume_coef:
                    excess = (vh - w.max_hstab_volume_coef) / max(
                        w.max_hstab_volume_coef, 1e-6
                    )
                    cost += excess**2 * w.tail_volume_penalty_scale

            if vv is not None:
                if vv < w.min_vstab_volume_coef:
                    deficit = (w.min_vstab_volume_coef - vv) / max(
                        w.min_vstab_volume_coef, 1e-6
                    )
                    cost += deficit**2 * w.tail_volume_penalty_scale
                elif vv > w.max_vstab_volume_coef:
                    excess = (vv - w.max_vstab_volume_coef) / max(
                        w.max_vstab_volume_coef, 1e-6
                    )
                    cost += excess**2 * w.tail_volume_penalty_scale
        except Exception:
            pass  # non-fatal: volume coefficient calculation failed

        # Wing position -- wing root LE must not encroach on the nose/cockpit
        x_wing_le = plane.wings[0].xsecs[0].xyz_le[0]
        wing_pos_frac = x_wing_le / max(1.0, dv.fuselage_length_m)
        if wing_pos_frac < w.min_wing_position_fraction:
            deficit = w.min_wing_position_fraction - wing_pos_frac
            cost += deficit**2 * w.wing_position_penalty_scale

        # Corrected fineness ratio calculation (guarded against a zero/near-zero
        # diameter, which would otherwise raise ZeroDivisionError and kill the
        # whole differential_evolution run since this section is unguarded).
        fuselage_diameter_m = max(self.config.geometry.fuselage.diameter_m, 1e-6)
        fineness_ratio = dv.fuselage_length_m / fuselage_diameter_m
        if fineness_ratio > w.fineness_ratio_max:
            cost += (
                fineness_ratio - w.fineness_ratio_max
            ) ** 2 * w.fineness_ratio_penalty_scale

        # Alpha window penalty -- only penalise extreme out-of-band values
        if alpha < w.alpha_min_penalty_deg:
            cost += (alpha - w.alpha_min_penalty_deg) ** 2 * w.alpha_penalty_scale
        elif alpha > w.alpha_max_penalty_deg:
            cost += (alpha - w.alpha_max_penalty_deg) ** 2 * w.alpha_penalty_scale

        # Span structural proxy
        cost += dv.span_m * w.span_penalty_per_m

        # Parasitic drag floor (rewards cleaner shapes)
        cost += cd0 * w.cd0_penalty_scale

        # Wing area soft constraint  [m^2]
        if plane.s_ref > req.max_wing_area_m2:
            excess_frac = (plane.s_ref - req.max_wing_area_m2) / max(
                1.0, req.max_wing_area_m2
            )
            cost += excess_frac**2 * w.area_penalty_scale

        # Wing loading floor  [kg/m^2]
        ws = req.mtow_kg / plane.s_ref
        if ws < req.min_wing_loading_kg_m2:
            deficit_frac = (req.min_wing_loading_kg_m2 - ws) / max(
                1.0, req.min_wing_loading_kg_m2
            )
            cost += deficit_frac**2 * w.wing_loading_penalty_scale

        # Static-margin TARGET deviation -- a SOFT preference nudging
        # valid-but-suboptimal candidates toward target_static_margin; NOT a
        # hard physical requirement (that's sm_floor_violation below). Kept
        # small relative to -L/D (~15-25) and the CG-envelope terms so it
        # steers without dominating the aerodynamic search. A now-removed
        # duplicate application of this exact sm_err**2 term
        # under `cg_penalty_scale` (200, on top of this term's own
        # `static_margin_penalty_scale`, then 2.0) gave a combined effective
        # weight of ~202 -- comparable to or larger than L/D's own range
        # across the search space, so the optimizer's cost kept improving
        # generation over generation by chasing an exact SM match while true
        # (fine-resolution) L/D got WORSE than baseline. `cg_penalty_scale`
        # was named for a different, never-actually-implemented (Δx_cg/MAC)²
        # CG-vs-aerodynamic-center term; kept in the schema (a stale/inert
        # field, not read here) only so old saved YAML configs referencing it
        # don't fail to load. Exponential/steeper once the mismatch exceeds
        # 0.5 MAC, same as before.
        sm_err = sm_physical - req.target_static_margin
        if abs(sm_err) > 0.5:
            cost += (abs(sm_err) * 10.0) ** 3
        else:
            cost += sm_err**2 * w.static_margin_penalty_scale

        # --- Physical-validity penalties: static-margin floor + CG envelope.
        # Both are large-but-CONTINUOUS additive terms (not early returns --
        # see the note above `trimmed_performance`), so a candidate deep in
        # violation still costs far more than any compliant one (even a
        # small deficit's cubed penalty below dwarfs the ~15-25 range of
        # -L/D), while the solver retains a smooth L/D + exceedance signal to
        # climb back toward compliance rather than losing all sense of which
        # infeasible candidates are "closer" to a good design. The 20.0/1_000
        # multiplier below rescales `w.instability_failure_cost` to a
        # per-unit-deficit-cubed constant: its legacy default (1_000) sets
        # the SEVERITY of the cubic rather than being returned directly
        # (deficit=0.05 -> cost=1_000, matching what a flat cost at a typical
        # violation depth would give; smaller/larger deficits scale smoothly
        # instead of jumping straight to 1_000). ---
        if sm_floor_violation:
            deficit = (
                max(0.0, req.min_physical_static_margin - sm_physical)
                if sm_physical == sm_physical
                else 1.0
            )
            severity = (w.instability_failure_cost / 1_000.0) ** (1.0 / 3.0) * 20.0
            cost += (deficit * severity) ** 3

        if cg_envelope_violation:
            cost += (cg_exc_max * 100.0) ** 2 * w.cg_envelope_penalty_scale
        else:
            # CG-envelope compliance reward.
            cost -= w.cg_envelope_reward

        # Fuel budget -- normalised by MTOW  [-]
        if m_fuel < 0.0:
            cost += (-m_fuel / max(1.0, req.mtow_kg)) * w.fuel_penalty_scale

        # Wing fuel-volume capacity -- the wing must be physically big enough
        # to HOLD the fuel mass the W&B analysis says this design needs, not
        # just fit it within the MTOW budget (the check above). Uses the same
        # Torenbeek geometric tank-volume estimate the Weight & Balance tab's
        # fuel-volume-check figure already shows (physics.performance.
        # wing_fuel_volume_m3), so this penalty and that plot can never
        # silently disagree. Soft/continuous (fractional shortfall squared),
        # not a hard reject, consistent with every other physical-validity
        # penalty in this function.
        try:
            from ..physics.performance import wing_fuel_volume_m3

            mm_fv = self.config.mass_model
            if mm_fv is None:
                from ..config.mass_config import MassModelConfig

                mm_fv = MassModelConfig()
            tank_capacity_kg = (
                wing_fuel_volume_m3(plane.wings[0], mm_fv.fuel_tank_usable_fraction)
                * mm_fv.fuel_density_kg_m3
            )
            required_fuel_kg = max(0.0, m_fuel)
            if required_fuel_kg > tank_capacity_kg:
                shortfall = (required_fuel_kg - tank_capacity_kg) / max(
                    1.0, required_fuel_kg
                )
                cost += shortfall**2 * w.fuel_volume_penalty_scale
        except Exception:
            pass  # non-fatal: skip the check rather than failing the whole candidate

        # Airfoil thickness floor
        if dv.airfoil_thickness_scale < w.thickness_floor:
            cost += (
                w.thickness_floor - dv.airfoil_thickness_scale
            ) * w.thickness_penalty_scale

        # Fuselage length floor
        if dv.fuselage_length_m < w.fuselage_floor_m:
            cost += (
                w.fuselage_floor_m - dv.fuselage_length_m
            ) * w.fuselage_penalty_scale

        # Wing taper realism (closes the "inflate MAC via a blunt yehudi break
        # to cheat %MAC-normalised CG/SM penalties" loophole). break_chord_m
        # is a free DOF (6-10 m) while the break's spanwise location is fixed
        # (GeometryConfig.wing.break_span_fraction); pushing break_chord_m
        # toward root_chord_m enlarges c_ref (MAC) directly, which shrinks
        # every %MAC-normalised term below (cg_envelope_penalty_scale, the
        # static-margin target penalty via static_margin_penalty_scale) without
        # genuinely improving stability -- a real-only reward-hacking avenue,
        # not a byproduct of the standard %MAC static-margin definition.
        taper_ratio_break = dv.break_chord_m / max(dv.root_chord_m, 0.1)
        if taper_ratio_break > w.max_break_root_chord_ratio:
            excess = taper_ratio_break - w.max_break_root_chord_ratio
            cost += excess**2 * w.taper_realism_penalty_scale

        # Wing-root trailing-edge angle -- seen in planform, the root-to-break
        # trailing-edge segment must not sweep FORWARD of the root by more
        # than a right angle relative to the fuselage centerline (i.e. the
        # break station's TE sitting ahead of the root's TE), which would
        # create a reflex/concave corner at the wing-fuselage junction -- a
        # severe stress concentration no real transport wing root has.
        # Derived straight from the design vector/geometry scaffold (the
        # root-to-break segment IS exactly what these DOFs fix) rather than
        # re-querying the built wing's finely subdivided xsecs.
        wing_g = self.config.geometry.wing
        y_break_root = wing_g.break_span_fraction * (dv.span_m / 2.0)
        if y_break_root > 1e-9:
            dx_break_root = y_break_root * math.tan(math.radians(dv.sweep_deg))
            te_dx_root = dx_break_root + dv.break_chord_m - dv.root_chord_m
            te_angle_deg = math.degrees(math.atan2(y_break_root, te_dx_root))
            if te_angle_deg > 90.0:
                excess_deg = te_angle_deg - 90.0
                cost += excess_deg**2 * w.te_root_angle_penalty_scale

        # Fuselage empty stretch penalty (closes the "free" tail volume loophole)
        cabin_start = self.config.geometry.fuselage.cabin_start_x_m
        tailcone_len = self.config.geometry.fuselage.tailcone_length_m
        cabin_len = max(1.0, dv.fuselage_length_m - cabin_start - tailcone_len)
        from ..config.mass_config import MassModelConfig

        mm = self.config.mass_model or MassModelConfig()
        occupied_len = min(
            cabin_len, req.payload_kg / max(mm.cabin_payload_density_kg_m, 1e-6)
        )
        empty_stretch = max(0.0, cabin_len - occupied_len)
        if empty_stretch > 0.0:
            cost += empty_stretch**2 * w.fuselage_penalty_scale

        # Check final validity and build a combined reject reason (a
        # candidate can fail more than one physical-validity check at once).
        # These are the soft-invalid categories: the candidate's cost above
        # already carries the corresponding continuous penalties (plus its
        # real L/D), but it must not be recorded as a *valid* design --
        # otherwise reject_reason_counts can never tally them and the
        # convergence/validity diagnostics (n_valid, best-L/D-so-far) count
        # physically non-compliant candidates as good ones.
        reasons = []
        if sm_floor_violation:
            reasons.append("static_margin")
        if cg_envelope_violation:
            reasons.append("cg_envelope")
        if shortfall_pct > 0.0:
            reasons.append("payload_shortfall")
        is_valid = not reasons

        self.history.record(
            x,
            valid=is_valid,
            cost=cost,
            ld=ld,
            span=dv.span_m,
            alpha=alpha,
            area=plane.s_ref,
            trim_ih=trim_ih,
            reason="+".join(reasons),
        )
        return cost
