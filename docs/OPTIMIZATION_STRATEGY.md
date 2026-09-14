# ALAS Optimization Strategy

**Status:** consolidated design, implementation, verification, and gap summary  
**Workspace snapshot:** 2026-09-11  
**Scope:** aircraft design-space optimization, mission sizing, aerodynamic-solver coupling, and the planned mixed-architecture/MDO workflow

## How to read this document

This is the single summary of the ALAS optimization strategy. It deliberately separates:

- **Current product behavior:** what the executable pipeline does in the present workspace.
- **Target strategy:** the architecture and evidence contract described by the MDO research/design baseline.
- **Verification status:** what has been tested, what is only numerically exercised, and what remains physically unvalidated.

The distinction matters. Several solver names remain available in configuration for compatibility, while the current product dispatch executes a single MADS search driver for every non-SQP product run. The intended solver portfolio and mixed-architecture funnel are broader than the current end-to-end wiring.

## Strategy at a glance

| Layer | Decision rule | Current implementation | Target contract |
|---|---|---|---|
| Requirements | Normalize and validate a requirements-first design brief before search | Configuration and requirement checks are used by the pipeline | Canonical, hashed `DesignBrief` is the authority |
| Architecture | Treat topology, counts, layouts, and technology choices as discrete decisions | One configured aircraft/engine/deck is optimized; no general outer catalogue driver | Enumerate a supported `ArchitectureKey`; never round categorical values from a continuous vector |
| Continuous design | Search a bounded, normalized, semantically ordered vector | Sixteen registered coordinates; design modes restrict the bounds | Keep only independent, active, auditable variables; preserve IDs, units, and bound sources |
| Candidate evaluation | Build geometry, close mass/fuel/CG/aero coupling, then compute typed residuals and objectives | Mission-sized `DesignObjective` is authoritative for product search | Typed stage records, provenance, fidelity, scenarios, and cacheable artifacts |
| Feasibility | Physical constraint misses outrank objective value; solver/tool failures are not physical feasibility | MADS uses feasible status, aggregate hard violation, then scalar cost | Lexicographic comparability, completion, hard/robust feasibility, residual severity, soft targets, then objectives |
| Search | Use the method suited to topology, smoothness, objective count, and evaluation cost | MADS is the product default path; SQP is the only alternate product branch | Architecture enumeration plus LHS, DE/CMA-ES/NSGA-II, validated surrogates, and fixed-architecture gradient refinement |
| Fidelity | Screen cheaply, promote selected candidates, and re-evaluate finalists | Native VLM and optional AVL branches; finalist is re-assessed at closed TOW | Explicit stage/fidelity plan, paired low/high evidence, robust/UQ promotion, and minimum fidelity per requirement |
| Evidence | Make runs reproducible and explainable | Seed, history, timings, rejection reasons, and run manifest are recorded | Complete candidate/stage keys, deterministic cache, convergence evidence, UQ statistics, and traceability |

The operating sequence is therefore:

> **Requirements → supported architecture → canonical continuous vector → deterministic screening → mission-sized evaluation → typed feasibility/objective record → constrained search → fidelity promotion → final closed-TOW re-analysis → report with margins and evidence.**

## 1. Problem formulation and authority boundaries

### 1.1 What is being optimized

The product problem is a bounded black-box aircraft-design search. A candidate design changes geometry-related quantities such as wing planform, fuselage length, tail scale, longitudinal placement, airfoil scales, and local airfoil shaping. Those changes propagate through geometry, payload accommodation, mass properties, trim/aerodynamics, propulsion, fuel/tank capacity, mission dispatch, and constraint residuals.

The product objective is **mission-sized** rather than a free-standing lift-to-drag proxy. The default objective is `block_fuel`. Other supported product objective kinds are:

- `takeoff_mass`;
- `operating_empty_mass`;
- `fuel_per_seat_kilometre`.

The old weighted lift-to-drag objective is retained only by `DesignObjective::new_reference_compatibility` for legacy parity fixtures. It is not product-selectable, and saved configuration using the removed legacy objective is rejected.

### 1.2 Authority hierarchy

The design authority should be interpreted in this order:

1. **Requirements and configuration** define the requested aircraft, mission, policies, limits, units, and solver budget.
2. **Architecture selection** defines discrete topology and equipment choices. In the target workflow this is an explicit `ArchitectureKey`, not a rounded coordinate.
3. **The canonical design snapshot** freezes the exact requirements, architecture, vector, bounds, options, stage plan, and evaluator/toolchain identity used for one evaluation.
4. **Derived geometry, meshes, decks, reports, and external-tool inputs** are artifacts produced from that snapshot. They do not become independent sources of design intent.
5. **CPACS or other interchange files** are interoperability artifacts. They must preserve IDs, units, reference frames, schema/version, status, and source hashes, but do not override the canonical snapshot.

Every optimization result should be traceable back to its requirements ID, architecture ID, vector coordinate ID, evaluator version, numerical tolerances, solver seed, and external-tool provenance.

### 1.3 Canonicalization before evaluation

Before a candidate enters the expensive loop, ALAS should:

- validate finite values, bounds, dimensions, units, and requirements;
- materialize or select the requested architecture;
- load the candidate payload/load case;
- resolve derived variables such as passenger-cabin-driven fuselage length;
- canonicalize the final vector and calculate stable hashes;
- freeze the options and fidelity/scenario plan.

The target complete evaluation key is conceptually:

```text
CandidateKey =
  requirements_sha256
  + architecture_id
  + canonical_vector_hash
  + fidelity_plan_id
  + scenario_plan_id
  + evaluator_version
  + toolchain_identity
```

