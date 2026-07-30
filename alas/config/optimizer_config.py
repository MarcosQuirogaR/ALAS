# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Optimizer configuration -- solver settings and objective/penalty weights.

All penalty weights are dimensionless and calibrated so that each term
contributes O(1-50) in the neighbourhood of a good design. This means the
optimizer can simultaneously "see" all constraints without any single penalty
drowning out the aerodynamic objective (L/D).

Penalty philosophy
------------------
* Primary reward : L/D x ld_weight  (typically 15-25 for a clean transport)
* Soft constraints: each penalty is normalized so it equals ~10 at the design
  boundary, rising steeply beyond.  This lets the solver escape infeasible
  regions rather than hitting a flat wall.
* Static margin: a SOFT target-tracking term, (SM - SM_target)^2 x
  static_margin_penalty_scale, is the only mechanism keying off static margin
  directly. ``cg_penalty_scale`` below is legacy/unused (see its own field
  docstring) -- it does NOT apply a separate (Delta x_cg/MAC)^2 term.
* CG envelope: exceedance beyond [fwd_limit, aft_limit] (% MAC, from DesignRequirements)
  is penalised by (exceedance_fraction)^2 x cg_envelope_penalty_scale.
* Fuel penalty: (-m_fuel / MTOW) x fuel_penalty_scale -- normalized by MTOW
  so it equals 1 when OEW+payload exactly fills the MTOW budget.

Every weight is user-adjustable from Advanced Settings -> Optimizer & weights
(field ``metadata`` below supplies the descriptive label/unit/tooltip the
auto-generated form shows for each one).
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Optional


