# Configuration reference

A compact index of the configuration ALAS reads (`AlasConfig`). Fields already
explained in depth elsewhere in this guide are linked rather than
re-described; this page exists for the fields that do not have a narrative
chapter of their own, and as a lookup table for everything else. Every field
in the application carries a label, a unit, valid bounds and a help string, and
the desktop forms are generated from that metadata, so the application is
self-documenting field by field. Treat this page as an index into it, not a
replacement.

## How a configuration file works

- A configuration is YAML or JSON. Load one with `-c, --config <PATH>` or
  **File, Load configuration**; write the effective configuration with
  `--save-config <PATH>` or **File, Save configuration**.
- A file names a subset of the fields, to any depth. A `preset` key is applied
  first, so the rest of the file is read as changes to that aircraft rather than
  to the defaults.
- Unknown field names and values of the wrong type are rejected with a message
  naming the field. Keys retired in earlier versions are migrated on load, with
  a note.
- The desktop application adds one `alas_workspace` entry (workspace mode,
  last sandbox design, window layout). It is removed before the configuration
  is decoded, so the file stays valid for every other consumer.
- Lengths, masses and forces are SI. Angles are degrees at the configuration
  boundary (names ending `_deg`); speeds are m/s unless the name says `_kt`;
  altitudes are metres (`_m`) except where a name says `_ft`. Dimensionless
  fields carry no suffix.

## Top-level groups

| Group | What it holds | Where to read more |
|---|---|---|
| `preset` | Name of the aircraft preset the configuration started from, or blank | [Installation](../installation.md), [Meet AVE](../meet-ave.md) |
| `requirements` | The mission targets: cruise point, takeoff mass, payload, structural and stability limits | [Mission requirements](../mission-requirements.md) |
| `geometry` | The geometry scaffold: wing, empennage, fuselage, engine installation | [Meet AVE](../meet-ave.md), [Sandbox mode](../sandbox.md) |
| `optimizer` | Design-space mode and bounds, plausibility limits, solver, objective | [Design space & optimizer](../design-space-and-optimizer.md) |
| `analysis` | Analysis fidelity: sweeps, panel resolution, probes, polar fitting | [Aerodynamic analysis](../aerodynamic-analysis.md) |
| `drag_model` | Parasite drag build-up and wave-drag assumptions | [Formulas & theory](formulas.md) |
| `performance` | High-lift coefficients, thrust lapse, speed schedule, field-length constants | [Low-speed & field performance](../field-performance.md) |
| `mass_model` | Mass architecture (FLOPS transport by default), declared inputs, gear load limits, fuel density and usable fraction | [Weight, balance & stability](../weight-balance-and-stability.md) |
| `landing_gear` | Gear sizing, tip-back, rotation and takeoff-trim settings | below |
| `cabin` | Passenger classes or the cargo deck | [Cabin & payload](../cabin-and-payload.md) |
| `mission` | Native mission analysis, route sources, speed profile | [Mission & route analysis](../mission-and-route.md) |
| `mses` | The optional transonic section solver | [Transonic section analysis](../transonic-analysis.md) |
| `control_surfaces` | Chord and span fractions of slats, flaps, ailerons, elevator, rudder | [User guide](../user-guide.md) |
| `propulsion_cycle` | On-design turbofan cycle assumptions | [Propulsion analysis](../propulsion-analysis.md) |
| `structures` | Wing-box layout, materials, mesh, solver cases | [Structural analysis](../structural-analysis.md) |
| `downstream` | Optional OpenVSP, VSPAERO, AVL and FLOWUnsteady stages | [External tools guide](../external-tools.md) |
| `fuel_policy` | Operating rule for taxi, contingency, alternate and final-reserve fuel | below |
| `fuel_tanks` | Tank arrangement and the published capacities of a registered aircraft | below |
| `departure_airport`, `arrival_airport` | Route endpoints, by display name | [Mission & route analysis](../mission-and-route.md) |

## Requirements: `requirements`

Fully covered in [Mission requirements](../mission-requirements.md).
Structural-sizing fields not covered there:

| Field | Default | Meaning |
|---|---|---|
| `ultimate_load_factor` | 3.75 | Limit load × 1.5 safety margin, feeds the structural-mass formulas |
| `dive_speed_m_s` | 220.0 | Design dive speed (V_D on the V-n diagram); design cruise speed is derived as V_D/1.25 |
| `limit_load_factor_neg` | −1.0 | CS-25.337(c) negative limit load factor |
| `cg_range_pct_mac` | 30.0 | CG envelope width; forward limit = aft limit − this |
| `min_physical_static_margin` | 0.05 | Hard floor on physical-CG static margin; designs below this are rejected outright |
| `passenger_mass_kg` | 100.0 | Combined mass per occupant, body plus baggage; the single load-case authority. A transparent project default, not a universal value: record the operator, population and baggage method before using another one |
| `min_passenger_capacity` | 0 | Hard floor on the geometry-resolved seat count; 0 disables it |
| `cargo_objective_kg` | 0 | Freighter cargo target the search is rewarded for approaching; 0 disables it. Not a floor |
| `max_structural_payload_kg` | 0 | Cap on payload with belly freight; 0 disables it |

## Design space: `DesignVector` and `optimizer.design_space`

All sixteen variables, defaults and bounds are tabulated in
[Design space & optimizer](../design-space-and-optimizer.md#sixteen-degrees-of-freedom).
`optimizer.design_space` controls how the search treats the starting aircraft:

| Field | Meaning |
|---|---|
| `mode` | `clean_sheet` (every variable free inside its global bounds; fuselage sized by the cabin), `reference_adaptation` (the registered aircraft is the reference and only the listed variables move inside configured windows) or `baseline_sandbox` (replay the aircraft as given, no optimizer-driven resizing; the mode of the sandbox and of **Analyze reference**) |
| `fuselage_sized_by_cabin` | Clean sheet only: derive the fuselage length from the cabin |
| `reference_fixed_variables` and `reference_*_half_width` | Which variables a reference adaptation holds fixed, and the half-width of the window of the others |
| `initial_design`, `bounds` | Per-variable overrides of the derived start and box |
| `clean_sheet_brief` | Whether the brief derives its own start and bounds |

## Optimizer: `solver`, `objective` and `weights`

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

`optimizer.solver` holds `method` (`differential_evolution`), the two stage
budgets `screening` and `refinement` (each an evaluation ceiling and a time
limit in seconds, at most 300), `tolerance`, `seed`, `workers` and
`stop_on_evaluations_only`. A seed alone does not reproduce a run, because a
stage that stops on its time limit analyses as many candidates as the machine
affords. With `stop_on_evaluations_only: true` every stage stops on its
evaluation ceiling, and a seeded run is bit-identical at any worker count. The
optional `replay_*` fields in a stage budget reproduce a recorded
time-limited run the same way.

Constraints are hard; there are no per-constraint penalty scales. The
`requirements` fields (wing-area cap, wing-loading floor, cruise-CL cap,
static-margin floor, CG range) and the plausibility limits in
`optimizer.weights` (wing-box depth and packaging, flap area, taper realism,
body angle) are the bounds a candidate must satisfy.

## Analysis fidelity: `analysis`

| Field | Default | Meaning |
|---|---|---|
| `sweep_alpha_min/max_deg`, `sweep_n_points` | −4° / 10° / 15 | The AoA sweep behind [Aerodynamic analysis](../aerodynamic-analysis.md)'s four-panel figure |
| `spanwise/chordwise_resolution` | 1 / 8 | VLM panel resolution during optimization (fast) |
| `fine_spanwise/chordwise_resolution` | 1 / 16 | VLM panel resolution for the final analysis pass (accurate) |
| `probe_alpha_low/high_deg` | 2° / 3° | Two-point AoA probe used to estimate lift-curve slope quickly |
| `trim_incidence_probe_delta_deg` | 1.0 | Perturbation used to solve for trim tail incidence |
| `autobalance_velocity_m_s`, `autobalance_alpha_low/high_deg` | 250 / 0° / 2° | Conditions used for the automatic CG-balancing probe |
| `tail_efficiency` | 0.90 | Dynamic-pressure recovery factor at the tail |
| `include_fuselage_stability` | true | Whether fuselage contribution is included in the stability derivatives |
| `polar_fit_cl_min/max`, `..._fallback` | 0.3–0.6, 0.1–0.8 | CL range the final drag-polar curve fit is anchored to |
| `avl_timeout_s` | 600 | Time allowed for the AVL cross-check |

## Mission: `mission`

Whether native mission analysis runs, route sources and the speed profile:

| Field | Default | Meaning |
|---|---|---|
| `enabled` | `true` | Run native mission segment analysis as part of the pipeline |
| `navdata_dir`, `routes_dir` | `alas/data/{navdata,routes}` | Where downloaded navigation data and cached routes live |
| `great_circle_points` | `50` | Resolution of the great-circle fallback route |
| `max_airway_stretch` | `1.2` | Largest allowed airway route length over the great circle |
| `simbrief_username`, `simbrief_timeout_s`, `simbrief_overrides_airports` | blank, 15, `true` | Importing a filed SimBrief flight plan |
| `timeout_s` | `900.0` | Maximum time allowed for mission simulation before reporting timeout |

`mission.profile` has about thirty fields (take-off and climb rates and speeds,
three cruise-leg speed and distance fractions, a four-step descent ladder with
altitudes in feet, landing speed and sink rate). They are covered narratively in
[Mission & route analysis](../mission-and-route.md#the-full-profile); the
defaults reproduce a long-haul transport's climb schedule, scaled to whatever
route distance the current run flies.

## Landing gear and takeoff rotation: `landing_gear`

Tyre, strut and placement settings are labelled in the application. The fields
behind the ground-attitude checks (see
[Formulas & theory](formulas.md#takeoff-rotation-the-forward-cg-limit) and
[Weight, balance & stability](../weight-balance-and-stability.md#tail-down-tip-back-and-main-gear-placement)):

| Field | Default | Meaning |
|---|---|---|
| `min_tip_back_deg` | 0 | Extra floor on the tip-back angle; the required angle is `max(floor, tail-scrape angle)`. 15 reproduces the Raymer/Roskam rule of thumb |
| `turnover_angle_limit_deg` | 63 | Lateral turnover limit |
| `required_rotation_angle_deg` | 10 | Pitch attitude at rotation that the tail-scrape angle must reach; a representative transport assumption, not a certified value |
| `fuselage_ground_clearance_m` | unset (0.25 × fuselage diameter) | Static belly clearance from the fuselage lower surface to the ground line |
| `rotation_pitch_acceleration_deg_s2` | unset (class value 5) | Override of the required pitch acceleration at rotation |
| `pitch_radius_of_gyration_frac_mac` | unset | Override of the pitch radius of gyration (fraction of MAC); unset derives it from the mass ledger |
| `takeoff_stabilizer_nose_up_deg` | unset | Nose-up setting of a trimmable stabiliser at takeoff; unset keeps a fixed stabiliser at its built incidence |
| `elevator_up_travel_deg` | unset (class value 25) | Trailing-edge-up elevator travel at rotation |
| `derived_main_gear` | not editable | The main-gear group translation solved for a redesigned candidate, saved with the delivered design |

## Fuel: `fuel_policy` and `fuel_tanks`

`fuel_policy` states which operating rule supplies taxi, contingency, alternate
and final-reserve fuel (for example 5 % trip contingency, a 370 km diversion and
30 minutes of holding in the EASA basic scheme the reference run uses) and the
operator assumptions the rule leaves open. `fuel_tanks` states which wing,
centre, trim and auxiliary tanks the aircraft has, the semispan stations that
bound them, and the published capacities of a registered aircraft. Takeoff fuel
is capped at the usable tank capacity in every
[takeoff-mass mode](../design-space-and-optimizer.md#takeoff-mass-modes).

## The geometry scaffold: `geometry`

`geometry` holds `wing`, `empennage`, `fuselage` and `engine` (the values in
[Meet AVE](../meet-ave.md)), plus the wetted-area factors. The engine entry
names an engine from the catalogue; the rest of its fields are derived from the
catalogue and installation. These are values held fixed per preset while the
optimizer searches the 16-D design space. They are "advanced" settings: safe to
leave at a preset's defaults, and there to define a *different* aircraft family
rather than tune the current one. For a registered preset they are protected
from manual edits in the guided workspace; the [sandbox](../sandbox.md) edits
them freely.

Optional geometry fields (all unset by default, which keeps the plain shape):

| Field | Meaning |
|---|---|
| `wing.flight_tip_rise_semispan_fraction` | Rise of the wing tip from the static ground shape to the 1 g flight shape, as a fraction of the semispan |
| `wing.custom_sections`, `fuselage.custom_sections` | Extra sections you define (station, chord or width and height, position) |
| `fuselage.belly_upsweep_length_m` | Station of the start of the rising lower line ahead of the tailcone; sets the tail-down angle |
| `fuselage.nose_windshield_angle_deg`, `nose_crown_end_fraction`, `nose_radome_length_fraction`, `nose_keel_exponent`, `nose_plan_exponent`, `nose_section_exponent` | Shaped nose in place of the single-ellipsoid nose |
| `fuselage.hump_height_m`, `hump_start_x_m`, `hump_crown_start_x_m`, `hump_crown_end_x_m`, `hump_end_x_m`, `hump_fairing_exponent` | Upper-deck hump over a constant crown |
| `fuselage.upper_deck_floor_height_m`, `upper_deck_start_x_m`, `upper_deck_end_x_m` | Make the hump a passenger deck |
