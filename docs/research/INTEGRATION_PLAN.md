# ALAS research-integration plan

**Plan date:** 2026-08-27
**Scope:** dependency-ordered implementation of the local aircraft-design research corpus
**Status:** implementation plan, not an implementation-completion claim, certification basis, or finding of compliance

This plan turns the research library indexed by [README.md](README.md) into an
auditable engineering program. It is intentionally stricter than a discipline
wish list: a work package cannot claim completion merely because a formula or
report page exists. Its result must participate in the shared load-case,
closure, evidence, validity, and uncertainty contracts and must feed every
downstream consumer that depends on it.

The current repository does **not** implement every finding in the research
corpus. The present pipeline is a valuable foundation, but several important
models are one-way or post-processed rather than converged as one aircraft.
The goal of this plan is to close those loops without discarding existing
parity fixtures, independent solver evidence, or the useful CLI and GUI
surfaces already present.

## 1. Current baseline and truth boundary

### 1.1 What exists

The normal transport-aircraft path in
[`alas-pipeline`](../../crates/alas-pipeline/) currently performs, in broad
terms:

1. design-brief/configuration validation and baseline weight, balance, and
   stability estimation;
2. VLM- and optionally AVL-backed optimization;
3. full geometry, payload, two-pass mass estimation, fine VLM, trim, and
   simplified field-performance analysis;
4. CPACS round-trip/export, OpenVSP materialization, feasibility and
   requirements-acceptance summaries, database export, and report figures;
5. optional or independent VSPAERO, AVL, FLOWUnsteady, mission, MSES, and
   structural stages, some of which can execute concurrently.

The current worktree also contains an in-progress headless control surface in
[`alas-app`](../../crates/alas-app/): `run`, `modules`, `figures`, and `tools`
commands; dotted configuration overrides; module enable/disable selection;
dry-run JSON; effective-config output; SVG/PNG selection and figure filters;
tool discovery; and a root run manifest. These additions are the right basis
for agent-driven debugging, but they are complete only when the CLI contract
tests, headless smoke runs, artifacts, and repository gates pass at one pinned
revision. A CLI switch controls execution; it does not increase model fidelity
by itself.

Useful current foundations include the three-outcome stage type in
[`alas-types`](../../crates/alas-types/), typed external-solver statuses,
requirements acceptance, feasibility residuals, CPACS/OpenVSP adapters,
several independent aerodynamic paths, mission telemetry, structural sizing,
search-method diversity, and extensive golden/parity evidence.

### 1.2 Main physical and architectural gaps

| Gap | Present consequence | Required resolution |
|---|---|---|
| MTOW is prescribed and fuel can be a remainder | A numerically balanced report can hide an unclosed aircraft | Solve mass, energy, tank volume, mission fuel, structure, and systems loads to a declared closure tolerance. |
| Mission is downstream rather than an authoritative sizing loop | Range/fuel evidence does not resize the candidate that produced it | Feed segment burn, reserve, tank state, and mass changes back into sizing until converged. |
| Structural sized mass is not fed back through mass, CG, trim, mission, and loads | The reported structure and the evaluated aircraft can be different aircraft | Iterate structure mass/stiffness, mass properties, aerodynamic loads, aeroelastic corrections, and mission performance. |
| External aerodynamics is chiefly diagnostic | A better solver may not govern objective or requirement evidence | Promote only comparable, valid results through a declared fidelity policy and retain low/high-fidelity discrepancy. |
| Field performance is correlation-based | No independent accelerate-stop/accelerate-go `V1` search or true balanced-field evidence | Add a phase-resolved takeoff/landing evaluator, braking and runway states, and typed proxy-versus-physics status. |
| A usable-looking polar fallback can survive a failed fit | Performance may inherit coefficients that look plausible without adequate evidence | Make fallback provenance and validity blocking when the consuming requirement demands higher fidelity. |
| Control derivatives and dynamics are mainly report outputs | Optimization can select a shape without rotation, rudder, actuator, or dynamic-mode authority | Add condition grids, physical authority solves, actuator limits, and explicit control/aeroelastic feedback. |
| Optimizer coverage omits important disciplines | The nominal optimum need not be mission-, field-, structure-, or high-fidelity-feasible | Make the staged assessment contract the only source of optimizer ranking. |
| Aircraft systems are only mass priors or scattered loads | Bleed, shaft power, electric/hydraulic availability, heat, ram drag, routing, volume, failure cases, and CG are not closed | Add a resource-network systems model and couple it to every affected discipline. |
| Landing gear, brakes, high lift, icing, engine decks, safety, environment, and operations have important data/model gaps | Some current numbers are proxies or absent | Implement the staged models below; keep unsupported requirement assessments `NotEvaluated`. |

This baseline agrees with the common doctrine in
[AIRCRAFT_DESIGN_DOCTRINE.md](AIRCRAFT_DESIGN_DOCTRINE.md), the workflow in
[REQUIREMENTS_FIRST_AIRCRAFT_DESIGN.md](../REQUIREMENTS_FIRST_AIRCRAFT_DESIGN.md),
and the individual capability maps in the discipline memos. It must be
re-audited after each work package; this section is not a permanent statement
about future code.

### 1.3 Repository-integrity prerequisite

Repository integrity is work package zero, before physical results are used as
new evidence:

- `docs/PHYSICS_SOLVER_FLOW.md` is referenced by `AGENTS.md` and the repository
  overview but is absent from the worktree and was not found in the inspected
  history.
- The tracked files `docs/PORTING.md`, `docs/RUNNING.md`, and
  `docs/ARCHITECTURE.md` are deleted in pre-existing user work. This plan does
  not restore or overwrite those changes.
- `docs/PORTING.md` is a required provenance/licence/fixture/parity ledger.
  The current `cargo xtask gate` failure is therefore a real repository-state
  failure, not a physics failure to waive.

The repository owner must choose and record one coherent disposition: recover
the intended tracked documents from an authoritative source, author approved
replacements, or intentionally change their references and the coupled
`xtask/src/ledger.rs` / `xtask/src/evidence.rs` policy together. The solver-flow
document must then be created from the audited code path and reviewed against
the CLI stage catalogue. Until that decision is merged and `cargo xtask gate`
passes, releases may be described only as development runs from a dirty,
documentation-incomplete tree. No physics work package may silently edit the
deleted files to make its own gate green.

## 2. Program invariants

The following rules apply to every work package.

1. **One frozen aircraft authority.** A run freezes the normalized design
   brief, architecture seed, CPACS/config authority, design vector, bounds,
   load cases, fidelity plan, and run options. Preview, optimizer, solvers, and
   reports derive from that snapshot.
2. **One case, one state.** Any scalar that depends on phase, mass, CG,
   atmosphere, configuration, propulsion, control, runway, failure, or systems
   state names a `LoadCase`; global design-point ambiguity is invalid input.
3. **Execution is not acceptance.** Process launch, parse success,
   comparability, physical convergence, and requirement satisfaction are
   separate statuses.
4. **No missing-value pass.** Missing input, absent translator, out-of-domain
   model, nonconvergence, stale cache, or unsupported failure case cannot
   produce zero residual or a passing default payload.
5. **Closure before comparison.** Candidates compared by an optimizer must be
   closed to the same declared fidelity. Stale fuel, structure, systems, CG,
   or aero states make candidates non-comparable.
6. **Feasibility before preference.** Valid execution and hard requirements
   precede robust-hard margins, soft penalties, and objectives. Diagnostics
   never alter feasibility unless the user promotes them with an evaluator and
   minimum fidelity.
7. **Fidelity does not overwrite evidence.** Raw proxy, native, external, and
   validation results remain separate. Promotion adds evidence and an explicit
   disposition; it never relabels a low-fidelity result.
8. **Every feedback is visible.** Fixed-point/nonlinear iteration stores each
   iteration's state, residual vector, relaxation or solver settings,
   termination reason, and selected converged state.
9. **Uncertainty is typed.** Input, aleatory, calibration, numerical, and
   model-form uncertainty remain distinguishable and are propagated only over
   stated distributions/correlations.