The same candidate is only comparable when the relevant requirements, architecture, fidelity, scenarios, evaluator, and required stages are compatible.

## 2. Design modes and continuous search space

### 2.1 The current 16-coordinate vector

`DesignVector`, its coordinate order, bounds, units, descriptions, and preset guardrails are generated from one registry. This avoids manually maintaining separate array, bound, and metadata orderings.

| # | Semantic ID | Nominal | Global bounds | Unit |
|---:|---|---:|---:|---|
| 1 | `span_m` | 71.75 | 60–80 | m |
| 2 | `root_chord_m` | 16.50 | 12–19 | m |
| 3 | `break_chord_m` | 7.80 | 6–10 | m |
| 4 | `tip_chord_m` | 1.60 | 1–3 | m |
| 5 | `sweep_deg` | 34.0 | 25–45 | deg |
| 6 | `tip_twist_deg` | 0.0 | −5–1 | deg |
| 7 | `wing_x_shift_m` | 0.0 | −5–8 | m |
| 8 | `tail_scale` | 1.0 | 0.75–1.25 | dimensionless |
| 9 | `fuselage_length_m` | 76.72 | 65–85 | m |
| 10 | `tail_x_shift_m` | 0.0 | −2–3 | m |
| 11 | `airfoil_thickness_scale` | 1.0 | 0.8–1.3 | dimensionless |
| 12 | `airfoil_camber_scale` | 1.0 | 0.7–1.4 | dimensionless |
| 13 | `bump_upper_front` | 0.0 | −0.005–0.002 | dimensionless |
| 14 | `bump_upper_rear` | 0.0 | −0.005–0.002 | dimensionless |
| 15 | `bump_lower_mid` | 0.0 | −0.005–0.003 | dimensionless |
| 16 | `bump_lower_rear` | 0.0 | −0.005–0.003 | dimensionless |

The search maps each free coordinate to a normalized unit-box coordinate for solver work:

```text
u_i = (x_i - lower_i) / (upper_i - lower_i),     0 <= u_i <= 1
```

The native quantities remain authoritative for geometry and reporting. Angles are in degrees at the configuration boundary; mass is in kg; length is in m; range is converted to SI internally; CG and static margin are reported in their configured engineering conventions, including `% MAC` where applicable.

### 2.2 Derived, fixed, and configuration-controlled quantities

Not every influential quantity is a free continuous coordinate. In the current product model:

- engine family, engine count, engine positions, propulsion deck, and deck selection come from configuration;
- thrust is not an optimization freedom; the deprecated `engine_scale_enabled` field does not activate thrust scaling;
- architecture, deck count, passenger/container layout, landing-gear topology, fuel/energy system, and other categorical choices are not created by rounding a real-valued coordinate;
- wing area and aspect ratio are not first-class independent coordinates. They are consequences of planform variables, which can make identifiability and constraint activity difficult to interpret;
- in clean-sheet passenger mode, `fuselage_length_m` is normally derived from the requested cabin/payload using a bounded sizing solve. Cargo mode does not use this passenger-cabin derivation;
- in baseline-sandbox mode, the nominal vector is the only allowed point.

Derived or duplicate variables should be removed from future search vectors. A variable that is insensitive, fully fixed, or numerically confounded with another coordinate should be frozen or represented as an equivalence class rather than counted as independent design freedom.

### 2.3 Design modes

| Mode | Purpose | Search envelope |
|---|---|---|
| `CleanSheet` | Product design exploration | All registered coordinates use global bounds, subject to requirements and derived-variable rules |
| `ReferenceAdaptation` | Controlled adaptation around a registered reference aircraft | Fixed variables are held at nominal values; remaining coordinates use explicit narrow windows clipped to preset guardrails |
| `BaselineSandbox` | Baseline/review/parity behavior | All coordinates are fixed at nominal; no optimization freedom |

Reference-adaptation windows are intentionally conservative: approximately 10% for lengths/scales, ±3 degrees for angles, ±1 m for shifts, and small bounded airfoil-bump changes. Fixed variable names must resolve against the registry, and all windows must be finite and ordered.

### 2.4 Discrete architecture strategy

The target mixed-architecture workflow places a small, supported catalogue outside the continuous optimizer. An `ArchitectureKey` may include:

- passenger, freighter, or mixed role;
- deck count;
- propulsion family and count;
- cabin/container layout;
- wing configuration;
- landing-gear layout;
- fuel or energy system;
- other explicitly supported topology/layout choices.

For each architecture, the continuous vector is rebuilt with stable semantic IDs and architecture-specific bounds. The optimizer never invents unsupported topology through rounding. The outer driver preserves every rejected architecture and seed with a typed reason, so an empty feasible set is diagnosable rather than silently discarded.

## 3. End-to-end candidate evaluation

### 3.1 Product evaluation flow

The current product path is a mission-sized `DesignObjective` used by the shared GUI/CLI `DesignPipeline`:

```text
requirements/config
  -> canonical candidate and payload load case
  -> geometry/materialization and accommodation
  -> preliminary mass, structural inventory, CG, and capacity
  -> native trim/drag polar or validated external AVL polar
  -> propulsion and airport/route resolution
  -> fuel/dispatch calculation
  -> mass/fuel/CG fixed-point closure
  -> typed residual assessment and scalar cost
  -> constrained search history
  -> finalist re-assessment and full analysis at closed TOW
```

The research target expresses the same dependencies as typed stages with snapshots, values, statuses, residuals, provenance, and fidelity. The current Rust implementation has the coupling and assessment data, but does not yet expose the complete general stage-record/cache contract end to end.