@dataclass
class ObjectiveWeights:
    """Weights and thresholds shaping the scalar cost the optimizer minimises.

    The objective (to be *minimised*, see ``optimization/objective.py`` for the
    authoritative, up-to-date term-by-term assembly) is built from a primary
    L/D reward plus soft, continuous penalty terms:

        cost = -ld_weight * (L/D)
             + alpha_penalty_scale  * alpha_error^2         [deg^2, only outside the alpha window]
             + span_penalty_per_m   * span_m                [m]
             + cd0_penalty_scale    * CD0                   [-]
             + area_penalty_scale   * max(0, dS/S_max)^2    [-]
             + wing_loading_penalty_scale * max(0, dWS/WS_min)^2
             + static_margin_penalty_scale * (SM-SM_tgt)^2  [soft target-tracking]
             + cg_envelope_penalty_scale * max(0, dcg_exceedance)^2  [graduated, see below]
             + fuel_penalty_scale   * max(0, -m_fuel/MTOW)  [-]
             + fuel_volume_penalty_scale * max(0, dfuel/m_fuel)^2  [-]
             + taper_realism_penalty_scale * max(0, break/root - max_break_root_chord_ratio)^2
             + several other structural/geometric soft penalties (tail area/volume,
               wing position, fineness ratio, payload shortfall, TE-root angle, ...)

    Only ONE true hard reject short-circuits this assembly entirely and returns
    a flat cost (``failure_cost``) instead of the accumulated formula above:
    a candidate that fails to *build or evaluate* at all (geometry/mass/VLM
    crash, or ``cl_target > max_cruise_cl``, the stall guard -- no sensible
    operating point exists to score). A candidate that builds and evaluates
    but is *physically invalid* -- ``static_margin < min_physical_static_margin``
    or a CG-envelope violation -- is NOT an early return: L/D is still computed
    and a large-but-continuous graduated penalty (using ``instability_failure_cost``
    /``cg_envelope_penalty_scale`` as its severity scale) is added on top, so the
    solver never loses the L/D signal needed to climb back toward compliance.
    See ``optimization/objective.py``'s module docstring and
    ``docs/methods.md`` Sec 12/16 for the full rationale and history.
    """

    # --- Primary reward ---
    ld_weight: float = field(
        default=1.0,
        metadata={
            "label": "L/D reward weight",
            "help": "Primary reward multiplier on L/D. Raise to prioritize aerodynamic efficiency over all penalties below.",
        },
    )

    # --- Trim-alpha band penalty ---
    alpha_penalty_scale: float = field(
        default=5.0,
        metadata={
            "label": "Trim-alpha penalty weight",
            "help": "Penalizes cruise trim alpha outside [alpha_min_penalty_deg, alpha_max_penalty_deg]. "
            "Zero cost inside the window, quadratic outside. Kept small so the optimizer focuses on L/D.",
        },
    )
    alpha_min_penalty_deg: float = field(
        default=0.0,
        metadata={
            "label": "Trim-alpha window: minimum",
            "unit": "deg",
            "help": "Lower bound of the acceptable cruise trim-alpha window (no penalty above this).",
        },
    )
    alpha_max_penalty_deg: float = field(
        default=10.0,
        metadata={
            "label": "Trim-alpha window: maximum",
            "unit": "deg",
            "help": "Upper bound of the acceptable cruise trim-alpha window (no penalty below this).",
        },
    )

    # --- Structural span proxy ---
    span_penalty_per_m: float = field(
        default=0.02,
        metadata={
            "label": "Wingspan penalty (per metre)",
            "help": "Small linear penalty per metre of span -- discourages excessively large wings.",
        },
    )

    # --- Parasitic drag floor ---
    cd0_penalty_scale: float = field(
        default=50.0,
        metadata={
            "label": "Parasite-drag (CD0) penalty weight",
            "help": "Rewards lower parasite drag (CD0); a thinner/cleaner shape reduces this term's cost. "
            "Typical cruise CD0 is 0.016-0.020.",
        },
    )

    # --- Wing sizing constraints (soft) ---
    area_penalty_scale: float = field(
        default=0.5,
        metadata={
            "label": "Max wing-area penalty weight",
            "help": "Penalty per m^2 the wing area exceeds requirements.max_wing_area_m2.",
        },
    )
    wing_loading_penalty_scale: float = field(
        default=0.005,
        metadata={
            "label": "Min wing-loading penalty weight",
            "help": "Penalty per (kg/m^2)^2 below requirements.min_wing_loading_kg_m2.",
        },
    )

    # --- Center-of-gravity alignment ---
    cg_penalty_scale: float = field(
        default=200.0,
        metadata={
            "label": "(Legacy, unused) CG/aero-balance mismatch weight",
            "help": "NOT read by the cost function (objective.py) -- kept only so old saved YAML configs "
            "referencing this key still load without error. Originally intended to penalize "
            "(Delta x_cg / MAC)^2, but that formula was never actually wired up; the field's real "
            "runtime effect was fully redundant with static_margin_penalty_scale, since both applied "
            "to the identical static-margin-vs-target term, so the two are consolidated into that "
            "single, correctly-named, appropriately-soft term. Physical CG-envelope compliance is "
            "enforced separately by cg_envelope_penalty_scale/cg_envelope_reward below.",
        },
    )

    # --- CG envelope enforcement ---
    cg_envelope_penalty_scale: float = field(
        default=400000.0,
        metadata={
            "label": "CG-envelope violation penalty weight",
            "help": "Penalizes the physical CG exceeding the [fwd, aft] CG envelope limits (% MAC, from Design Requirements). "
            "At 5% MAC beyond the limit the cost is comparable to one L/D unit, rising steeply further out.",
        },
    )
    cg_envelope_reward: float = field(
        default=5.0,
        metadata={
            "label": "CG-envelope compliance reward",
            "help": "Reward applied to cost if the aircraft's entire operational CG envelope is within limits.",
        },
    )

    # --- Fuel budget ---
    fuel_penalty_scale: float = field(
        default=50.0,
        metadata={
            "label": "Negative-fuel penalty weight",
            "help": "Penalizes negative fuel mass (OEW + payload exceeding MTOW), normalized by MTOW.",
        },
    )

    # --- Wing fuel-volume capacity ---
    fuel_volume_penalty_scale: float = field(
        default=300.0,
        metadata={
            "label": "Insufficient wing fuel-volume penalty weight",
            "help": "Penalizes the wing's physical usable fuel-tank volume (physics.performance.wing_fuel_volume_m3, "
            "Torenbeek geometric estimate) being too small to hold the fuel mass the weight & balance analysis "
            "says this design actually needs -- a wing that's too thin/small/tapered to carry its own required "
            "fuel is not a buildable aircraft, independent of whether the MTOW fuel-mass budget itself closes. "
            "Quadratic on the fractional shortfall (required_fuel - tank_capacity) / required_fuel.",
        },
    )

    # --- Static-margin deviation ---
    static_margin_penalty_scale: float = field(
        default=20.0,
        metadata={
            "label": "Static-margin target penalty weight",
            "help": "SOFT preference nudging compliant-but-suboptimal candidates toward requirements.target_static_margin "
            "-- NOT a hard requirement (the physical floor, min_physical_static_margin, and the CG-envelope "
            "itself are enforced separately and are what actually keep a design safe/legal). Kept deliberately "
            "small relative to -L/D (typically 15-25) so this doesn't crowd out genuine aerodynamic "
            "improvements: raising it much above ~20-30 risks the optimizer chasing an exact SM match instead "
            "of exploring shape space, overwhelming the L/D signal the search is meant to prioritize.",
        },
    )

    # --- Thickness floor ---
    thickness_floor: float = field(
        default=0.90,
        metadata={
            "label": "Minimum airfoil thickness scale",
            "help": "Minimum allowed airfoil thickness scale (relative to the reference section) before the thickness penalty kicks in.",
        },
    )
    thickness_penalty_scale: float = field(
        default=20.0,
        metadata={
            "label": "Thin-airfoil penalty weight",
            "help": "Penalty weight applied when the morphed airfoil thickness collapses below thickness_floor.",
        },
    )

    # --- Fuselage length floor ---
    fuselage_floor_m: float = field(
        default=60.0,
        metadata={
            "label": "Minimum fuselage length",
            "unit": "m",
            "help": "Minimum allowed fuselage length before the too-short-fuselage penalty kicks in.",
        },
    )
    fuselage_penalty_scale: float = field(
        default=5.0,
        metadata={
            "label": "Too-short-fuselage penalty weight",
            "help": "Penalty weight applied when the fuselage shrinks below fuselage_floor_m.",
        },
    )

    # --- Tail-area fraction constraints ---
    min_hstab_area_fraction: float = field(
        default=0.15,
        metadata={
            "label": "Minimum H-stab area fraction",
            "help": "Minimum allowed horizontal-stabiliser area as a fraction of wing area. "
            "Typical transports: ~20-30%. Prevents a 'tiny tail on a huge moment arm' cheat.",
        },
    )
    min_vstab_area_fraction: float = field(
        default=0.07,
        metadata={
            "label": "Minimum V-stab area fraction",
            "help": "Minimum allowed vertical-stabiliser area as a fraction of wing area. Typical transports: ~8-14%.",
        },
    )
    tail_area_penalty_scale: float = field(
        default=150.0,
        metadata={
            "label": "Tail-area-deficit penalty weight",
            "help": "Penalty weight for tail area fractions below their minimums (stiff quadratic).",
        },
    )

    # --- Tail volume coefficient constraints ---
    min_hstab_volume_coef: float = field(
        default=0.75,
        metadata={
            "label": "Minimum H-stab volume coefficient (Vh)",
            "help": "Lower bound on the horizontal-tail volume coefficient Vh = Sh*Lh/(S*c_bar), which captures tail "
            "effectiveness accounting for its moment arm, not just area (Etkin/Reid convention).",
        },
    )
    max_hstab_volume_coef: float = field(
        default=1.25,
        metadata={
            "label": "Maximum H-stab volume coefficient (Vh)",
            "help": "Upper bound on Vh -- penalises an oversized tail / an unnecessarily stretched fuselage moment arm.",
        },
    )
    min_vstab_volume_coef: float = field(
        default=0.06,
        metadata={
            "label": "Minimum V-stab volume coefficient (Vv)",
            "help": "Lower bound on the vertical-tail volume coefficient Vv = Sv*Lv/(S*b).",
        },
    )
    max_vstab_volume_coef: float = field(
        default=0.13,
        metadata={
            "label": "Maximum V-stab volume coefficient (Vv)",
            "help": "Upper bound on Vv (typical jet-transport max is ~0.12).",
        },
    )
    tail_volume_penalty_scale: float = field(
        default=200.0,
        metadata={
            "label": "Tail-volume-coefficient penalty weight",
            "help": "Quadratic penalty weight for Vh/Vv falling outside their [min, max] bounds.",
        },
    )

    # --- Wing taper realism (closes the "free MAC inflation" loophole) ---
    max_break_root_chord_ratio: float = field(
        default=0.65,
        metadata={
            "label": "Max break/root chord ratio",
            "help": "Upper bound on break_chord_m / root_chord_m. Real transport wings taper noticeably from root to "
            "the yehudi break (typically 0.45-0.65); without this bound the optimizer can inflate the break "
            "chord toward the root chord to enlarge MAC (c_ref) 'for free', which cheapens every "
            "%MAC-normalised penalty (CG envelope, static-margin target) without a real stability improvement. "
            "Soft penalty above this ratio, not a hard bound.",
        },
    )
    taper_realism_penalty_scale: float = field(
        default=250.0,
        metadata={
            "label": "Break-chord taper-realism penalty weight",
            "help": "Penalty weight applied when break_chord_m / root_chord_m exceeds max_break_root_chord_ratio.",
        },
    )

    # --- Wing-root trailing-edge angle (structural realism) ---
    te_root_angle_penalty_scale: float = field(
        default=100.0,
        metadata={
            "label": "Wing-root trailing-edge angle penalty weight",
            "help": "Penalizes the wing's root-to-break trailing edge (seen in planform) making an angle greater than "
            "90 deg with the fuselage centerline -- i.e. the break station's trailing edge sitting forward of "
            "the root's. That creates a reflex (concave) corner at the wing-fuselage junction: a severe stress "
            "concentration no real transport-category wing root has, caused by a short root chord combined "
            "with a comparatively long break chord and/or too little sweep. Quadratic on the angle exceedance "
            "beyond 90 deg.",
        },
    )

    # --- Wing longitudinal position ---
    min_wing_position_fraction: float = field(
        default=0.27,
        metadata={
            "label": "Minimum wing position (fraction of fuselage length)",
            "help": "Wing-root leading edge must sit at least this fraction of fuselage length aft of the nose -- "
            "prevents the optimizer placing the wing in the cockpit. Typical transports: 25-55%.",
        },
    )
    wing_position_penalty_scale: float = field(
        default=300.0,
        metadata={
            "label": "Wing-too-far-forward penalty weight",
            "help": "Penalty weight applied when the wing sits forward of min_wing_position_fraction.",
        },
    )

    # --- Payload / seating shortfall ---
    payload_shortfall_penalty_scale: float = field(
        default=1000.0,
        metadata={
            "label": "Payload-shortfall penalty weight",
            "help": "Steep quadratic penalty on the fractional shortfall vs. the target passenger count / cargo payload -- "
            "guides the optimizer to grow the fuselage long enough to actually fit the requested payload.",
        },
    )

    # --- Fineness ratio (fuselage length / diameter) ---
    fineness_ratio_max: float = field(
        default=15.0,
        metadata={
            "label": "Maximum fuselage fineness ratio",
            "help": "Maximum allowed fuselage length/diameter ratio before the too-slender-fuselage penalty kicks in.",
        },
    )
    fineness_ratio_penalty_scale: float = field(
        default=500.0,
        metadata={
            "label": "Slender-fuselage penalty weight",
            "help": "Penalty weight applied when the fuselage fineness ratio exceeds fineness_ratio_max.",
        },
    )

    # --- Failure cost ---
    failure_cost: float = field(
        default=1_000.0,
        metadata={
            "label": "Invalid-design cost",
            "help": "Cost returned for any candidate design that raises an exception or fails a hard guard during evaluation.",
        },
    )
    instability_failure_cost: float = field(
        default=1_000.0,
        metadata={
            "label": "Instability reject cost",
            "help": "Severity scale for the static-margin-floor penalty applied to a candidate that builds and analyses "
            "successfully but is rejected as physically invalid (static margin below "
            "requirements.min_physical_static_margin). NOT a flat returned cost -- this floor is graduated, not "
            "an early return (objective.py's `(deficit * severity)**3` term, where this field sets `severity` "
            "so raising/lowering it steepens/relaxes the penalty without editing code; the default reproduces "
            "the exact cubic constant used before this field was wired up). Kept distinct from failure_cost so "
            "run diagnostics can tell 'geometry/analysis crashed' apart from 'physically unstable' rejects "
            "(see OptimizationHistory.reject_reason_counts).",
        },
    )