10. **Reports cite evidence.** Every table row, pass/fail statement, and PNG/SVG
    figure identifies case, fidelity, validity, uncertainty, and evidence IDs.

## 3. Shared contracts

### 3.1 Ownership and compatibility strategy

The versioned, serialization-safe primitives belong in
[`alas-types`](../../crates/alas-types/) so discipline crates do not depend on
the pipeline. [`alas-config`](../../crates/alas-config/) owns user-editable
requirement/scenario construction and validation; [`alas-units`](../../crates/alas-units/)
owns conversions and dimension checks; [`alas-pipeline`](../../crates/alas-pipeline/)
owns freeze, orchestration, artifact retention, and manifest assembly.

The current `Stage<T>` wire format is parity-sensitive and writes default
payload fields for non-results. Do not break it in the first contract change.
Introduce the richer `StageResult<T>` as an additive sidecar and provide an
explicit compatibility adapter. New requirement evaluators and cache code must
consume `StageResult<T>`, where a non-completed status has no payload.

All contracts below carry `schema_version`, use canonical SI internally, use
stable string IDs, reject unknown required enum values, and support explicit
schema migration. Serialized maps and lists that participate in hashes have a
defined order.

### 3.2 `RequirementRecord`

```text
RequirementRecord {
  schema_version, requirement_id, revision,
  kind: stakeholder_need | tlar | derived | certification_reference
        | operational_constraint | objective | diagnostic,
  statement, rationale,
  quantity: scalar | interval | categorical | boolean | vector,
  unit, dimension, relation: min | max | target | equality | in_set,
  tolerance_or_scale,
  applicability: scenario_ids[] + load_case_ids[] + predicates[],
  policy: hard | soft | objective | diagnostic,
  source: user | preset | derived | regulation | research | assumption,
  source_refs[], parent_requirement_ids[], allocation_ids[],
  evaluator_id, minimum_fidelity, verification_method,
  assurance_scope, lifecycle_status,
  evidence_ids[]
}
```

Required invariants:

- value, unit/dimension, direction, scale, applicability, policy, source, and
  acceptance criterion are present before baselining;
- a hard requirement has exactly one primary evaluator or an explicit
  `NotEvaluated` blocking assessment;
- derived requirements record derivation inputs and evaluator version;
- regulation references carry authority, jurisdiction, amendment/version,
  applicability rationale, and access date;
- evaluators never mutate the frozen requirement; they emit evidence.

### 3.3 `LoadCase`

```text
LoadCase {
  schema_version, load_case_id, family, scenario_id, revision,
  phase, event_or_segment,
  mass_state_id, mass_kg, payload_layout_id,
  tank_states[], fuel_or_energy_state, reserve_state,
  cg_m[3], inertia_tensor_kg_m2[3][3], uncertainty_state_id,
  atmosphere: model + altitude + temperature + pressure + density
              + humidity + wind + ISA policy,
  kinematics: speed_definition + speed + Mach + flight_path + ground_state,
  configuration: gear + flap + slat + spoiler + doors + brakes
                 + anti_ice + surface_condition + ground_effect,
  propulsion_states[], system_load_schedule_id,
  controls: commanded/actual positions + limits + trim policy,
  failure_scenario_id,
  runway_or_airport_state,
  structural_load_definition,
  acceptance_criterion_ids[],
  provenance
}
```

Case families must cover at least nominal cruise, max-payload cruise, MTOW
takeoff, hot/high takeoff, accelerate-stop/go, MLW landing, approach, climb and
time-to-climb, OEI segments and ceiling, VMO/MMO, forward/aft CG, loading and
fuel-transfer sequences, maneuver/gust, evacuation, cargo loading, turnaround,
dispatch, environmental cases, and systems normal/failure cases. Each
discipline may add a typed extension, but it may not reinterpret common fields.

### 3.4 `StageResult<T>` and residuals

```text
StageResult<T> {
  schema_version, stage_id, evaluator_id,
  execution_status: completed | rejected_physical | not_evaluated
                    | inconclusive | failed_evaluator | blocked_dependency,
  payload: Option<T>,
  load_case_ids[], requirement_ids[],
  residuals: ConstraintResidual[], diagnostics[], warnings[],
  fidelity: level + method + model_version,
  validity_domain + validity_disposition,
  uncertainty_summary,
  input_artifact_ids[] + input_hashes[], output_artifact_ids[],
  solver/process/parser/comparability statuses,
  iteration_and_convergence,
  cache_disposition,
  timing_and_resources,
  evidence_ids[]
}

ConstraintResidual {
  residual_id, requirement_id, load_case_id,
  actual: Option<quantity>, target, relation, scale,
  signed_margin: Option<quantity>, normalized_violation: Option<number>,
  policy, status, uncertainty_adjusted_margin,
  evaluator_id, fidelity, evidence_ids[]
}
```

`payload` is present only for a trustworthy completed or physically rejected
calculation that intentionally retains partial physical output. A failed or
blocked evaluator cannot manufacture default values. Positive normalized
violation has one repository-wide meaning. Physical rejection and evaluation
failure remain separate even if both stop a candidate.

### 3.5 `Evidence`

```text
Evidence {
  schema_version, evidence_id,
  kind: requirement_evaluation | verification | validation | calibration
        | comparison | provenance | visualization | limitation,
  subject_ids[], claim, assessment_status,
  load_case_ids[], scenario_ids[],
  evaluator: id + source_revision + fidelity + validity,
  input_artifact_ids[], output_artifact_ids[],
  actual_target_residual,
  tool_process_parser_and_convergence,
  uncertainty_and_comparison_error,
  limitations[], applicability,
  generated_by_stage_id, derived_from_evidence_ids[],
  created_utc
}
```

Evidence is immutable. A correction creates a new record and supersession
link. `pass`, `fail`, `not_evaluated`, `inconclusive`, `diagnostic`, `blocked`,
and `not_applicable_with_rationale` are assessment states; no state is named
`certified`.

### 3.6 `RunManifest`

`RunManifest` implements the four-layer digital thread specified in
[digital-thread.md](digital-thread.md):

```text
RunManifest {
  manifest_kind, schema_version, run_id, campaign_id, created_utc,
  overall_status,
  authority: design_brief + aircraft/CPACS + schema/units/frames,
  design_snapshot_id,
  requirement_ids[], load_case_ids[], architecture_id,
  design_vector_and_bounds_artifacts,
  requested_and_effective_stage_plan,
  stage_dependency_dag, stage_results[],
  tools[] + executable/deck/model/parser identities,
  environment_allowlist + source/lockfile/diff hashes,
  randomness + deterministic evaluation order,
  resource_policy + worker/license/GPU limits,
  artifacts[] + SHA-256 + derivation + retention,
  evidence_ids[], reports_and_figure_evidence[],
  cache_summary, closure_summary, uncertainty_summary,
  termination_reason
}
```

The manifest written by the CLI before execution is the planned manifest; the
same run ID is finalized after execution. Cancellation, crash, unavailable
tool, or rejected candidate must still leave an auditable manifest and the
artifacts produced safely before termination.

### 3.7 Evaluation identity

The canonical stage cache and comparison identity is:

```text
SHA256(
  normalized requirement snapshot
  + architecture seed and topology
  + normalized continuous vector
  + authoritative geometry/upstream artifact hashes
  + load case and uncertainty scenario
  + stage and fidelity IDs
  + evaluator/model/tool/parser versions
  + numerical settings and tolerance policy
  + unit/frame/reference policy
)
```

Any changed term is a cache miss. Tolerant nearest-neighbor reuse is a named
surrogate prediction with uncertainty, never a cache hit.

## 4. Dependency graph and coupled solve

### 4.1 Program dependency graph