### 3.2 Geometry and payload accommodation

For each candidate, ALAS builds geometry with the selected engines included and checks physical reference area and chord. Payload is then laid out for the configured passenger/cargo load case.

For a clean-sheet passenger design, cabin-driven fuselage sizing uses a bounded bisection solve, with a short scan fallback when discrete seat packing is non-monotone. If the upper allowable fuselage length cannot accommodate the requested payload, the candidate receives a typed design-space or payload-layout failure. Cargo layouts bypass this passenger-cabin derivation.

Geometry residuals cover, as configured:

- span and wing-area limits;
- wing-loading limits;
- body-alpha plausibility windows;
- horizontal/vertical tail-volume windows;
- passenger or cargo shortfall;
- approach-speed and runway-related geometry/performance consequences.

Tail-volume limits are treated as plausibility soft preferences in the current product assessment even where the broader family policy is hard. This prevents a geometrically plausible candidate from being rejected solely for a design preference while retaining the residual and penalty.

### 3.3 Mass, structure, and balance

The evaluator performs a preliminary two-pass mass analysis with payload layout and structural feedback. Supported mass options include frozen fractions and selectable transport-style FLOPS structural/propulsion/systems groups, with applicability checks and typed missing-datum failures.

The mass ledger distinguishes at least:

- operating empty mass and its structural, propulsion, systems, controls, cabin, and landing-gear groups;
- payload, zero-fuel mass, ramp/takeoff mass, and landing mass;
- taxi, trip, contingency, alternate/final reserve, additional, extra, and unusable fuel;
- tank volume/capacity and fuel density conversion;
- structural inventory status and any unverified inventory.

Balance checks are evaluated across relevant mass states, including forward/aft CG and static-margin constraints, nose/main gear strength, and minimum nose-gear load. The evaluator retains actual CG, neutral point, MAC, masses, and provenance for the state used in feasibility assessment.

### 3.4 Aerodynamics and propulsion

The native path computes a cruise trim/drag polar at the candidate condition, including candidate CG and MTOW context. Propulsion is bound to the configured engine/deck and is used for thrust-to-weight, cruise-thrust-margin, engine-out, climb, and mission calculations.

The external AVL path is accepted only when its polar is physically usable and comparable to the candidate:

- finite, physically valid coefficients and neutral-point data;
- a bracketed interpolation range at the required lift coefficient;
- Mach agreement to the configured tight tolerance;
- altitude agreement to the configured tolerance;
- reference-area agreement to the configured relative tolerance;
- source/deck provenance retained in the assessment.

An unavailable or out-of-range AVL polar is a typed tool/data failure, not evidence that the aircraft is physically feasible or infeasible.

### 3.5 Mission and fuel policy

The mission is built from resolved route/airport data, cruise condition, payload, propulsion, polar, and fuel policy. Airport elevation, ISA deviation, runway lengths, holding altitude, and route range are part of the candidate context. An unresolved airport or missing required runway datum becomes a typed residual/failure according to policy.

The supported fuel-policy families include EASA basic, FAA 121.639/121.645, study convention, and trip-only. Policy components are explicit rather than hidden in a single multiplier:

```text
block / dispatch fuel
  = taxi
  + trip
  + contingency
  + alternate and final reserve
  + additional
  + extra
  + unusable / expansion treatment as configured
```

Tank capacity is derived from explicit tank cells and candidate wing/spar-box geometry. The redesigned wing changes the available cells, and fuel density converts tank volume to fuel mass. Tank capacity, reserve policy, and maximum fuel are constraint inputs, not post-processing decoration.

Current mission limitations are material: there is one sizing mission per candidate; no wind/CAS–Mach leg schedule or optimum-altitude search is implemented; trim drag is not fully re-solved by fuel state; and phase-specific high-lift/aero models are not yet complete.

### 3.6 Mass/fuel/CG fixed-point closure

For mission sizing, the core closure is:

```text
TOM = ZFM + required_takeoff_fuel(TOM)
```

The current Gauss–Seidel-style MDA loop is:

1. Start from the current takeoff mass and compute mass properties, CG, trim, and polar.
2. Recompute mass and payload layout at the updated mass.
3. For native aerodynamics, refresh trim/polar as the closed-mass state changes; an external AVL polar is held fixed for the closure and is marked accordingly. The configured CG tolerance is retained as a policy/reporting threshold, while the current native loop refreshes the polar on each closed-mass pass.
4. Recompute policy fuel and dispatch under MTOW, MLW, tank-capacity, range, and reserve constraints.
5. Apply a bounded Aitken delta-squared acceleration where admissible.
6. Stop when takeoff-mass change is below the sizing tolerance and dispatch has converged, or return a typed `sizing_not_closed` status at the pass limit.

The default sizing budget is 30 passes with a 1 kg takeoff-mass tolerance. `fixed_requirement` mode performs one sizing pass rather than solving the mission closure. Closure is an inner physical coupling problem, not an equality constraint handed to an outer population optimizer; this avoids treating a measure-zero convergence condition as a population feasibility rule.

## 4. Objectives, residuals, and acceptance

### 4.1 Scalar objective and normalization

Let (J_mathrm{raw}) be the selected mission quantity. The current normalized objective is:

```text
J_norm = J_raw / scale(kind)
```

The documented scales are:

| Objective kind | Raw quantity | Scale |
|---|---|---|
| `block_fuel` | policy block/dispatch fuel | `0.3 * MTOW ceiling` |
| `takeoff_mass` | sized takeoff mass | `MTOW ceiling` |
| `operating_empty_mass` | OEW | `MTOW ceiling` |
| `fuel_per_seat_kilometre` | fuel divided by seats actually carried and distance | `1e-3 kg/(seat km)` |

Soft constraints contribute a normalized penalty:

```text
cost = J_norm + soft_penalty_weight * soft_residual_sum
```

The current cost function also adds `1 + hard_residual_sum` for hard-infeasible candidates. The search comparator still treats hard feasibility and hard violation severity as higher-priority information than this scalar cost.

The configured soft penalty weight defaults to 10. `failure_cost` and tail-volume windows are the product-relevant fields in `optimizer.weights`; other legacy weight fields are retained as frozen reference-replay data rather than silently changing product search behavior.

### 4.2 Residual convention

Every requirement should be represented as a signed residual with a consistent direction:

```text
raw_residual > 0  -> violation
raw_residual <= 0 -> pass
```

The normalized violation is non-negative and dimensionless. For a limit (L), the current convention is approximately:

```text
violation = max(raw_residual / abs(L) - 1e-5 slack, 0)
```

The complete target record stores actual value, target, direction, unit, scale, policy, requirement ID, load case, scenario, fidelity, and source/provenance. Equality, lower-bound, upper-bound, and range constraints must be normalized explicitly rather than relying on a solver-specific sign convention.

### 4.3 Constraint policies

| Policy | Search meaning | Reporting meaning |
|---|---|---|
| `hard` | Candidate is physically infeasible when violated | Must be shown with residual, actual, limit, and dominant reason |
| `soft` | Candidate remains eligible but pays a normalized penalty | Shows a preference or plausibility miss |
| `diagnostic` | Does not affect acceptance or cost | Evidence only; useful for unavailable/unsupported checks |
| `off` | Not evaluated for the decision | Must not be presented as passed |

The default family policy is hard for mass, balance, performance, and geometry, with explicit exceptions such as the current tail-volume plausibility treatment. Configured options also include MTOW sizing mode, design range, maximum span, optional approach-speed limit, and a soft-penalty weight.

### 4.4 Current residual families

| Family | Representative checks |
|---|---|
| Mass | Structural inventory status, fuel capacity, MTOW, MLW/landing mass, dispatch mass/tank limits, mission closure |
| Balance | Static-margin floor, forward/aft CG ranges, gear strength, minimum nose-gear load, CG-model status |
| Performance | Engine-out/second-segment evidence, cruise thrust margin, route/airport resolution, mission range, takeoff/landing field, approach speed when configured |
| Geometry | Span, wing area, wing loading, body-alpha plausibility, tail volume, passenger/cargo shortfall |

A requirement that cannot be evaluated because a tool, datum, bracket, or model is unavailable must retain that status. It is not automatically converted to a passing value or an arbitrary large objective cost.

### 4.5 Ranking semantics

The target lexicographic ranking is:

```text
(comparable and required stages complete, descending)
(hard feasible, descending)
(robust hard feasible when robust mode is required, descending)
(evaluation failure count, ascending)
(hard violation severity, ascending)
(normalized hard residual, ascending)
(hard violation count, ascending)
(normalized soft residual, ascending)
(ordinary objective tuple, ascending)
(architecture ID, ascending)
(canonical vector hash, ascending)
```

This guarantees that a hard-feasible candidate is not beaten by a cheaper but physically infeasible candidate. It also separates a physical requirement miss from a solver failure, tool-unavailable result, timeout, malformed output, or numerical failure.

The current product MADS comparator is a narrower realization: it compares feasible status, then aggregate normalized hard violation, then scalar cost. Because scalar cost includes the soft penalty, soft preferences affect cost, but they are not yet represented as a separate explicit lexicographic key with comparability/completion/robustness. This is an important implementation gap, not an intended reason to change physical feasibility semantics.

### 4.6 Multiobjective policy

True multiobjective operation should use constrained dominance:

- a feasible candidate dominates an infeasible candidate;
- among infeasible candidates, lower aggregate normalized violation dominates;
- among feasible candidates, Pareto dominance applies to the declared objective tuple;
- crowding/diversity preserves the front.

Cross-architecture fronts should only be combined when objectives, requirements, fidelity, scenarios, and completion status are comparable. Otherwise report per-architecture fronts and apply an explicit decision policy. The current compact `ParetoCandidate` is not yet a complete residual/UQ/provenance record.

## 5. Search algorithms and actual dispatch

### 5.1 The current product dispatch

The solver setting accepts these method names: `differential_evolution`, `feasibility_first_de`, `nsga2`, `turbo_1`, `cma_es`, and `sqp`. The executable behavior is currently:

| Configured method | Current product execution | Interpretation |
|---|---|---|
| Any of `differential_evolution`, `feasibility_first_de`, `nsga2`, `turbo_1`, `cma_es` | Product `run_product_search` dispatches to one MADS driver | Method name is loadable for compatibility, but does not select the corresponding product kernel |
| `sqp` | Product SQP branch | Bound-constrained, fixed-architecture local refinement with explicit hard residual constraints |
| `new_reference_compatibility` constructor | Legacy native differential evolution | Compatibility/parity path only; not the product objective path |

The run manifest records both `configured_method` and `executed_method`. A product run should therefore be interpreted from the manifest, not from the requested method string alone. The operational guide and help text describe the intended method catalogue; the source dispatch and manifest describe what actually ran.

### 5.2 Current MADS product driver

The product MADS driver is a bounded, normalized, derivative-free search with a progressive barrier:

- variables are mapped to the unit box and fixed coordinates are removed from the free poll set;
- initial points include deterministic Latin-hypercube samples snapped to the current mesh;
- poll directions use deterministic positive-spanning/Halton-like constructions;
- a feasible point is always retained and ranked by the current scalar comparator;
- infeasible points can improve the progressive hard-violation threshold;
- non-finite, failed, or extreme-barrier evaluations do not become physically feasible;
- the mesh contracts geometrically until the minimum mesh, evaluation budget, or iteration limit is reached.

The current product settings derive an evaluation budget from population size, dimension, and generation/iteration settings. With the default 16-coordinate vector, the configured population multiplier of 6 corresponds to a nominal population scale of 96 for the legacy calculation. Product MADS does not currently use the worker-batching trait; its callback is serial. MADS is robust to non-smooth black-box behavior but does not provide a global-optimality or aircraft-convergence theorem under finite budget, noisy analyses, or hidden failures.

### 5.3 Current SQP branch

SQP is the only configured alternate that changes the product driver. It is appropriate only after the architecture and active variable set are fixed and the coupled evaluation is locally smooth enough.

Current mechanics:

1. Normalize free variables to the unit box.
2. Evaluate the mission-sized objective and hard residuals.
3. Remove the hard-infeasibility cost offset for the SQP objective; pass each hard residual as an explicit inequality with `signed_normalized() <= 0`.
4. Compute forward-difference objective and constraint gradients in a parallel batch, retrying a failed probe in the opposite direction.
5. Solve an elastic dense QP with slack/L1 penalty, update a damped BFGS Hessian, and perform an L1-merit/Armijo line search with bounded backtracking.
6. Retain the best valid point and terminate on step, objective, iteration, subproblem, line-search, fixed-variable, or invalid-initial-point conditions.

The current implementation zeros a finite-difference column when both directional probes fail. This can create false convergence; the audit test `sqp_reports_convergence_after_failed_probes` records that risk. A production-grade implementation should report probe reliability and avoid declaring convergence solely from failed sensitivities.

### 5.4 Legacy differential-evolution compatibility path

The reference-compatibility path preserves SciPy-style behavior for parity fixtures:

- LHS initialization or a jittered near-initial population with row 0 equal to the initial design;
- PCG64-based initialization and generation RNG streams;
- default `best1bin` mutation/crossover behavior with generation dithering (F in [0.5,1)) and crossover rate 0.7;
- supported `best1`, `rand1`, `best2`, `rand2`, `rand-to-best1`, and `current-to-best1` bin/exp variants;
- out-of-bounds resampling and a standard population-spread convergence criterion;
- native worker batching with stable input-order merge where this compatibility path is used.

This path is useful for regression comparison. It must not be mistaken for the current product mission-sized search.

### 5.5 Intended search portfolio

After the outer architecture choice, the target portfolio is:

| Situation | Recommended method | Required safeguards |
|---|---|---|
| Coverage and seed survival | Deterministic LHS plus baseline/requirement seeds | Report strata, dimensions, seed, coverage, feasible count, and typed rejection groups |
| Non-smooth or multimodal single objective | Feasibility-first DE | Separate hard residual ranking from objective; fixed seed; bound and failure accounting |
| Fixed-architecture covariance adaptation | Bounded CMA-ES | Normalize variables; preserve feasible incumbent; regularize covariance and record active bounds |
| Declared multiobjective problem | Constrained NSGA-II | Feasible-first dominance, per-architecture comparability, Pareto diversity, explicit decision policy |
| Expensive promoted fidelity | Validated trust-region/surrogate method | Separate models for feasibility, residuals, objectives, cost, and failure; holdout validation; real-evaluation confirmation |
| Smooth local final refinement | SQP or gradient/adjoint method | Fixed architecture; explicit constraints; reliable derivatives; no claims beyond local refinement |

No method guarantees a global optimum. Better algorithms cannot repair missing constraints, poor parameterization, unit/frame mismatches, nondeterminism, or an invalid fidelity model.

## 6. Initialization, DOE, scaling, and identifiability

### 6.1 Deterministic initialization

The baseline strategy is:

1. include the nominal/reference design as a marked seed;
2. add requirement-driven or boundary seeds where useful;
3. fill the normalized unit box with a deterministic LHS;
4. for a reference adaptation, optionally seed a jittered neighborhood while retaining the exact nominal row;
5. screen all seeds in a deterministic order before expensive optimization.

The current solver default enables near-initial seeding with a 5% perturbation fraction. A missing seed uses a runtime-generated seed, so reproducibility requires explicitly recording and reusing the run seed. `BaselineSandbox` is useful for comparison but is not a search.

### 6.2 Screening order

The target low-fidelity screen rejects candidates in a stable order:

1. requirements, units, bounds, and canonicalization;
2. geometry materialization, topology/intersections, explicit volume, and basic bounds;
3. payload accommodation and cabin/container layout;
4. Class-I/II mass, CG, fuel/energy capacity, and propulsion closure;
5. mission range, reserves, climb/ceiling proxies, and field-length proxies;
6. native low-fidelity aero/trim and stability when the candidate survives the cheap checks.

The reason, family, dominant residual, fidelity, and relevant artifacts are retained for every rejection. “Not evaluated” is not a pass.

### 6.3 Variable audit

Each variable should have:

- a stable semantic ID and canonical order;
- native unit and normalized mapping;
- lower/upper bounds and their source;
- fixed, derived, or active status;
- differentiability flag and variable group;
- sensitivity/identifiability evidence where the variable is used in a final claim.

