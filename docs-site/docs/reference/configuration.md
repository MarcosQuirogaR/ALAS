# Configuration reference

A compact, scannable index of every config dataclass ALAS exposes.
Fields already explained in depth elsewhere in this guide are linked
rather than re-described; this page exists for the fields that don't have
a narrative chapter of their own, and as a lookup table for everything
else. Every field in the actual codebase also carries a `help` string:
the GUI is self-documenting field by field, so treat this page as an index
into that, not a replacement for it.

## Requirements: `DesignRequirements`

Fully covered in [Mission requirements](../mission-requirements.md).
Structural-sizing fields not covered there:

| Field | AVE default | Meaning |
|---|---|---|
| `ultimate_load_factor` | 3.75 | Limit load × 1.5 safety margin, feeds the Torenbeek structural-mass formulas |
| `dive_speed_m_s` | 220.0 | Design dive speed (V_D on the V-n diagram); design cruise speed is derived as V_D/1.25 |
| `limit_load_factor_neg` | −1.0 | CS-25.337(c) negative limit load factor |
| `cg_range_pct_mac` | 30.0 | CG envelope width; forward limit = aft limit − this |
| `min_physical_static_margin` | 0.05 | Hard floor on physical-CG static margin; designs below this are rejected outright |
| `passenger_mass_kg` | 100.0 | Mass per occupant, FAA AC 120-27E standard |

## Design space: `DesignVector` / `DESIGN_VARIABLE_SPECS`

Fully covered in
[Design space & optimizer](../design-space-and-optimizer.md#sixteen-degrees-of-freedom):
all sixteen variables, defaults, and bounds are tabulated there.

## Cabin: `CabinConfig`

Fully covered in [Cabin & payload](../cabin-and-payload.md).

## Optimizer: `SolverSettings` / `ObjectiveWeights`

`SolverSettings` is fully covered in
[Design space & optimizer](../design-space-and-optimizer.md#the-search-differential-evolution).
`ObjectiveWeights` penalty terms not already named there:

| Field | Default | Penalizes |
|---|---|---|
| `ld_weight` | 1.0 | (primary term, not a penalty) reward for cruise L/D |
| `alpha_penalty_scale` / `alpha_min/max_penalty_deg` | 5.0 / 0°–10° | Cruise AoA drifting outside a sane window |
| `span_penalty_per_m` | 0.02 | Linear cost per meter of span (a mild leverage against unbounded growth) |
| `cd0_penalty_scale` | 50.0 | Excess parasite drag |
| `area_penalty_scale` / `wing_loading_penalty_scale` | 0.5 / 0.005 | Violating `max_wing_area_m2` / `min_wing_loading_kg_m2` |
| `cg_penalty_scale` / `cg_envelope_penalty_scale` | 200.0 / 400,000 | CG drifting from target / outside the operational envelope |
| `cg_envelope_reward` | 5.0 | Bonus for margin *inside* the CG envelope, not just avoiding violation |
| `fuel_penalty_scale` / `fuel_volume_penalty_scale` | 50.0 / 300.0 | Fuel-budget mismatch / fuel physically not fitting in the wing |
| `static_margin_penalty_scale` | 20.0 | Static margin below target |
| `thickness_floor` / `thickness_penalty_scale` | 0.90 / 20.0 | Airfoil thickness scaled too far below nominal |
| `fuselage_floor_m` / `fuselage_penalty_scale` | 60.0 / 5.0 | Fuselage shrunk implausibly short |
| `min_h/vstab_area_fraction`, `tail_area_penalty_scale` | 0.15 / 0.07 / 150.0 | Tail undersized relative to wing |
| `min/max_h/vstab_volume_coef`, `tail_volume_penalty_scale` | see code / 200.0 | Tail volume coefficient outside a realistic band |
| `max_break_root_chord_ratio`, `taper_realism_penalty_scale` | 0.65 / 250.0 | Implausible taper shape |
| `te_root_angle_penalty_scale` | 100.0 | Unrealistic trailing-edge root angle |
| `min_wing_position_fraction`, `wing_position_penalty_scale` | 0.27 / 300.0 | Wing shifted too far forward on the fuselage |
| `payload_shortfall_penalty_scale` | 1000.0 | Payload budget not met |
| `fineness_ratio_max`, `fineness_ratio_penalty_scale` | 15.0 / 500.0 | Fuselage fineness ratio (length/diameter) too high |
| `failure_cost` / `instability_failure_cost` | 1,000.0 each | Fixed cost for a hard-infeasible or unstable candidate |

## Analysis fidelity: `AnalysisConfig`

| Field | Default | Meaning |
|---|---|---|
| `sweep_alpha_min/max_deg`, `sweep_n_points` | −4° / 10° / 15 | The AoA sweep behind [Aerodynamic analysis](../aerodynamic-analysis.md)'s four-panel figure |
| `spanwise/chordwise_resolution` | 1 / 1 | VLM panel resolution during optimization (fast) |
| `fine_spanwise/chordwise_resolution` | 2 / 8 | VLM panel resolution for the final analysis pass (accurate) |
| `probe_alpha_low/high_deg` | 2° / 3° | Two-point AoA probe used to estimate lift-curve slope quickly |
| `trim_incidence_probe_delta_deg` | 1.0 | Perturbation used to solve for trim tail incidence |
| `autobalance_velocity_m_s`, `autobalance_alpha_low/high_deg` | 250 / 0° / 2° | Conditions used for the automatic CG-balancing probe |
| `tail_efficiency` | 0.90 | Dynamic-pressure recovery factor at the tail |
| `include_fuselage_stability` | true | Whether fuselage contribution is included in the stability derivatives |
| `polar_fit_cl_min/max`, `..._fallback` | 0.3–0.6, 0.1–0.8 | CL range the final drag-polar curve fit is anchored to |

## Mission: `MissionConfig` / `MissionProfileConfig`

Whether mission analysis runs at all, and where the isolated environment
lives:

| Field | Default | Meaning |
|---|---|---|
| `enabled` | true | Run SUAVE mission analysis as part of every `DesignPipeline.run()` |
| `suave_venv_dir` | `.suave-venv` | Isolated Python 3.10 environment, repo-root-relative |
| `navdata_dir` / `routes_dir` | `alas/data/{navdata,routes}` | Where the optional airway-graph download and cached routes live |
| `great_circle_points` | 50 | Resolution of the always-available great-circle fallback route |
| `timeout_s` | none | Subprocess timeout for the SUAVE run |

`MissionProfileConfig`'s ~30 fields (climb rates/speeds, three cruise-leg
speed/distance fractions, a four-step descent ladder) are covered
narratively in
[Mission & route analysis](../mission-and-route.md#the-full-profile); the
defaults reproduce the original Madrid-Nairobi validation script's climb
schedule, scaled to whatever route distance the current run actually
flies.

## The geometry scaffold

`GeometryConfig` (wing, empennage, fuselage, engine placement: the values
in [Meet AVE](../meet-ave.md)), `config/engines.py` (the engine database:
BPR/OPR/FPR/TIT per engine, [Propulsion analysis](../propulsion-analysis.md)),
`config/materials.py`, `config/structures_config.py`, and
`config/landing_gear_config.py` round out the scaffold: values held fixed
per-preset while the optimizer searches the 16-D design space. They're
"advanced" settings in the GUI sense: safe to leave at a preset's
defaults, and there specifically for defining a *different* aircraft
family rather than tuning the current one.