```text
WP-00 repository + CLI truth
  -> WP-01 shared contracts and manifest
     -> WP-02 requirements, cases, evaluator registry
     -> WP-03 deterministic execution, cache, resource and V&V harness
        -> WP-04 architecture + master geometry + accommodation/tanks/gear
           -> WP-05 propulsion decks + aircraft-systems foundation
              -> WP-06 mass/fuel/energy/mission closure
                 -> WP-07 aero/propulsion/field/high-lift/gear-brake closure
                    -> WP-08 structures/aeroelastic feedback
                       -> WP-09 stability/control and complete envelope
                          -> WP-10 safety/certification architecture screen
                             -> WP-11 environment/operations/economics
                                -> WP-12 mixed-architecture MDO + robust UQ
                                   -> WP-13 finalist substantiation and release
```

This is the order of authoritative integration, not a ban on parallel feature
development. A later discipline can build fixtures earlier, but it cannot
become a hard optimizer gate before its upstream state is authoritative.

### 4.2 Outer architecture loop

The brief expands into a small morphological catalogue: role, layout, deck,
wing/tail, propulsion/energy, engine count, tank, gear, cargo, cabin, and
high-lift choices. Unsupported combinations are rejected with typed reasons.
Continuous optimizers operate inside a named seed; they do not round floating
point variables into topology. Architecture comparison occurs only after each
survivor closes the same required cases and fidelity.

### 4.3 Geometry/accommodation/tanks/gear loop

```text
cabin/cargo/exits/doors/crew/monuments
  -> pressure shell and external loft
  -> wing/tail/nacelle/control placement
  -> tank volumes, systems routes, gear bays and retraction
  -> ground clearance, load paths, usable interior volume
  -> updated dimensions and arrangement
  -> convergence or typed packaging rejection
```

This loop owns integer placement, clearances, master geometry identity, usable
tank volume, gear stance/retraction, and analysis-representation gates. A
preview mesh, a CPACS file, or a seat-count preset does not satisfy it by
itself. See [cabin-interiors-cargo.md](cabin-interiors-cargo.md) and
[geometry-configuration.md](geometry-configuration.md).

### 4.4 Mass/fuel/energy/mission loop

```text
payload + mission graph + reserve policy
  -> Class-I seed and wing/thrust/power loading
  -> component/system/propulsion/structure mass ledger
  -> tank-local fuel/energy state and volume/feedability
  -> segment mission with propulsion/aero/system extractions
  -> landing/reserve mass, CG and inertia
  -> update MTOW/OEW/fuel/geometry
  -> converged state or mass/volume/feed/mission rejection
```

The converged candidate exposes OEW, ZFW, ramp, takeoff, landing, reserve, and
critical intermediate states. MTOW-minus-empty-minus-payload remains only a
Level-0 diagnostic remainder. It cannot establish fuel capacity or range. See
[mission-sizing.md](mission-sizing.md) and [mass-balance.md](mass-balance.md).

### 4.5 Propulsion/aero/performance loop

Installed thrust/power/flow comes from an engine or propulsor deck at every
required condition, including bleed/shaft/electric extraction and failure
state. Aerodynamics returns a decomposed drag and force/moment state with
validity. The performance solver integrates climb, ceiling, accelerate-go,
accelerate-stop, takeoff, approach, landing, braking, and runway margins. Wing
area, thrust/power, high-lift geometry, gear/brakes, engine selection, tank
energy, and mission fuel are then updated. The loop converges only when the
same installed engine, configuration, mass, atmosphere, and aero state support
all reported performance cases. See [aerodynamics.md](aerodynamics.md),
[propulsion-energy.md](propulsion-energy.md), and
[performance-airport.md](performance-airport.md).

### 4.6 Structures/aeroelasticity loop

Named maneuver, gust, landing, pressurization, control, propulsion, fuel, and
payload cases generate equilibrium-checked loads and load paths. Sizing returns
mass, CG/inertia contribution, stiffness, margins, modes, and validity.
Aeroelastic deformation updates aerodynamic loading and control effectiveness;
the revised structure feeds mass and mission. Iterate until load, structural
mass, stiffness/deformation, and aeroelastic residuals close. Unsupported
fatigue, damage tolerance, flutter, divergence, or control reversal remains
`NotEvaluated`. See [structures-aeroelasticity.md](structures-aeroelasticity.md).

### 4.7 Stability/control loop

For the mass/CG/configuration/failure envelope, solve trim, static stability,
rotation/recovery, lateral-directional authority, physical OEI controllability,
dynamic modes, actuator-limited response, and gust/aeroelastic response at the
declared fidelity. Update tail/control geometry, propulsion placement, gear
rotation geometry, CG limits, and control-system resources, then repeat the
affected upstream loops. OEI climb and OEI controllability are separate hard
requirements. See [stability-control.md](stability-control.md).

### 4.8 Coupling scheduler

Each tightly coupled candidate uses a deterministic scheduler:

1. evaluate the cheapest invalidity/packaging gates;
2. iterate continuous closures with recorded scaling and relaxation, using the
   existing HYBRD primitive where the residual/Jacobian behavior is suitable;
3. keep discrete architecture changes outside the continuous fixed point;
4. recompute every invalidated downstream stage from content identity;
5. reject nonconvergence explicitly and retain the best finite iteration for
   diagnosis, not acceptance;
6. compare candidates only when required residuals use the same convergence
   and fidelity policy.

## 5. Work packages, acceptance evidence, and exit criteria

### WP-00: Repository integrity and completed headless baseline

**Depends on:** none.
**Owners:** `alas-app`, `alas-config`, `alas-exec`, `alas-pipeline`, `alas-report`,
`xtask`, repository documentation.

Deliverables:

- resolve the documentation/ledger disposition described in Section 1.3;
- freeze and document the audited pipeline stage DAG and CLI module catalogue;
- complete CLI input, CPACS, overrides, stage selection, dry-run, tools,
  effective-config, JSON summary, output-root manifest, and PNG/SVG export;
- preserve mandatory core analysis/CPACS/feasibility/database behavior and
  distinguish it from optional downstream modules;
- add one minimal no-GUI smoke fixture and one full headless fixture.

Acceptance evidence:

- parser/contract tests cover help, invalid combinations, overrides, module
  precedence, deterministic dry-run JSON, figure filtering, and exit codes;
- a headless run produces the same selected physical outputs and hashes across
  repeat runs with a fixed seed;
- selected PNG files decode, have nonzero dimensions, and cite figure evidence;
- missing optional tools yield typed statuses rather than a false run failure;
- `cargo test --workspace` and `cargo xtask gate` pass at the pinned revision,
  or every pre-existing failure has a separately approved, time-bounded waiver
  in the manifest. A release exit requires no waiver.

**Exit:** agents can inspect inputs, effective configuration, stage plan,
status, residuals, artifacts, and figures without screen control, and the
repository's own integrity checks agree with that revision.

### WP-01: Common contracts and digital-thread spine

**Depends on:** WP-00.
**Owners:** `alas-types`, `alas-units`, `alas-config`, `alas-pipeline`,
`alas-report`, `xtask`.

Deliverables are the five contracts in Section 3, canonicalization and schema
migrations, artifact hashing, evidence graph, planned/final run manifests, and
compatibility adapters for existing stage/status/report types.

Acceptance evidence:

- JSON golden fixtures round-trip every status without default numeric payload
  on non-results;
- semantically equal unit/order inputs produce one snapshot hash; changing any
  identity field changes it;
- every artifact is hash/size/producer/retention complete and tampering fails
  the manifest audit;
- a report assertion or figure without evidence IDs fails `xtask` audit;
- a dirty worktree is reproducible through commit plus captured-diff identity.

**Exit:** all later work can publish a case-specific `StageResult` and immutable
`Evidence` without depending on GUI or optimizer code.

### WP-02: Requirement registry, case expansion, and evaluation matrix

**Depends on:** WP-01.
**Owners:** `alas-config`, `alas-acceptance`, `alas-pipeline`, `alas-gui`.

Deliverables:

- promote current per-field policies into canonical `RequirementRecord`s while
  retaining compatibility adapters;
- add ConOps/scenario records, reserve policy, regulatory-basis record, and
  deterministic load-case-family expansion;