@dataclass
class SolverSettings:
    """SciPy ``differential_evolution`` settings."""

    strategy: str = field(
        default="best1bin",
        metadata={
            "label": "DE mutation/crossover strategy",
            "help": "SciPy differential_evolution strategy name (e.g. 'best1bin', 'rand1bin', 'best2bin') -- "
            "controls how new candidate designs are generated from the population each generation.",
        },
    )
    max_iterations: int = field(
        default=15,
        metadata={
            "label": "Max generations",
            "help": "Maximum number of generations (iterations) the solver runs before stopping.",
        },
    )
    population_size: int = field(
        default=6,
        metadata={
            "label": "Population size multiplier",
            "help": "Population size as a multiplier on the number of design variables -- more candidates per generation "
            "explores more broadly but costs more evaluations.",
        },
    )
    tolerance: float = field(
        default=0.01,
        metadata={
            "label": "Convergence tolerance",
            "help": "Relative tolerance for convergence; the solver stops early once the population's cost spread falls below this.",
        },
    )
    seed: Optional[int] = field(
        default=None,
        metadata={
            "label": "Random seed",
            "help": "Set an integer for a reproducible run (same seed -> same result); leave blank for a different search each run.",
        },
    )
    workers: int = field(
        default=1,
        metadata={
            "label": "Parallel worker processes",
            "help": "Number of worker processes for parallel evaluation (>1 uses multiprocessing). "
            "Requires a picklable objective -- already the case for ALAS's optimizer.",
        },
    )
    display_progress: bool = field(
        default=True,
        metadata={
            "label": "Print progress to console",
            "help": "Print a one-line progress summary (valid count, best L/D so far) after each generation.",
        },
    )
    seed_near_initial_design: bool = field(
        default=True,
        metadata={
            "label": "Seed search near the initial design",
            "help": "Initialize the population as a tight cluster of small perturbations around the initial/preset "
            "design (plus the design itself, unperturbed) instead of SciPy's default uniform latin-hypercube "
            "coverage of the whole bounds space. Guarantees at least one known-valid, physically-balanced "
            "design is in generation 0, and lets the solver refine from there instead of having to rediscover "
            "CG/stability balance from scratch across the full 16-D space. Disable to fall back to the old "
            "full-space exploration (e.g. if you specifically want to explore far from the initial design).",
        },
    )
    seed_perturbation_fraction: float = field(
        default=0.05,
        metadata={
            "label": "Seed cluster perturbation size",
            "help": "Size of the initial random perturbation around the initial design, as a fraction of each design "
            "variable's (upper - lower) bound range. Only used when seed_near_initial_design is enabled. "
            "Small values (e.g. 0.05) start with a tight, mostly-valid cluster; larger values explore more "
            "broadly from the start at the cost of more of the population starting off invalid.",
        },
    )


@dataclass
class OptimizerConfig:
    """Composed optimizer configuration."""

    weights: ObjectiveWeights = None
    solver: SolverSettings = None

    def __post_init__(self):
        if self.weights is None:
            self.weights = ObjectiveWeights()
        if self.solver is None:
            self.solver = SolverSettings()
