# How the design optimization is driven, and what it reads

This is the operational answer to "what happens when I press Run with
optimization enabled, and which inputs decide the result". The methods are
in `docs/methods.md`; this page is the wiring and the input list.

## The driving path

1. **Entry.** The GUI Run page and the headless CLI (`alas --config <yaml>
   [--optimization-solver vlm|avl|both] [--seed <n>] [--no-optimize]`) both
   build `PipelineOptions` and call the same `DesignPipeline`. `optimize = true` is the default.
2. **Stage 2, design-space optimization.** `run_solver_optimizations` runs
   the requested branches (native VLM, external AVL, or both in parallel).
   Each branch constructs `DesignOptimizer::new(config)` and calls
   `run(bounds, Some(preset design vector))`. The bounds are the sixteen
   design-variable bounds of `alas_config::design_variables::SPECS`. With no
   bounds, `DesignOptimizer::resolved_bounds` derives the search box from the
   configured design space and the nominal design (the preset's design vector
   when one is loaded), so the desktop and the command line search the same
   envelope.
3. **The optimization method.** `optimizer.solver.method` is always
   `differential_evolution`: the mission-sized objective, a budgeted screening
   stage, a diverse elite and a budgeted refinement (current-to-pbest/1/bin
   under Deb's rules with an epsilon level) with bounded feasibility
   restoration. Retired method tokens (`scipy_legacy`, `sqp`, `nsga2`,
   `turbo_1`, `cma_es`, `feasibility_first_de`) migrate to it when a saved file
   is loaded, with a load note; the retired `optimizer.solver.strategy` and
   the penalty-table keys of `optimizer.weights` are dropped.
4. **One evaluation.** The evaluation builds geometry and payload load case,
   runs the mass analysis, cruise trim and drag polar, then closes mission fuel
   and takeoff mass through `mdo::mda`; its residual table and selected
   mission objective form the search cost.
5. **Result.** The best design is re-analysed at full fidelity
   (`FullAnalysis`) and becomes the optimized report; the history feeds the
   convergence figure and the run manifest. The AVL branch scores the same
   objective around AVL's induced drag (`assess_candidate_with_polar_cancellable`).

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
| `aerodrome_reference_code`, `max_approach_speed_kt` | ICAO Annex 14 code letter capping the clean-sheet wingspan (A 15 m ... F 80 m, strictly below the band edge; a registered aircraft in reference adaptation uses its own letter), approach-category speed limit (zero disables) | F, 0 |
| `soft_penalty_weight` | weight of the soft-residual sum against the objective | 10 |

### `optimizer.solver`: how the search is run

`method` is `differential_evolution`, the only method.
`screening` and `refinement` each hold `max_evaluations`, `time_limit_s` (at
most 300 s; 30 s and 120 s by default) and an optional `replay_evaluations`.
`max_evaluations` is a ceiling (20 000 by default) that the time limits reach
first: at the measured 0.3 s to 1.5 s of lane time per analysis, about 8
analyses per second on a 32-thread machine, even 300 s affords about 2 400.
In time mode the refinement plans its budget from its measured throughput:
`B = min(max_evaluations, floor(r (1 - 0.2) T eta) + 10)`, at least 240, with
`T` the refinement time limit (a fifth kept for verification) and `eta = 0.9`
an engineering safety factor. The refinement model first analyses the
baseline and the first 11 elite members, the seeds of the smallest (24)
initial population and so of every one the plan can choose; the set does not
depend on the plan, so a replay analyses it too. `r` is the workers times the
screening's lane utilization over their mean lane time. With the screening's
own model they are cache hits and `r` is the screening's analyses per second.
Measured (A320-200, 32 threads): the screening model's random samples cost
0.95 s of lane time each, refinement candidates 2.4 s, so the screening rate
alone would plan twice what the time affords. `B` sets the initial population
and the population reduction, and the time limit stays the backstop. Without
a time limit (`stop_on_evaluations_only`) `B = max_evaluations`, and a replay
uses its recorded `replay_planned_evaluations`. The elite is sized for the
initial population of `max_evaluations`, which a replay keeps.
Screening evaluates seeded Latin-hypercube batches of 64 plus the baseline;
its elite (the best half by the feasibility rule, the rest by max-min distance,
`(6 D - 1) / 2` members) seeds the refinement, whose initial population is
`clamp(B / 10, 24, 6 D)` for refinement budget `B`, the baseline and the
first `(N_init - 1) / 2` elite members included, and shrinks linearly in
evaluations to 8. A
screening batch is refilled past the design-vector pre-gate: it draws
points until 64 of them pass, so it keeps the workers busy. Every stage
budget counts analysed candidates only: pre-gate rejects cost microseconds,
are counted per failed check, and have their own cap `max_pregate_rejects`
(20 times the analysed budget by default); reaching it stops the stage with
`pregate_exhausted`. The screening sampler maps the root chord into the range
the chord order and the exposed trailing-edge angle admit, so most draws
pass the pre-gate. When a whole batch would overrun the screening time
limit, the last batch carries only the whole waves of one analysis per
worker that fit in the time left, projected from the previous batch, so the
stage ends within one batch time of its limit. The evaluated points are a
prefix of the seeded sequence whatever the partition, so they depend only on
the seed and the recorded count. A refinement trial is not redrawn: a generation of
at most `N_init` trials, not the rejects, bounds its lanes, and redrawing
spent the budget on rejects. Each stage reports its lane wall time per
analysed candidate and its lane utilization. The refinement keeps back
`min(10, B / 10)` evaluations and a fifth of its time
limit (an engineering estimate) for the reporting-fidelity work after the
search: up to eight finalist verifications, the baseline analysis and the
final analysis, so all of it runs inside the declared budget; the summary
reports the analyses used against that reserve.

The time limits are the default stopping rule. Each is checked only at
generation (batch) boundaries, the first right after the initial population,
so a generation in flight always finishes. Time-limited: the stopping point
depends on machine speed and worker count; replay with the recorded
counts for a bit-identical result at any worker count. The result, the
results card, the CLI summary, the report figure and the run manifest record
each stage's replay count (pre-gate-passed candidates, including repeats:
exact repeats and the reused elite and baseline count, but cost no
analysis), shown apart from its coupled analyses, its rejections, and the
refinement's planned budget. Setting `replay_evaluations` to the replay
count ignores that stage's time limit and stops at the first generation
boundary that reaches it; the refinement's `replay_planned_evaluations` is
set to the recorded planned budget, so its initial population and reduction
schedule are those of the recorded run. The coupled-analysis count does not
replay the run.
`stop_on_evaluations_only = true` ignores both time limits, so a seeded run
is bit-identical at any worker count however long it takes. The seed the run
used is recorded even when none was configured. `parameter_adaptation`
switches from static `F = 0.5`,
`CR = 0.9` to L-SHADE success history (recommended above 800 evaluations per
free variable). `tolerance` is the normalized spread below which a stagnated
refinement (best feasible objective improving less than 1e-4 over
`convergence_stagnation_generations` generations, or `ceil(2 N_init / 8)`
if longer, i.e. two initial populations of trials at the final size) reports
`converged` rather than `stagnated`. Stagnation never stops a run before half
its budget `B` is spent (an engineering choice: without it A320-200, seed
20260922, stopped after 215 of 590 evaluations, 1.6 % worse than the same
seed run to its budget). `workers = 0` uses every thread; it changes what a stage
evaluates only through a time-limit stop. A saved
`max_iterations`/`population_size` pair loads as a refinement budget of
`(max_iterations + 1) x population_size x 16` evaluations with a load note;
the retired `display_progress`, `seed_near_initial_design`,
`seed_perturbation_fraction`, `optimizer.objective.max_span_m` and weight
keys are dropped with a load note. `finite_difference_step` and
`constraint_tolerance` remain only for saved-file compatibility with the
retired SQP driver.

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
records the relaxation review as one determination per residual identifier, with
the reason: a limit is `NeverRelaxable` (a failed or incomplete evaluation,
or a boolean availability flag), or `Ineligible` because no traceable
primary engineering or regulatory source states a fraction of it that may be
exceeded and this program has no measured error band for the quantity
either, or `Eligible` with a sourced tolerance ceiling. The third state has
no entries. A configuration that lists an ineligible or unknown identifier
is a blocking validation error quoting the recorded reason, so the shipped
run is strict and stays strict.

### `optimizer.weights`: failure cost and planform thresholds

The search reads `failure_cost` for a candidate that cannot be built, trimmed
or analysed and the transport-planform thresholds; its objective and
requirement policies live
under `optimizer.objective` and `requirements`.

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