- add a registry mapping requirement and case to primary evaluator, fallback,
  minimum fidelity, residual direction/scale, and verification method;
- lint ambiguity, units, missing case context, contradictory requirements,
  missing parent trace, and unevaluable hard requirements before launch;
- generate a verification/coverage matrix in CLI, GUI, JSON, and report form.

Acceptance evidence includes property tests for unit conversions/residual
signs, exact expansion fixtures for the minimum case catalogue, cross-field
contradiction fixtures, and a mixed hard/soft/objective/diagnostic report in
which unsupported requirements are explicitly `NotEvaluated`.

**Exit:** every baselined requirement is traceable to cases and an evaluator or
is visibly blocked by missing evidence; no hard requirement disappears from a
run.

### WP-03: Deterministic execution, cache, resources, and V&V harness

**Depends on:** WP-01; begins in parallel with WP-02.
**Owners:** `alas-pipeline`, `alas-opt`, `alas-exec`, `alas-testkit`, `xtask`.

Deliverables:

- stage-level content-addressed cache using Section 3.7 identity;
- a dependency DAG with invalidation, cancellation, retries, and partial-run
  preservation;
- a global resource broker for CPU, memory, process, external-license, GPU,
  and I/O budgets;
- deterministic candidate/case ordering and independent RNG streams;
- benchmark/validation fixture metadata, comparison metrics, tolerance source,
  and calibration-versus-validation split;
- reference-aircraft campaign schema covering geometry, mass, aero,
  propulsion, mission, field, stability, structures, and reported uncertainty.

Acceptance evidence:

- one-worker and many-worker runs produce identical ordered assessments,
  hashes, rankings, and manifests;
- concurrent external tools use unique workspaces and obey configured token
  limits; cancellation leaves no claim without artifacts;
- deterministic physical failures can be cached, while transient launch,
  license, network, or resource failures are retried/invalidated by policy;
- changing case, fidelity, tolerance, evaluator, tool, geometry, or upstream
  artifact forces a miss;
- benchmark data used for calibration is not reused as blind validation.

**Exit:** later expensive disciplines can run reproducibly and in parallel
without oversubscription or stale evidence.

### WP-04: Architecture, authoritative geometry, and packaging closure

**Depends on:** WP-02 and WP-03.
**Owners:** `alas-config`, `alas-geom`, `alas-payload`, `alas-pipeline`, CPACS and
OpenVSP adapters.

Deliverables:

- typed architecture seeds and supported topology catalogue;
- shared master geometry for preview, cabin/cargo, tanks, gear, controls,
  structures, and solver representations;
- integer seat/crew/monument/exit/ULD/cargo placement with requested, placed,
  available, and unfilled capacity by deck/hold;
- pressure shell, doors, accessibility, loading sequence, systems routing
  envelopes, tank exclusions/ullage/feed zones, gear contact/retraction/bay,
  high-lift mechanism, and structural attachment intent;
- geometry gates G0-G8 from [geometry-configuration.md](geometry-configuration.md)
  and source-to-derived UID/frame/unit/hash mappings.

Acceptance evidence includes analytic planform/volume fixtures, self-
intersection and clearance failures, double-deck/ULD layouts, tank-versus-
gear/system collision cases, gear tipback/turnover/tail-strike tests, control/
high-lift deployment geometry, CPACS round-trip identity, and preview-versus-
solver geometry consistency.

**Exit:** every surviving seed materializes deterministically into one aircraft
whose accommodation, tanks, gear, control surfaces, and solver representations
agree. Unusual architectures remain unsupported rather than approximated by a
conventional hidden default.

### WP-05: Engine-deck and aircraft-systems foundation

**Depends on:** WP-04.
**Owners:** `alas-prop`, proposed `alas-systems`, `alas-config`, `alas-mass`,
`alas-geom`, `alas-pipeline`.

Deliverables:

- versioned installed engine/propulsor decks over altitude, Mach/speed,
  temperature, rating, throttle, shaft/bleed/electric extraction, installation,
  inlet/nozzle/nacelle losses, transient/failure state, and uncertainty;
- explicit interpolation, extrapolation rejection, deck provenance, and
  cycle-model-versus-deck identity;
- a new function-oriented `alas-systems` resource graph described in Section
  10, initially retaining FLOPS-style system mass as a Level-0 prior;
- phase/failure load schedules and normal/failure resource-balance cases;
- per-case mass/location, shaft/bleed/electric extraction, heat rejection, ram
  drag, routing/volume, and availability outputs back to owning disciplines.

Acceptance evidence includes deck-grid exact/interpolation tests, out-of-domain
cases, installed-versus-uninstalled thrust bookkeeping, whole-network resource
conservation, failure reachability/isolation, deterministic graph ordering, and
Level-0 mass parity. Public reference fixtures must be separate from any OEM-
quality claim.

**Exit:** mission and performance can request trustworthy installed propulsion
and systems loads for each case. Novel architectures without calibrated decks
or networks remain `NotEvaluated` beyond transparent Level-0 priors.

### WP-06: Coupled mass, fuel/energy, and mission closure

**Depends on:** WP-05.
**Owners:** `alas-mass`, `alas-payload`, `alas-mission`, `alas-prop`,
`alas-systems`, `alas-math`, `alas-pipeline`.

Deliverables:

- item-level installed mass ledger with source, method, state predicate,
  location, tensor, uncertainty, and calibration delta;
- named DWM/OEW/ZFW/ramp/takeoff/landing/reserve states;
- tank-local capacity, temperature/density, ullage, unusable fuel, feed and
  transfer topology, energy-storage state, and CG/inertia evolution;
- mission graph with taxi, climb, cruise, descent, alternate, holding,
  diversion/critical-failure, and reserve policy components;
- Class-I seed, Class-II replacement, payload-range corners, matching-diagram
  inputs, and iterative closure through propulsion, systems, and provisional
  structure mass.

Acceptance evidence is the twelve-test mass/balance set in
[mass-balance.md](mass-balance.md), plus Breguet limiting checks, segment energy
conservation, reserve decomposition, payload-range monotonicity, feed failure,
mission nonconvergence, and convergence from multiple initial mass seeds.

**Exit:** the candidate's mass and energy are solved outputs, not prescribed
answers; all downstream cases consume the correct state snapshot. A closure
remainder is clearly labeled and cannot satisfy tank or mission requirements.

### WP-07: Aerodynamics, propulsion, performance, high lift, and gear/brakes

**Depends on:** WP-06.
**Owners:** `alas-aero`, `alas-prop`, `alas-perf`, proposed gear/brake module or
clearly bounded submodules in `alas-perf`/`alas-geom`/`alas-struct`,
`alas-pipeline`.

Deliverables:

- decomposed drag and force/moment database with clean, high-lift, powered,
  icing, ground-effect, gear, control, and engine-out state dimensions;
- explicit validity/diagnostic propagation through drag buildup, VLM,
  lifting-line, NeuralFoil/MSES, AVL/VSPAERO, and consumers;
- real deployed flap/slat geometry and calibrated section/aircraft high-lift
  evidence; no clean-wing substitution;
- independent accelerate-stop and accelerate-go integration with a `V1` root
  search, raw curves, branch control, stopway/clearway, failure/crew delay,
  brakes, reverse-thrust policy, wind/slope/surface/contamination state;
- landing phase integration, brake kinetic-energy/thermal/cool-down/fade and
  tire/load/speed limits; gear ground-load/load-share and pavement inputs;
- climb schedule, time-to-climb, ceiling/drift-down, approach/V-speed, airport
  declared-distance, stand, and legacy ACN/PCN versus current ACR/PCR semantics.

Acceptance evidence includes analytic energy/force cases, independent field-
path fixtures, `ASD(V1)-AGO(V1)` bracketing/root tests, high-lift geometry and
polar benchmarks, clean-state refusal tests, engine-deck coupling, ground-
effect/icing status tests, brake-energy conservation, and airport unit/
declared-distance fixtures.