Near the final design, inspect local Jacobian rank, singular values, parameter correlations, condition number, active bounds, and objective/residual sensitivity. A numerically optimal but unidentifiable solution is a weak region or equivalence class, not strong evidence of a unique optimum. Sensitivity analysis and identifiability analysis answer different questions and should not be conflated.

## 7. Mixed architecture and feasibility-first strategy

The target outer driver is explicit enumeration followed by per-architecture continuous optimization:

```text
DesignBrief
  -> supported ArchitectureKey catalogue
  -> deterministic low-fidelity screen
  -> seeded continuous population for each survivor
  -> feasibility-first local/global search
  -> promotion of comparable finalists
  -> per-architecture and, only when valid, global Pareto/decision set
```

The outer catalogue is intentionally small and supported. It should not create an apparent global optimum by comparing an under-evaluated architecture with a fully evaluated one. A topology/layout choice that fails accommodation, propulsion, or mission closure is retained as a typed rejected architecture.

For infeasible candidates, optimization proceeds on normalized physical violation rather than arbitrary penalty weights. Solver failures, unavailable external tools, timeouts, malformed output, and numerical failures remain separate statuses. This is essential when a cheap but failed evaluation would otherwise look better than a costly physical miss.

For multiobjective studies, report at least:

- feasible nondominated candidates;
- per-architecture fronts where cross-architecture comparability is absent;
- the decision policy used to select a point from the front;
- near-infeasible boundary candidates;
- seed survival and rejection groups;
- fidelity and uncertainty evidence for every promoted claim.

## 8. Fidelity funnel and external solver coupling

### 8.1 Target fidelity funnel

The intended funnel is:

```text
F0  requirements and architecture validation
F1  algebraic sizing, geometry, accommodation, basic closure
F2  Class-I/II mass, CG, payload, fuel, and mission proxies
F3  native low-fidelity aero, trim, stability, and field checks
F4  low-fidelity loads/wingbox/structural checks
F5  uncertainty, scenario, sensitivity, and robust screening
F6  external high-fidelity aerodynamics/structures and final confirmation
```

Promotion should select candidates that are comparable and required-stage complete, hard-feasible or best on a meaningful boundary, diverse in design space, calibration-useful, sensitivity-informative, or required for uncertainty quantification. A nominally feasible point is not reliable until the required robust/high-fidelity checks have been completed.

Raw low- and high-fidelity values must remain separate. Paired candidates should be used to estimate bias and scatter; a low-fidelity correction must not erase the underlying high-fidelity evidence. Each requirement has a minimum fidelity at which it is allowed to support a decision.

Low-fidelity results do not support claims about stall, viscous drag, junction flows, aeroelasticity, manufacturing feasibility, or certification unless the corresponding model and evidence exist.

### 8.2 Current VLM/AVL branches

The pipeline distinguishes aerodynamic-solver branches from search algorithms:

- `OptimizationSolverMode::Vlm` is the default optimization branch;
- `Avl` requires a configured native AVL executable/output directory;
- `Both` can run independent VLM and AVL branches, typically in separate `solvers/vlm` and `solvers/avl` output trees and optionally in parallel.

The VLM branch optimizes with the product `DesignOptimizer`, then re-assesses the best design for hard feasibility and runs full analysis at the closed/sized takeoff mass. The AVL branch wraps the product objective with an AVL polar evaluator, checks interpolation and candidate-condition compatibility, closes the mission around the accepted external polar, and reruns the final analysis/AVL case for comparability.

When both branches are requested, the selection logic prefers a completed VLM result and falls back to a completed AVL result only when VLM did not complete. An explicit AVL-only run does not silently fall back to VLM.

The current AVL cache is a local design-bit hash inside the branch evaluator. It is useful for repeated evaluations in that run, but it is not yet the complete requirements/architecture/fidelity/scenario/toolchain stage cache required by the target contract.

### 8.3 Structural and external-tool evidence

FLOPS-style groups have been numerically reproduced against two NASA Aviary validation cases to quoted precision. This verifies implementation/calculation parity for those cases; it is not physical validation of the complete ALAS aircraft model.

External structural and aerodynamic tools remain environment- and configuration-dependent. Missing licenses/executables, solver convergence problems, unpinned navigation data, and tool/model-form differences must be reported as provenance or typed unavailability, not hidden behind a penalty.

## 9. Uncertainty, robustness, and sensitivity

Robust optimization is a separate decision mode, not an implicit multiplier on the nominal objective. The scenario contract should state:

- aleatory variables such as weather, wind, payload, runway condition, and manufacturing scatter;
- epistemic uncertainty such as technology/regression parameters;
- model-form uncertainty for aerodynamics, structures, propulsion, weight, and discretization;
- numerical uncertainty from tolerances, meshes, interpolation, and floating-point behavior;
- distributions, bounds, correlations, labels, sample generator, sample count, random seed, and common-random-number policy;
- mean, standard deviation, quantiles, confidence intervals, tails, residuals, fidelity, and decision-grade status.

Typical decision forms are:

```text
chance constraint:      P(residual >= 0) >= p
robust scalar objective: E[J] + lambda * Risk(J)
```

Nominal mode records UQ as `NotRun`; robust mode requires the scenario/UQ stage. Nominal and robust rankings must not be mixed without an explicit policy. Sensitivity may use OAT/Morris, rank/correlation, Sobol/PCE, gradients/adjoints, or local perturbations/active-bound analysis according to evaluation cost and smoothness.

The current product does not provide a general robust/UQ/chance-constraint service. The research recommendation is to screen nominally, then apply robust hard-feasibility and UQ evidence during finalist promotion.

