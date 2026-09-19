# How the design optimization is driven, and what it reads

This is the operational answer to "what happens when I press Run with
optimization enabled, and which inputs decide the result". The methods are
in `docs/methods.md` ("Multidisciplinary sizing loop and gradient-based
driver"); this page is the wiring and the input list.

## The driving path

1. **Entry.** The GUI Run page and the headless CLI (`alas --config <yaml>
   [--optimization-method <m>] [--optimization-solver vlm|avl|both]
   [--seed <n>] [--no-optimize]`) both build `PipelineOptions` and call the
   same `DesignPipeline`. `optimize = true` is the default.
2. **Stage 2, design-space optimization.** `run_solver_optimizations` runs
   the requested branches (native VLM, external AVL, or both in parallel).
   Each branch constructs `DesignOptimizer::new(config)` and calls
   `run(bounds, Some(preset design vector))`. The bounds are the sixteen
   design-variable bounds of `alas_config::design_variables::SPECS`, recentred
   on the preset when one is loaded.
3. **Method dispatch.** `optimizer.solver.method` selects the driver:
   `differential_evolution` (the SciPy-parity loop, feasibility first),
   `feasibility_first_de`, `nsga2`, `turbo_1`, `cma_es`, or `sqp` (the
   gradient-based driver). Every method scores candidates through the same
   `DesignObjective`.
4. **One evaluation.** Geometry build from the design vector; the candidate
   payload load case; a two-pass mass analysis with the payload layout;
   cruise trim and drag polar by the vortex-lattice method; then the sizing
   loop (`mdo::mda`): mass and CG at the current takeoff mass, re-trim when
   the CG has moved, mission fuel from the Breguet model under the fuel
   policy, new takeoff mass, until the takeoff mass settles. The residual
   table (mass and fuel, balance, performance, geometry) and the objective
   are assembled into the scalar cost the method ranks by.
5. **Result.** The best design is re-analysed at full fidelity
   (`FullAnalysis`) and becomes the optimized report; the history feeds the
   convergence figure and the run manifest. The AVL branch scores the same
   objective around AVL's induced drag (`assess_candidate_with_polar`).

The frozen weighted lift-to-drag objective of the Python reference is not
selectable. It exists only behind `DesignObjective::new_reference_compatibility`
for the parity fixtures.

## Inputs the mission-sized search reads

Grouped by configuration group. "Default" is what a fresh configuration
holds; presets override the physical inputs.

### `optimizer.objective`: what is minimised and how it is bounded

| Field | Role | Default |
| --- | --- | --- |
| `kind` | block fuel, takeoff mass, operating empty mass, fuel per seat-kilometre | `block_fuel` |
| `design_range_nmi` | still-air design range; zero uses the great-circle distance between the departure and arrival aerodromes | 0 |
| `mtow_sizing` | takeoff mass closed by the mission (`requirements.mtow_kg` is the ceiling) or fixed at the requirement | `sized_by_mission` |
| `sizing_max_iterations`, `sizing_tolerance_kg` | sizing-loop budget and closure tolerance | 30, 1 kg |
| `retrim_cg_tolerance_pct_mac` | CG shift that triggers a re-trim inside the loop; zero keeps one trim | 0.1 |
| `mass_constraints`, `balance_constraints`, `performance_constraints`, `geometry_constraints` | hard, soft, diagnostic or off per family | hard |
| `max_span_m`, `max_approach_speed_kt` | aerodrome span limit, approach-category speed limit (zero disables) | 80 m, 0 |
| `soft_penalty_weight` | weight of the soft-residual sum against the objective | 10 |

### `optimizer.solver`: how the search is run

`method`, `max_iterations`, `population_size` (multiplier on the sixteen
variables), `tolerance`, `seed`, `workers`, `seed_near_initial_design` and
`seed_perturbation_fraction`, and for the SQP driver `finite_difference_step`
(fraction of each bound range) and `constraint_tolerance` (normalised). The
`strategy` field is the differential-evolution mutation scheme.

### `optimizer.plausibility`: the model's validity domain

Fourteen fields: six two-sided windows, one ordering requirement and an
`enabled` switch. The windows are dimensionless except the two twist bounds
in degrees, and bound wing aspect ratio, fuselage fineness (length over
equivalent diameter), horizontal-tail arm as a fraction of fuselage length,
tip-to-root chord ratio, root thickness-to-chord ratio, and built geometric
washout (tip section incidence less root section incidence, negative for
washout). The ordering requirement keeps the trailing-edge break chord
between the tip and root chords.

These are statements about where this program's own mass, drag and
stability correlations were fitted, not performance requirements, and each
window is deliberately wider than every registered aircraft. They reach the
search as named Geometry residuals (`aspect_ratio_min/max` and the rest) and
follow the geometry family's configured policy. The group is edited in
Advanced Settings > Optimizer > Model validity domain, and is written to a
saved document only when it differs from the shipped defaults, so an older
file loads with those defaults rather than with zeros.

### `optimizer.relaxation`: controlled constraint relaxation (D01-D03)

`enabled` and `allowed_violated_groups` are on the Inputs page under Run
options and in Advanced Settings; `eligible`, the per-limit list, is
document-only. Violated discipline *groups* are counted rather than limits
(D01), a limit must be on the eligibility list and missed inside its own
declared tolerance (D02), and a relaxed design never ranks ahead of, or is
labelled as, a fully feasible one (D03).

**No limit is currently eligible.** `alas_config::optimizer::policy_review`
records the D02 review as one determination per residual identifier, with
the reason: a limit is `NeverRelaxable` (a failed or incomplete evaluation,
or a boolean availability flag), or `Ineligible` because no traceable
primary engineering or regulatory source states a fraction of it that may be
exceeded and this program has no measured error band for the quantity
either, or `Eligible` with a sourced tolerance ceiling. The third state has
no entries. A configuration that lists an ineligible or unknown identifier
is a blocking validation error quoting the recorded reason, so the shipped
run is strict and stays strict.

### `optimizer.weights`: replay table

Only `failure_cost` (the cost of a candidate that cannot be built, trimmed
or sized) and the tail-volume window (`min/max_hstab_volume_coef`,
`min/max_vstab_volume_coef`, a soft plausibility band) are read. Every
other weight belongs to the frozen reference objective.

### `requirements`: the brief

`mtow_kg` (ceiling of the sized takeoff mass and the mass the first trim is
made at), `cruise_mach` and `cruise_altitude_m` (the cruise point of the
polar and of the Breguet leg), `num_passengers` and `passenger_mass_kg` or
`cargo_payload_kg` with `aircraft_type` (the payload), `max_cruise_cl`
(stall guard of the trim), `max_wing_area_m2` and `min_wing_loading_kg_m2`
(geometry family), `ultimate_load_factor` and `dive_speed_m_s` (structural
mass), `min_physical_static_margin` and `cg_range_pct_mac` (balance
family), `optimize_passenger_capacity` (whether the candidate cabin sets the
passenger count), `gravity_m_s2`.

### Aerodromes and route

`departure_airport` and `arrival_airport` resolve elevation and ISA
deviation (takeoff and landing density ratio), TODA and LDA (field-length
residuals), the holding altitude datum, and the great-circle range when
`design_range_nmi` is zero. A route that does not resolve makes the
performance family report `airport_unavailable`.

### `fuel_policy` and `fuel_tanks`

The reserve scheme (`scheme`: EASA basic, 14 CFR 121.639 or 121.645, study
convention, trip only), `taxi_time_min`, `contingency_trip_fraction` and
`contingency_minimum_hold_min`, `alternate_distance_nmi`,
`final_reserve_hold_min`, `holding_altitude_ft`, `additional_fuel_kg`,
`extra_fuel_kg`, `unusable_fuel_fraction` and `expansion_space_fraction`.
The declared tank cells give the usable capacity the `fuel_capacity`
residual is checked against; on a redesigned wing the cells scale with the
candidate spar box. `mass_model.fuel_density_kg_m3` converts volumes.

### Propulsion

`geometry.engine`: the turbofan spec (rated thrust, cruise TSFC, takeoff
fuel flow) or the turboprop spec (shaft powers, cruise fuel flow, propeller
diameter), the engine count and spanwise positions, and the nacelle profile.
These set the Breguet consumption, the installed thrust of the performance
family and the propulsion mass.

### Mass model

`mass_model`: the structural, propulsion and systems methods (frozen
fractions or FLOPS, with `flops_transport` and `flops_structure` inputs when
FLOPS is selected), the Torenbeek and fraction parameters,
`mlw_fraction_mtow` (landing-mass residual), gear stations and load
fractions (balance family), `fuel_tank_usable_fraction`. `structures`
supplies the wingbox centroid the CG is built on; `control_surfaces` the
flap area in the wing mass; `cabin` the payload layout; `landing_gear` the
gear architecture.

### Performance constants

`performance`: `cl_max_to`, `cl_max_land`, `k_land`, `thrust_lapse`,
`oei_gradient` (overridden by the CS-25.121 value for the engine count),
`oei_climb_cl`, `oei_climb_delta_cd`, and the approach-speed factors used
when `max_approach_speed_kt` is set.

### Analysis fidelity

`analysis` (in-loop VLM resolution and probe angles) and `drag_model`
(parasite build-up factors) decide the polar every evaluation is trimmed
on; the winner is re-run at the fine resolution.

## What a run cannot do yet

Thrust, wing area and aspect ratio are not first-class design variables;
they follow from the sixteen geometry variables and the preset engine. Only
one sizing mission is flown per candidate (no simultaneous maximum-range or
short-field mission), and no optimization study has been run to convergence
on a preset with the mission-sized objective and reported as verified.
