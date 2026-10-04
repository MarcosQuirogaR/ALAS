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

## Optimizer: `solver` and `objective`

`optimizer.solver` is covered in
[Design space & optimizer](../design-space-and-optimizer.md#the-search-differential-evolution).
`optimizer.objective` holds what is minimised and how it is bounded:

| Field | Default | Meaning |
|---|---|---|
| `kind` | `block_fuel` | Block fuel, takeoff mass, operating empty mass, or fuel per seat-kilometre |
| `design_range_nmi` | 0 | Still-air design range; 0 uses the great-circle distance of the route |
| `mtow_sizing` | `sized_by_mission` (presets: `fixed_requirement`) | [Takeoff-mass mode](../design-space-and-optimizer.md#takeoff-mass-modes) |
| `mtow_target_kg`, `mtow_band_fraction` | 0 (= `mtow_kg`), 0.05 | Target and half-width for `mtow_band` |
| `sizing_max_iterations`, `sizing_tolerance_kg` | 30, 1 kg | Sizing-loop budget and closure tolerance |
| `retrim_cg_tolerance_pct_mac` | 0.1 | CG shift that triggers a re-trim inside the loop |
| `preference_weight` | 10 | Scale of study-preference terms; never a constraint allowance |
| `aerodrome_reference_code` | `F` (`auto` for a clean-sheet brief) | ICAO Annex 14 letter capping the wingspan; `unrestricted` disables |
| `max_approach_speed_kt` | 0 | Approach-speed limit; 0 disables |

Constraints are hard; there are no per-constraint penalty scales. The
`requirements` fields (wing-area cap, wing-loading floor, cruise-CL cap,
static-margin floor, CG range) are the bounds a candidate must satisfy.

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

Whether native mission analysis runs, and directory configuration for navigation data:

| Field | Default | Meaning |
|---|---|---|
| `enabled` | `true` | Run native mission segment analysis as part of the pipeline |
| `navdata_dir` / `routes_dir` | `alas/data/{navdata,routes}` | Where downloaded navigation data and cached routes live |
| `great_circle_points` | `50` | Resolution of the great-circle fallback route |
| `timeout_s` | `120.0` | Maximum time allowed for mission simulation before reporting timeout |

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