## 10. Reproducibility, caching, and parallel execution

### 10.1 Current controls

The solver configuration records method, strategy, iteration/evaluation budget, population multiplier, tolerance, seed, worker count, initialization policy, perturbation fraction, finite-difference step, and constraint tolerance. The run manifest records configured versus executed method, strategy, termination, evaluations, timings, and search metadata. Candidate histories use stable merge order where batched evaluations are available.

Current execution details are not uniform:

- product MADS evaluation is serial;
- SQP finite-difference probes are batched in parallel;
- VLM/AVL solver branches may run concurrently with separate output directories;
- legacy compatibility DE supports native worker batching;
- external delegated evaluators may be serial.

Parallelism must not alter candidate order, random streams, or accepted incumbents. Independent candidates need unique external workspace names. Nested parallelism must be bounded, and a worker-count equivalence check should demonstrate identical results for `workers = 1` and `workers > 1` before treating parallel speedup as trustworthy.

### 10.2 Target stage cache

Cache at stage granularity using a key composed of:

```text
requirements hash
+ architecture key
+ upstream artifact hashes
+ stage and fidelity IDs
+ scenario ID
+ evaluator/toolchain version
+ numerical tolerance
```

Cache records should include exact inputs, hashes, units, frames, outputs, status, residuals, tool logs, runtime, source/version, timestamp, and invalidation reason. Deterministic successes and physical failures can be reused. Transient failures should be retried or invalidated according to policy rather than treated as reusable physical labels.

The cache must never allow stale geometry, external-tool output, or nominal results to masquerade as a result for a changed requirement, architecture, fidelity, scenario, evaluator, or tolerance.

## 11. Outputs and evidence package

### 11.1 Search result

The current `OptimizationResult` contains the best design/cost/validity, history, wall time, executed method, strategy, termination, and an optional compact Pareto front. A no-feasible result retains rejection categories so the operator can distinguish geometry, payload, mass, trim, mission, or external-tool problems.

### 11.2 Finalist re-analysis

The selected finalist is not accepted solely because it was the best search score. The pipeline:

1. re-assesses the candidate with the mission-sized objective and hard residual table;
2. binds the report/export state to the same closed takeoff mass, CG, and residual state;
3. runs full analysis at the sized takeoff mass;
4. includes the final aerodynamic/structural provenance and any external-tool evidence;
5. writes history, timings, convergence information, and run-manifest fields.

This prevents the common failure mode where an optimizer state and a report state use different masses, CGs, polar conditions, or reserve assumptions.

### 11.3 Required convergence evidence

A defensible study should report:

- algorithm/version, configuration, seed, worker count, and toolchain;
- architecture and canonical vector identity;
- population, iteration, evaluation, and wall-clock budgets;
- total, unique, cached, failed, timed-out, and cancelled evaluations;
- feasible count by generation/iteration and first-feasible point;
- best feasible objective and best near-feasible violation;
- hard/soft residual severity and dominant rejection groups;
- population/objective spread and active bounds;
- Pareto hypervolume/epsilon when applicable;
- surrogate validation metrics when applicable;
- UQ sample count, quantiles, and confidence intervals when applicable;
- termination reason: budget, stagnation, mesh/spread, target, cancellation, or failure.

Population standard deviation alone is not convergence evidence for a coupled aircraft analysis.

## 12. Verification and validation status

### 12.1 Existing implementation evidence

The workspace handoff/status records the following checks for the documented snapshot:

- `cargo fmt --all -- --check` clean;
- `cargo xtask checks` passed;
- `cargo clippy --workspace --all-targets -- -D warnings` clean;
- the gate test run completed 204 suites with 2,305 passed, 0 failed, and 17 ignored;
- `cargo xtask gate` passed;
- `cargo deny --locked check` passed with two documented maintenance exceptions;
- FLOPS group calculations reproduce two NASA Aviary reference cases to quoted precision;
- SQP is verified on analytic constrained problems, a delegated bowl objective, and one native major iteration;
- product mission sizing, policy fuel, tank capacity, AVL assessment, and typed residual paths have targeted unit/integration coverage.

These are implementation and numerical-verification results from the recorded workspace snapshot. They are not a claim that a complete aircraft optimization study has converged or that the model has been physically validated.

### 12.2 Required verification matrix

The research baseline defines a broader V1–V23 plan. The important checks are:

1. canonical requirements, unit conversion, and reference-frame preservation;
2. architecture separation and rejection accounting;
3. vector order, bounds, normalization, fixed/derived identity, and round-trip serialization;
4. LHS strata, deterministic seeds, and baseline inclusion;
5. residual sign, scale, direction, policy, and aggregate-severity correctness;
6. hard/soft/diagnostic/off policy behavior and short-circuit evidence;
7. no-feasible and typed failure ranking;
8. constrained NSGA-II dominance and Pareto consistency;
9. method determinism, cache equivalence, and serial/parallel equivalence;
10. fidelity promotion, paired low/high comparison, and minimum-fidelity enforcement;
11. surrogate holdout validation and failure-prediction safety;
12. robust scenario reproducibility, chance constraints, quantiles, and confidence intervals;
13. sensitivity and identifiability diagnostics;
14. convergence evidence and end-to-end architecture-funnel traceability;
15. benchmark and external-tool provenance, including AGARD 445.6 where applicable.

### 12.3 Three kinds of evidence

Every conclusion should be labelled as one of:

- **Implementation verification:** code paths, type contracts, serialization, deterministic behavior, and tests.
- **Numerical verification/calibration:** comparison against analytic solutions, benchmark cases, or quoted reference calculations.
- **Physical validation:** comparison with flight data, trusted experiments, validated external data, or operational flight/OFP evidence.