**Exit:** all performance values identify their proxy/physics/validated scope.
TOFL, landing, approach, or icing requirements remain `NotEvaluated` when the
required high-lift, brake, runway, propulsion, or validation evidence is
absent.

### WP-08: Structural and aeroelastic feedback closure

**Depends on:** WP-07.
**Owners:** `alas-struct`, `alas-aero`, `alas-mass`, `alas-pipeline`.

Deliverables:

- named load-case catalogue and equilibrium-checked load paths for wing,
  fuselage/pressurization, empennage, gear/landing, control, engines/pylons,
  tanks/payload, gust, and failure-induced loads;
- staged beam/wingbox, shell/static/modal/buckling, reduced aeroelastic, and
  external FEA contracts with material/allowable provenance;
- strength, stiffness, local/global buckling, joints/load introduction,
  preliminary fatigue/damage-tolerance proxies, modes, and uncertainty;
- structural mass/CG/inertia and aeroelastic deformation/control-effectiveness
  feedback to WP-06, WP-07, and WP-09;
- strict NASTRAN input, mesh, equilibrium, solver, parser, convergence, and
  correlation promotion gates.

Acceptance evidence includes analytic beam/panel cases, mesh refinement and
quality, global force/moment equilibrium, mass feedback changing mission and
trim, modal benchmark cases, AGARD 445.6 only where model scope matches, and
failure tests for missing load paths/allowables/results.

**Exit:** no candidate is ranked using structural mass that was merely appended
after its mass/mission solve. Flutter, divergence, gust response, control
reversal, fatigue, and damage tolerance pass only at their required fidelity;
otherwise they remain explicit gaps.

### WP-09: Stability, control authority, and envelope closure

**Depends on:** WP-08.
**Owners:** `alas-stab`, `alas-aero`, `alas-perf`, `alas-mass`, `alas-systems`,
`alas-struct`, `alas-pipeline`.

Deliverables:

- trim/static-stability matrix over mass, CG, speed/Mach, altitude,
  configuration, propulsion, icing, ground effect, and failure state;
- elevator/stabilizer rotation and recovery, aileron/roll, rudder/yaw,
  crosswind, and physical VMC/VMCL/VMCG authority with actuator position, rate,
  force, delay, and system availability;
- independent OEI performance and OEI controllability assessments;
- full small-disturbance state-space and nonlinear 6-DOF path with derivative
  provenance, modes, actuator/control-law limits, failure modes, and later gust/
  aeroservoelastic coupling;
- condition/CG coverage summary and feedback to tail/control sizing, wing/
  engine/gear placement, systems resources, and operational envelope.

Acceptance evidence includes sign/frame/datum tests, finite-difference step and
mesh convergence, closed-form versus VLM versus AVL comparisons, trim
nonconvergence, surface saturation, physical asymmetric-thrust moment balance,
analytic modes, and flexible-versus-rigid case distinctions.

**Exit:** a nominal static-margin number cannot hide an untrimmed, uncontrollable,
or uncovered case. Handling-quality or certification claims remain
`NotEvaluated` without the corresponding validation/test level.

### WP-10: Systems safety and certification-oriented architecture screen

**Depends on:** WP-09; preliminary FHA work may start after WP-05.
**Owners:** proposed `alas-safety`, `alas-systems`, `alas-config`,
`alas-acceptance`, `alas-pipeline`, `alas-report`.

Deliverables:

- regulatory-basis/version/applicability catalogue and requirement obligations;
- function hierarchy, FHA/PSSA-style failure conditions, severity/rationale,
  dependencies, independence/common-resource checks, redundancy/isolation,
  dispatch/degraded configurations, and evidence gaps;
- architecture screens for fire/smoke/EWIS, lightning/HIRF, bird strike,
  icing, emergency landing, evacuation/ditching, pressure, propulsion/energy,
  and failure-induced loads;
- a compliance matrix that distinguishes conceptual screen, later analysis,
  test/inspection, and unavailable evidence.

Acceptance evidence includes deterministic graph cut/reachability fixtures,
single-resource/common-cause discovery, required-function availability by
failure case, evacuation/accommodation trace, and report-language tests that
forbid `certified`, `approved`, or unqualified `compliant` output.

**Exit:** architecture-level hazards and missing assurance evidence can reject
or block a candidate by explicit policy. ALAS still makes no certification or
airworthiness claim.

### WP-11: Environment, lifecycle, operations, and economics

**Depends on:** WP-10 for system/safety state; deterministic mission outputs
from WP-06 allow earlier prototypes.
**Owners:** proposed `alas-environment` and `alas-ops` (or equivalently isolated
new modules), `alas-mission`, `alas-payload`, `alas-report`, `alas-viz`.

Deliverables:

- environmental brief and evidence contract; mission fuel/energy and species
  inventory; local-air-quality, non-CO2, noise/procedure, parametric LCA,
  manufacturing/MRO/end-of-life, and energy-carrier/infrastructure stages;
- operations scenario set, deterministic ground-task graph, boarding/cargo
  distributions, turnaround P50/P90, airport stand/GSE/pavement adapters,
  dispatch/reliability/maintainability proxies, DOC and lifecycle cashflow;
- explicit geography, time horizon, functional unit, allocation, currency/year,
  schedule/fleet/operator assumptions, data coverage, and uncertainty;
- physical feasibility kept separate from environmental targets and airline
  objectives.

Acceptance evidence includes mass/energy/species conservation, lifecycle
boundary tests, no-double-counting tests, noise/source/procedure coverage,
ground-task critical path/resource conflicts, cost-basis sensitivity,
scenario-weight/tail aggregation, and missing-data status fixtures.

**Exit:** these metrics can rank physically feasible aircraft only inside their
declared data domain. Missing emissions factors, noise calibration, operator
reliability, costs, airport resources, or lifecycle inventories remain
diagnostic/`NotEvaluated` rather than universal defaults.

### WP-12: Mixed-architecture MDO, robust UQ, and fidelity promotion

**Depends on:** WP-03 and all disciplines required by the selected study; full
program exit depends on WP-11.
**Owners:** `alas-opt`, `alas-pipeline`, all evaluator owners.

Deliverables:

- architecture enumeration outside continuous search; feasibility-first DE as
  a default inner method, with CMA-ES/NSGA-II/local-surrogate alternatives
  compared under identical budgets;
- complete candidate history containing identity, stages, residuals, fidelity,
  scenario, cache state, evidence, closure, and termination;
- total ordering: evaluation completeness, hard feasibility, robust-hard
  margin, normalized hard severity for diagnosis, soft penalty, then objective;
- uncertainty scenario service with correlations, common random numbers,
  convergence/confidence, quantile/chance constraints, and model-form ensembles;
- Pareto diversity, cross-architecture comparability, surrogate holdout/error/
  trust-region controls, and explicit low/high-fidelity discrepancy models;
- promotion policy requiring current-level completeness, boundary/diversity
  value, paired calibration value, and final re-evaluation of winners and near
  competitors.

Acceptance evidence is the V1-V23 suite proposed in
[optimization-mdo.md](optimization-mdo.md), extended so mission, field,
systems, structures, safety, environment, and operations can be required
stages. Repeated seeds/methods, parallel equivalence, cache equivalence,
no-feasible diagnostics, robust synthetic fixtures, and finalist rechecks are
mandatory.

**Exit:** an objective cannot select a candidate with stale or unevaluated hard
physics. The run reports why architectures died, whether the search converged,
which evidence promoted the finalist, and which uncertainty remains.

### WP-13: External substantiation, unified reference campaign, and release

**Depends on:** WP-12.
**Owners:** all discipline owners, `alas-exec`, `alas-testkit`, `alas-report`,
`xtask`.

Deliverables:

- unified conventional reference-aircraft campaign and separate benchmark
  families for aero, high lift, propulsion, field performance, mass, systems,
  stability, structures/aeroelasticity, mission, and operations/environment;
- paired low/high-fidelity studies with mesh/time/model sensitivity and
  experimental or independent-data comparisons where legally available;