The current project has substantial implementation verification and selected numerical calibration. It does not yet have complete physical validation against flight data or a full operational flight plan.

## 13. Known gaps and priority roadmap

### 13.1 Current gaps

1. There is no general candidate/stage memoization layer; the current AVL design-bit cache is narrower than the required complete key.
2. Basic history does not yet contain the full architecture, stage, residual, fidelity, scenario, provenance, and evidence contract.
3. Product ranking does not yet expose comparability, required-stage completion, robust feasibility, and soft residual as separate explicit keys.
4. `ParetoCandidate` is compact legacy data rather than a complete auditable assessment record.
5. There is no complete mixed categorical outer driver with per-architecture screening, optimization, promotion, and reporting.
6. Product method dispatch is inconsistent with the configured method catalogue: non-SQP names currently execute MADS.
7. The `turbo_1` kernel is a bounded local RBF surrogate with feasibility offsets; it is not a full Gaussian-process TuRBO implementation.
8. Robust/UQ/chance constraints, model-form uncertainty, and confidence intervals are not a general product service.
9. External high-fidelity tools remain dependent on local licensing, executable availability, convergence, and pinned data.
10. CPACS schema/version/mapping contracts still need to be pinned and tested as interoperability artifacts.
11. FLOPS/N+3/Class-II empirical methods need explicit applicability tags, calibration assumptions, and broader validation before supporting strong design claims.
12. There is no converged, verified preset optimization study demonstrating the full workflow from seed survival through high-fidelity/UQ promotion.

### 13.2 Mission/model limitations that affect claims

The present optimization should not be described as a complete operational mission optimizer. It lacks wind and legwise CAS–Mach scheduling, optimum altitude selection, full fuel-state trim/drag re-evaluation, phase-specific high-lift performance, and simultaneous multi-mission range/short-field optimization. Tank spanwise boundaries remain estimates unless a higher-fidelity tank/structure model is supplied.

### 13.3 Recommended implementation order

1. Make the run manifest and product dispatch unambiguous: either wire each configured method or narrow the public configuration to the currently executable MADS/SQP choices while preserving compatibility explicitly.
2. Introduce the typed assessment/stage record and complete `CandidateKey`; move history and cache semantics to that contract.
3. Add deterministic outer `ArchitectureKey` enumeration and per-architecture feasibility-first screening.
4. Separate hard residuals, soft residuals, ordinary objectives, comparability, and required-stage completion in the ranking implementation.
5. Add stage-level caching, serial/parallel equivalence tests, and robust invalidation/retry rules.
6. Add fidelity promotion with paired low/high evidence and minimum-fidelity checks.
7. Add scenario/UQ/robust ranking and sensitivity/identifiability reporting.
8. Calibrate and physically validate the claims before using optimization results for certification-like or operational decisions.

## 14. Source of truth and code map

The consolidated strategy is derived from these local sources:

- [Operational optimization guide](OPTIMIZATION.md) — user-facing inputs, pipeline flow, policies, fuel/tank settings, and documented method catalogue.
- [MDO research/design baseline](research/optimization-mdo.md) — target architecture, typed assessment contract, mixed-architecture funnel, fidelity, UQ, caching, ranking, and verification plan.
- [Methods and model notes](methods.md) — mission closure, fuel policy, mass methods, FLOPS scope, residuals, MADS rationale, and SQP details.
- [Current status](STATUS.md) — delivered behavior, superseding product-objective decisions, and known verification/external-tool limits.
- [Fuel/mission roadmap](FUEL_MISSION_ROADMAP.md) — delivered P1/P2/P4 scope and remaining P3/P5 mission/UQ work.
- [Handoff snapshot](../handoff.md) — recorded workspace test/gate evidence and external-tool status.

Key implementation files are:

- [`design_variables.rs`](../crates/alas-config/src/design_variables.rs) — canonical 16-coordinate vector registry.
- [`design_space.rs`](../crates/alas-config/src/optimizer/design_space.rs) — clean-sheet, reference-adaptation, and baseline-sandbox envelopes.
- [`differential_evolution_optimizer.rs`](../crates/alas-opt/src/differential_evolution_optimizer.rs) — product/compatibility dispatch, bounds, initialization, and result assembly.
- [`mads.rs`](../crates/alas-opt/src/search/mads.rs) — current product progressive-barrier MADS driver.
- [`sqp_search.rs`](../crates/alas-opt/src/sqp_search.rs) — current product SQP branch.
- [`search_methods`](../crates/alas-opt/src/search_methods) — isolated feasibility-first DE, CMA-ES, constrained NSGA-II, and bounded RBF-surrogate kernels.
- [`sizing.rs`](../crates/alas-opt/src/mdo/sizing.rs) — candidate preparation and mission-sized assessment.
- [`mda.rs`](../crates/alas-opt/src/mdo/mda.rs) — mass/fuel/CG closure loop.
- [`cost.rs`](../crates/alas-opt/src/mdo/cost.rs) — objective normalization, policies, residuals, and scalar cost.
- [`part_01.rs`](../crates/alas-pipeline/src/dual_solver_parts/part_01.rs) and [`part_02.rs`](../crates/alas-pipeline/src/dual_solver_parts/part_02.rs) — VLM/AVL optimization branches and finalist re-analysis.

When this summary conflicts with executable behavior, treat the current source, tests, and run manifest as the authority for what happened in a run; treat the research/design baseline as the authority for the intended future contract until the implementation catches up.