- CPACS and all solver decks, raw outputs, parsers, transformations, hashes,
  transcripts, limitations, and comparison contracts retained;
- release coverage matrix and model cards specifying safe claims, validity,
  calibration, validation error, and unresolved cases.

Acceptance evidence includes fresh replay from the manifest, independent
reproduction on a second supported environment when possible, report-to-
artifact trace audit, benchmark tolerance review, and a deliberate review of
every `NotEvaluated` hard requirement.

**Exit:** a release may claim only the capabilities supported by retained
evidence. “All research integrated” means every research finding has a ledger
disposition and every implemented hard claim passes its evidence gate; it does
not mean every conceivable certification analysis exists.

## 6. Research-to-deliverable traceability

Every recommendation in the research corpus must be assigned a stable finding
ID in a machine-readable implementation ledger, for example
`research.requirements-systems.RQ-001`. The ledger fields are: memo and section,
finding, owning work package/crate, planned fidelity, requirement/case IDs,
implementation revision, test/evidence IDs, status, data gap, and disposition.
Allowed statuses are `Planned`, `ImplementedUnverified`, `Verified`,
`DeferredWithRationale`, `BlockedByData`, and `NotApplicableWithRationale`.
Only `Verified` counts as implemented for a release claim.

| Research memo | Primary owners | Required deliverables / work packages |
|---|---|---|
| [README.md](README.md) | research ledger, `xtask`, report | Source/evidence boundary, source IDs/hashes/rights, failed-PDF exclusions, and review rule in WP-01/WP-13. |
| [AIRCRAFT_DESIGN_DOCTRINE.md](AIRCRAFT_DESIGN_DOCTRINE.md) | all owners | Canonical study model, staged evaluator, closure order, feasibility/fidelity doctrine, digital-thread invariants, and definition of done across WP-01-WP-13. |
| [requirements-systems.md](requirements-systems.md) | `alas-config`, `alas-acceptance`, `alas-types` | Typed requirements, ConOps, load cases, policy, assurance scope, V&V matrix, evaluator trace, and regulatory boundary in WP-01/WP-02/WP-10. |
| [mission-sizing.md](mission-sizing.md) | `alas-mission`, `alas-mass`, `alas-perf`, `alas-prop` | Mission graph/reserve policy, payload-range, Class-I/II closure, matching constraints, architecture seeds, and robust margins in WP-04-WP-07/WP-12. |
| [cabin-interiors-cargo.md](cabin-interiors-cargo.md) | `alas-payload`, `alas-geom`, `alas-mass`, `alas-ops` | Seats/aisles/exits/monuments/accessibility, overhead/carry-on, deck/ULD/cargo capacity, loading/CG, evacuation and turnaround interfaces in WP-04/WP-06/WP-10/WP-11. |
| [geometry-configuration.md](geometry-configuration.md) | `alas-geom`, `alas-config`, CPACS/OpenVSP adapters | Mixed topology, parameterization, master geometry, CST/section schema, tanks/gear/high lift/controls/mesh, G0-G8 gates, and UID/frame trace in WP-04. |
| [mass-balance.md](mass-balance.md) | `alas-mass`, `alas-payload`, `alas-mission` | Component ledger, all mass states, full CG/inertia tensor, tank/feed/loading closure, calibration and uncertainty in WP-06 and feedback from WP-08. |
| [aerodynamics.md](aerodynamics.md) | `alas-aero`, `alas-stab`, `alas-perf`, `alas-exec` | Decomposed drag, typed case/validity, section/3-D/high-lift/powered/CFD ladder, aero database/cache, benchmarks and UQ in WP-07/WP-09/WP-13. |
| [propulsion-energy.md](propulsion-energy.md) | `alas-prop`, `alas-systems`, `alas-mission`, `alas-perf` | Architecture families, cycle/deck distinction, off-design lapse, installation/OEI, alternative energy/thermal/volume, noise/emissions interfaces in WP-05-WP-07/WP-11. |
| [aircraft-systems.md](aircraft-systems.md) | proposed `alas-systems`, all coupling disciplines | Resource-network contracts, normal/failure schedules, electrical/hydraulic/pneumatic/ECS/thermal/APU/anti-ice/fire/water-waste models, conservation and coupled closure in WP-05/WP-10. |
| [performance-airport.md](performance-airport.md) | `alas-perf`, `alas-prop`, gear/brake owner | Typed runway/airport cases, independent accelerate-stop/go and `V1` search, climb/ceiling/TTC, approach/high lift, runway state, reserves, and ACR/PCR boundary in WP-07. |
| [stability-control.md](stability-control.md) | `alas-stab`, `alas-aero`, `alas-systems`, `alas-struct` | Trim/static/control case matrix, physical OEI authority, dynamics/actuators, gust/envelope, validation/UQ and ranking policy in WP-09. |
| [structures-aeroelasticity.md](structures-aeroelasticity.md) | `alas-struct`, `alas-aero`, `alas-mass` | Load catalogue/paths, sizing/buckling/fatigue proxies, material allowables, structural feedback, modal/aeroelastic ladder and NASTRAN gates in WP-08/WP-13. |
| [optimization-mdo.md](optimization-mdo.md) | `alas-opt`, `alas-pipeline` | Candidate identity, complete stage assessment, mixed architectures, feasibility/Pareto policy, DOE/search/surrogates, cache/parallel, promotion, UQ and V1-V23 evidence in WP-03/WP-12. |
| [digital-thread.md](digital-thread.md) | `alas-types`, `alas-pipeline`, `alas-exec`, `xtask`, report | CPACS authority, freeze/version/unit/frame rules, manifest/evidence graph, reproducible failures/campaigns, and report traceability in WP-01/WP-03/WP-13. |
| [certification-safety.md](certification-safety.md) | proposed `alas-safety`, `alas-systems`, acceptance/report | Operational envelope, FHA/PSSA-style records, safety-driven architecture, hazard screens, compliance matrix, and strict assurance language in WP-02/WP-10. |
| [environment-lifecycle.md](environment-lifecycle.md) | proposed `alas-environment`, mission/propulsion/report | Environmental brief, fuel/species/non-CO2/noise/LCA/energy infrastructure ladder, double-counting control and uncertainty in WP-11. |
| [operations-economics.md](operations-economics.md) | proposed `alas-ops`, mission/payload/airport/report | Scenario/turnaround/reliability/maintainability/airport/DOC/LCC models, data coverage and objective boundary in WP-11. |

The fresh systems synthesis now exists as
[aircraft-systems.md](aircraft-systems.md) and is indexed in the research
[README](README.md). It still needs stable finding IDs and implementation
evidence before a release may call that memo fully integrated.

## 7. Fidelity, validation, and uncertainty policy

### 7.1 Common fidelity ladder

| Level | Meaning | Typical use | Pass boundary |
|---|---|---|---|
| F0 | Schema, dimensional, analytic-limit, and transparent empirical prior | Input lint, architecture rejection, seed generation | Never satisfies a requirement whose minimum fidelity is higher. |
| F1 | Conceptual correlation or algebraic model with validity and uncertainty | Population screening and matching diagrams | May satisfy only explicitly F1 requirements. |
| F2 | Native coupled physics with convergence and mesh/step checks | Inner-loop feasibility and nominal ranking | Requires closed upstream/downstream state and case coverage. |
| F3 | Independent/external solver or calibrated reduced database at matching scope | Promotion, derivative/high-lift/structure refinement | Requires artifact/comparability and paired discrepancy evidence. |
| F4 | Higher-order CFD/FEA/aeroelastic/system simulation with sensitivity and validation | Finalists and critical requirements | Requires model/mesh/time sensitivity and relevant benchmark/domain. |
| F5 | Controlled test, inspection, demonstration, approved data/method, or authority finding | Future development/certification program | Outside ordinary ALAS conceptual execution unless imported as controlled evidence. |

Promotion is requirement-specific. A high-fidelity lift result does not promote
fuel systems, brakes, noise, or flutter. A candidate is promoted when it is
complete/comparable at the current level, hard-feasible or deliberately near a
boundary, diverse/useful for discrepancy calibration, and affordable under the
resource plan. Final winners and their close competitors are re-evaluated at
the final declared level.

### 7.2 Verification, calibration, and validation

- **Code verification:** analytic solutions, conservation, signs/frames/units,
  property tests, parser tests, and deterministic failures.
- **Solution verification:** mesh/panel/time/step/tolerance refinement,
  iteration convergence, conditioning, and numerical uncertainty.
- **Calibration:** named parameters fit on a declared subset; priors and fitted
  deltas retained.
- **Validation:** independent data and observables, geometry/case identity,
  measurement uncertainty, signed error, validation domain, and disposition.
- **Prediction:** explicitly state whether the ALAS candidate interpolates or
  extrapolates from validation evidence.

The reference-aircraft campaign must be unified enough to expose compensating
errors: matching total mass while components are wrong, matching drag while
lift/trim are wrong, or matching range with inconsistent fuel/deck assumptions.
It must also preserve discipline-specific benchmarks such as analytic wings,
airfoil/high-lift cases, engine deck points, field trajectories, network
balance cases, beams/panels, and dynamic modes.

### 7.3 UQ contract

Every uncertain input has distribution/interval, units, source, epistemic or
aleatory class, correlations, applicability, and seed/sampling plan. Use common
random numbers for candidate comparison and retain sample-convergence evidence.
Nominal screening may precede robust screening, but a requirement declared
robust-hard is passed only by its chosen probability/quantile rule. Model-form
alternatives remain an ensemble/discrepancy term rather than being mixed into
input noise. Reports show nominal value, uncertainty interval/quantile,
dominant sensitivities, sample count/confidence, and extrapolation.

## 8. Cache, parallelism, and Rust dependency policy

### 8.1 Cache policy

- Cache at stage granularity using the full identity in Section 3.7.
- Store canonical inputs, outputs, residuals, statuses, raw transcripts,
  runtime/resources, producer versions, and invalidation reason.
- Cache successful and deterministic physically infeasible results.
- Do not persist transient tool/network/license/resource failures as physical
  evidence; use bounded retry with recorded attempts.
- Write into a unique temporary stage directory, verify artifacts/hashes, then
  publish atomically into the run/cache namespace.
- Raw solver artifacts are immutable. Reduced tables and surrogate models have
  separate identities and validation records.

### 8.2 Parallel resource policy

- Candidate and independent load-case evaluation may run in parallel after
  their immutable upstream snapshot exists. Iterations inside one coupled
  closure preserve deterministic dependency order.
- A global broker issues tokens for logical CPU, memory, external processes,
  licenses, GPUs, and heavy I/O. Solver adapters declare worst-case requests.
- Disable or cap nested solver threading so candidate-level parallelism does
  not oversubscribe the machine. Record environment variables that control it.
- Assign immutable sequence indices and merge by input order, never completion
  order. RNG streams derive from run seed plus identity, not thread timing.
- Each external invocation owns a unique workspace and bounded stdout/stderr,
  timeout, cancellation, and cleanup/retention policy.
- Benchmark wall/CPU time, memory high-water mark, cache hit rate, tool
  utilization, and evaluations per fidelity before changing defaults.

### 8.3 Rust libraries

Prefer existing repository primitives until a new dependency demonstrably
reduces risk or complexity. Candidates for a short architecture decision
record are:

| Crate | Possible use | Adoption gate |
|---|---|---|
| `sha2` | portable SHA-256 artifact and identity hashing | Canonical-byte golden vectors and dependency/licence review. |
| `petgraph` | requirement/evidence/system resource DAGs and reachability | Only if typed adjacency maps become materially more complex; deterministic serialization remains ALAS-owned. |
| `indexmap` | stable insertion order for human-facing graphs and manifests | Hash order must still be explicitly canonical, not incidental. |
| `rayon` | ergonomic CPU-bound candidate/case parallelism | Only behind the global resource broker after one/many-worker equivalence and benchmark evidence. Existing scoped-thread behavior remains valid until then. |
| `schemars` plus a JSON-schema validator | generate/audit public manifest and contract schemas | Generated schema must be pinned, reviewed, and migration-tested; Rust types remain authoritative. |

Do not replace mature ALAS optimizers or numerical kernels merely because a
library exists. Compare correctness, determinism, serialization, platform/tool
support, licensing, binary size, and measured performance under the
dependency-justification rule in `CONTRIBUTING.md`.

## 9. Safety, environmental, operational, and claim boundaries

### 9.1 Safety/certification

ALAS may enumerate obligations, build preliminary FHA/PSSA-style architecture
screens, calculate candidate residuals, and retain future verification needs.
It cannot establish certification, compliance, airworthiness, approved data,
or an accepted means of compliance. Live regulation, applicable amendment,
authority, certification basis, conformity, approved plans, and findings remain
external controlled evidence. Safety probabilities, common-cause data, fire/
smoke substantiation, lightning/HIRF, evacuation, ditching, and damage-tolerance
claims are `NotEvaluated` until their specific methods/data exist.

### 9.2 Environment/lifecycle

Fuel burn/energy, species inventories, non-CO2, noise, LCA, SAF/hydrogen/
battery pathways, manufacturing, MRO, and end-of-life have different system
boundaries and evidence. Reports must state functional unit, geography,
time/technology horizon, allocation, energy pathway, procedure, factors,
uncertainty, and double-counting policy. Fuel burn alone is not total climate
or lifecycle impact. Missing current factors, noise tools/calibration, or
novel-energy lifecycle inventories are `NotEvaluated`, not zero impact.

### 9.3 Operations/economics

Aircraft physics may be feasible while operations are unproven. Turnaround,
dispatch, maintenance, crew, schedule, airport/GSE, DOC, LCC, and revenue
depend on named operator/network/station assumptions and often proprietary
data. Costs carry currency, price year, escalation and ownership basis;
probabilities carry source/sample coverage. No public historical rate becomes
a universal default. Business objectives rank feasible candidates and never
repair a physical or safety failure.

## 10. Fresh aircraft-systems primary-source research and parent extension point

> **Clearly marked parent extension section.** The 2026-08-27 synthesis is now
> captured in [aircraft-systems.md](aircraft-systems.md). The parent may add new
> systems primary sources here and to that memo, but must include exact
> citation, access date, rights, local path/hash when retained, applicable
> model/case, and the finding IDs changed by the source. An unregistered source
> cannot silently change a default or validation threshold.

### 10.1 Integrated synthesis

Create a new `alas-systems` crate organized by function and conserved-resource
network rather than by a single weight fraction. The common graph is:

```text
SystemModel {
  components[]: stable ID + type + mass/location/volume + capability,
  ports[]: resource type + direction + units + limits,
  connections[]: source/target + capacity/loss/isolation/routing,
  schedules[]: phase demand and control state,
  load_cases[]: normal/failure/dispatch/environment state,
  results[]: balances + availability + extraction + heat + drag
             + mass/CG + residual/status/evidence
}
```

Resource families: AC/DC electrical power, hydraulic power, pneumatic/bleed
air, cabin air/pressure, thermal liquid/heat rejection, potable/waste fluids,
and fire-suppression agent. Functions include generation, storage, conversion,
distribution, protection/isolation, control, and consumers. Initial subsystems
are electrical generation/distribution/storage, engine/APU start and ground
power, hydraulics and actuation, pneumatic/bleed, ECS/pressurization, thermal
management, ice protection, fire detection/suppression, potable/waste, flight-
control and gear/brake actuation loads, avionics/cabin loads, and health/
maintenance interfaces.

Minimum case matrix: cold ground/start, hot-soak turnaround, engine start,
taxi, takeoff/hot-high, climb with icing, cruise, idle descent, landing, and
turnaround; one generator/source unavailable, one hydraulic circuit
unavailable, pack/bleed/APU/outflow/duct failures, normal-electric/essential-
bus configuration, fire isolation, and anti-ice failure. Each case feeds:

- `alas-prop`: shaft/bleed/electric extraction, APU and installation;
- `alas-mission`: time histories and energy/consumable use;
- `alas-mass`: installed item mass, fluids, CG and inertia;
- `alas-aero`: cooling/ram/vent/icing drag and surface degradation;
- `alas-geom`: equipment, tank, duct, line, heat-exchanger and access volumes;
- `alas-struct`: pressure, equipment, engine/pylon and attachment loads;
- `alas-stab` / `alas-perf`: control, braking, anti-ice and degraded-resource
  availability;
- `alas-safety`: independence, isolation, reachability and failure evidence.

FLOPS systems mass remains a transparent F0/F1 prior until function-level
items replace it. Conservation, capacity, connectivity, priority shedding,
failure isolation, and unmet demand are hard model residuals. OEM maps/masses/
routing, reliability/common-cause probabilities, fire-agent/smoke
substantiation, and icing certification remain unverified or `NotEvaluated`.

### 10.2 Primary-source anchors and additions register

The exact links and evidence boundary are in the systems memo's
[source and evidence register](aircraft-systems.md#2-source-and-evidence-register).
The table below records the integration role and leaves an explicit place for
future parent-added sources.

| Source family | Intended validation use | Integration/addition action |
|---|---|---|
| NASA FLOPS documentation | Level-0 system mass parity and applicability | Preserve current golden behavior and report physical-ledger discrepancy. |
| NASA thermal-management, generalized power-architecture, and 13-bus AC/DC load-flow studies | Network conservation, load flow, thermal-loop mass/power/drag fixtures | Encode exact cases/units/tolerances from the memo; do not generalize one fixture. |
| EASA CS-25 Amendment 28 and current FAA system-safety/powerplant/drain guidance | Air/cabin, electrical, hydraulic, fire, icing and failure-case obligations | Version/applicability map; conceptual screen only, never a compliance finding. |
| Public Airbus A320 hydraulic, ECS/hot-soak, ground-interface, water and waste data | Named-aircraft topology and operating-point fixtures | Preserve aircraft/configuration/date and manufacturer-specific scope. |
| Pratt & Whitney APS3200 public concurrent bleed/90 kVA point | APU simultaneous-load fixture | Treat as one named product/condition, not a generic APU deck. |
| NASA LEWICE 3.2/3.5 | External icing/protection and validation boundary | Pin software/report versions, geometry/cases, validation domain, and limitations. |
| **Parent-added primary source, reserved** | New component map, network, thermal, safety, or validation evidence | Add source ID/link/path/hash/rights, extracted case, uncertainty, finding IDs, and disposition before use. |

### 10.3 Systems research exit

The systems research note exists and is indexed in [README.md](README.md). The
research publication slice is therefore complete. Integration remains open
until retained sources are covered by the repository source/rights/hash ledger
and stable finding IDs map every recommendation to WP-05/WP-10 acceptance
evidence. Until then, no plan or release may say the systems research corpus is
fully integrated.

## 11. Explicit `NotEvaluated` register

The following must remain `NotEvaluated` for requirement satisfaction until
the named evidence exists, even if ALAS can display a proxy or diagnostic:

- clean-wing-derived CLmax, stall, takeoff/landing, or approach values without
  deployed high-lift geometry and applicable aerodynamic evidence;
- powered lift, propulsive interaction, ground effect, or icing degradation
  without a matching geometry/power/ice state and validation domain;
- installed engine performance outside a deck/cycle/calibration envelope,
  especially novel electric, hybrid, hydrogen, open-rotor, or distributed
  architectures;
- true balanced field, accelerate-stop/go, contaminated-runway performance,
  brake energy/thermal/fade/cool-down, tire limits, or reverse-thrust credit
  when their phase models and inputs are absent;
- ACR/PCR or ACN/PCN compatibility when method, gear/tire, pavement, subgrade,
  traffic, and published airport value do not match;
- physical VMC/VMCL/VMCG, OEI controllability, rotation/recovery, crosswind,
  actuator-limited response, handling qualities, or control-law safety when
  only empirical V-speeds or bare derivatives exist;
- full-airframe load coverage, local buckling, joints, fatigue, damage
  tolerance, flutter, divergence, control reversal, flexible gust, or
  aeroservoelastic stability when only the current beam/modal screen exists;
- systems redundancy, independence, load shedding, failure isolation,
  reliability/common cause, fire/smoke, lightning/HIRF, and continued safe
  function without network and evidence coverage;
- evacuation, accessibility, ditching, cargo restraint/fire, and emergency
  equipment compliance beyond geometric/conceptual screens;
- current regulatory or paid-standard clauses that have not been accessed,
  versioned, and mapped to an applicable project basis;
- non-CO2 climate, community noise, lifecycle, SAF/hydrogen/electric pathways,
  and local-air-quality claims without current factors/tools/boundaries;
- turnaround, dispatch reliability, maintenance, crew workload, airport/GSE,
  DOC/LCC, revenue, or schedule robustness without operator-quality scenario
  data and validation;
- a general claim for novel architectures outside the calibration and
  validation set;
- certification, compliance, approval, airworthiness, or operational approval
  in all ordinary conceptual runs.

The report may show a proxy next to these gaps only when it is labeled with its
method, validity, uncertainty, and non-acceptance consequence.

## 12. Data and regulation acquisition backlog

The following acquisition work is a dependency, not a request to invent
defaults:

1. Pin current, legally accessible FAA/EASA/ICAO and airport/pavement versions;
   record paid/citation-only material and obtain authorized access where the
   project requires it.
2. Build open, versioned engine/propulsor deck fixtures and installation data;
   segregate OEM/confidential data and licences.
3. Acquire high-lift, icing, powered-lift, ground-effect, brake/tire, runway,
   and landing-gear validation cases with geometry and conditions.
4. Build a unified conventional reference-aircraft dataset whose geometry,
   mass ledger, engine, mission, aero, stability, field, and structure are
   mutually consistent; do not splice unrelated public numbers invisibly.
5. Acquire system equipment/network and failure data at increasing fidelity;
   mark OEM reliability and common-cause gaps.
6. Curate environmental factors/noise tools and LCA inventories with geography,
   year, energy pathway, licensing, and uncertainty.
7. Acquire operator-quality or explicitly synthetic operations data for
   boarding, turnaround, maintenance, dispatch, costs, schedules, airports,
   and GSE; keep synthetic scenarios visibly synthetic.

Every dataset receives a data card: source/rights/hash, units/frames, revision,
coverage, transformations, missingness, uncertainty, calibration split,
validation split, and prohibited claims.

## 13. Release exit criteria

An integration release exits this plan only when:

- WP-00 repository and CLI gates are green from a pinned revision;
- all shared contracts and manifests pass schema/hash/evidence audits;
- every research finding has a ledger disposition and no item is silently
  omitted;
- required coupling loops converge or reject with retained residual histories;
- optimizer ranking uses the same complete, closed stage contract as reports;
- one/many-worker, fresh/cache, and repeat-seed equivalence tests pass;
- the selected reference and benchmark campaigns reproduce within reviewed,
  source-backed tolerances;
- finalists and close competitors are re-evaluated at the declared final
  fidelity;
- every hard requirement is `Pass`, `Fail`, or `NotEvaluated` with evidence;
  no default or missing value is a pass;
- safety, environmental, operational, and certification language stays inside
  Section 9 boundaries;
- CLI JSON, effective configuration, manifest, CPACS, raw solver artifacts,
  reports, and PNG/SVG figures are reproducibly linked by evidence IDs;
- unresolved data/model gaps are prominently listed in the final report.

The desired outcome is a materially more significant aircraft model: geometry,
accommodation, tanks, gear, propulsion, systems, mass, mission, aerodynamics,
performance, structures, aeroelasticity, stability/control, safety,
environment, and operations describe the same frozen aircraft and exchange
case-specific state. Detail is increased only where the evidence supports it;
honest `NotEvaluated` results are part of the finished engineering product.
