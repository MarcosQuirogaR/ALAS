# ALAS state-of-the-art aircraft-design doctrine

Research synthesis for the requirements-first aircraft-design wizard and staged candidate evaluator.

**Date:** 2026-08-26
**Repository:** ALAS-rust
**Status:** engineering design doctrine; not a certification basis, approved means of compliance, or claim that an ALAS concept is airworthy

## Executive decision

The wizard is a model-based systems-engineering front end to aircraft synthesis. It is not a form that fills a handful of variables before an optimizer runs, and it is not a second aircraft model. It must turn stakeholder intent into a versioned, analysis-ready design study containing:

- a ConOps and named mission scenarios;
- typed technical requirements with units, applicability, policy, provenance, evaluator, verification method, and status;
- explicit architecture choices and a small set of physically meaningful seeds;
- cabin/cargo and ground-operation assumptions;
- coupled geometry, mass, energy, aerodynamics, stability, performance, structures, safety, environment, and operations models;
- a fidelity plan and validity envelope for every result; and
- a reproducible evidence graph that explains how the candidate was produced and why it was accepted or rejected.

The optimizer begins only after the selected architecture can be materialized and its disciplines have defined interfaces. Continuous optimization is a search within a coherent aircraft family; it is not architecture discovery by arbitrary floating-point values.

The strongest cross-discipline rule is:

> No scalar result is meaningful without its scenario, load case, model fidelity, validity domain, and evidence status.

This applies equally to range, passenger capacity, lift-to-drag, static margin, OEI ceiling, ACN, noise, cost, and structural stress.

## 1. What the research established

The detailed findings are in the 15 discipline notes indexed by [research README](README.md). Their common conclusions are:

1. Requirements engineering, architecture, and verification form an iterative loop. Stakeholder needs, ConOps, technical requirements, architecture, analysis, and validation must be traceable in both directions.
2. Architecture is a mixed discrete/continuous problem. Deck count, propulsion family, engine count, energy carrier, cargo system, high-lift concept, and landing-gear arrangement must be selected or enumerated before continuous shape search.
3. Accommodation is primary aircraft geometry. Seats, aisles, exits, galleys, lavatories, overhead bins, crew stations, accessibility, baggage, ULDs, cargo doors, and loading sequences change fuselage, mass, CG, structure, turnaround, and safety.
4. Preliminary sizing is an inner closure problem. Weight, fuel/energy, wing area, thrust/power, volume, and performance must converge before an outer optimizer can compare shapes fairly.
5. Every discipline needs a fidelity ladder. A fast proxy may screen a design, but its result must not be presented as a higher-fidelity or certification result.
6. Feasibility precedes objective ranking. A low-drag candidate that cannot carry the required payload, evacuate, remain controllable, or close its structure is not a better aircraft.
7. Certification and safety must be represented as obligations, failure conditions, and evidence gaps. A conceptual screen must never emit a generic `certified = true` claim.
8. Environmental and operational effects belong in the concept trade space. Fuel burn alone misses noise, non-CO2 effects, turnaround, dispatch reliability, maintenance access, and lifecycle consequences.
9. The digital thread is part of the physics. Coordinate frames, units, UIDs, tool versions, solver decks, hashes, convergence, and transformations determine whether two results are comparable.

## 2. Canonical study model

The existing `DesignBrief` is the right user-facing starting point. The mature study should extend it through adjacent typed records rather than duplicating fields in the wizard, optimizer, or report layer.

```text
DesignStudy
├── study_id, revision, author, created_at
├── intent
│   ├── mode: improve_existing_baseline | ground_up_synthesis
│   ├── baseline_reference
│   └── ConOps and stakeholder needs
├── requirements: RequirementRecord[]
├── scenarios: Scenario[]
├── architecture_space: ArchitectureFamily[]
├── selected_architecture: ArchitectureSeed
├── design_vector and bounds
├── accommodation and loading policies
├── fidelity_plan
├── solver/run options
└── provenance and evidence graph
```

### 2.1 Requirement record

```text
RequirementRecord {
    id,
    statement_kind: stakeholder_need | technical_requirement | derived_requirement
                     | certification_reference | operational_constraint,
    quantity: value + unit + tolerance or interval,
    relation: minimum | maximum | target | equality | categorical,
    applicability: scenario/load-case identifiers,
    policy: hard | soft | objective | diagnostic,
    source: user | preset | derived | regulation | research | assumption,
    rationale and parent links,
    evaluator_id,
    verification_method: analysis | inspection | test | demonstration | review,
    assurance_scope,
    status: draft | screened | verified_by_analysis | evidence_pending
            | inconclusive | not_evaluated | not_applicable_with_rationale,
    evidence_ids
}
```

The current per-field policy fields in `DesignBrief` are useful compatibility inputs, but a general requirement registry is needed for cross-field semantics, derived values, certification references, and diagnostics. In particular, `diagnostic` should be an explicit policy/status path rather than being inferred from a missing evaluator.

### 2.2 Scenario and load case

```text
LoadCase {
    id,
    flight_phase,
    mass_state: MTOW | MLW | MZFW | OEW | custom,
    payload_layout and loading sequence,
    fuel/energy state and reserve state,
    centre_of_gravity and inertia assumption,
    atmosphere: model, altitude, temperature, pressure, wind,
    configuration: gear, flaps, slats, spoilers, doors, high-lift state,
    propulsion: throttle/power, engine state, bleed, failure state,
    controls/trim/actuator limits,
    airport/runway/ground condition when applicable,
    acceptance criterion and residual scale
}
```

Minimum families include: design cruise, maximum-payload cruise, MTOW take-off, hot-day take-off, balanced-field or specified TOFL, MLW landing, approach, climb to ICA, TTC, maximum cruise altitude, OEI climb/ceiling, VMO/MMO, gust and maneuver V-n points, forward/aft CG, fuel-transfer/loading sequence, emergency landing, evacuation/egress, cargo loading, turnaround, dispatch, and environmental operating scenarios.

“Design payload” is a load case. “Maximum passenger capacity” is a capacity calculation. “Maximum cargo” is a capacity requirement. “Cabin could fit 180 seats” does not prove that 180 passengers were loaded in the design mass/CG case.

### 2.3 Stage result and evidence

Every discipline should return a common envelope:

```text
StageResult<T> {
    stage_id,
    status: accepted | rejected | not_evaluated | inconclusive | failed,
    value: Option<T>,
    residuals: ConstraintResidual[],
    warnings,
    evaluator_version,
    fidelity_level,
    validity_domain,
    input_hashes,
    evidence_ids,
    uncertainty_summary
}
```

`failed` means the evaluator did not produce trustworthy physical evidence. It is not the same as a physical miss. A candidate with a failed high-fidelity tool must not be silently promoted using a stale or fabricated result.

## 3. Wizard doctrine

The wizard should be a visual, progressive-disclosure journey. It should keep one shared state with expert pages, update the existing preview surfaces, and make assumptions visible at the moment they matter.

### Step 0 - Intent, ConOps, and assurance scope

- Choose improve-existing-baseline or ground-up synthesis.
- If a baseline is selected, load its geometry, engine, cabin, mass, requirements, calibration, and evidence as a named comparison.
- If synthesizing, choose role, architecture family, propulsion/energy family, deck arrangement, technology assumptions, and intended operation.
- Select a certification/reference profile or explicitly mark the study as non-certification conceptual work.
- Capture stakeholders, mission use, airport context, and success measures.

The baseline is evidence and a starting point, not an invisible replacement for the user brief.

### Step 1 - Mission and operational scenarios

Collect range, reserve convention, route/airports, cruise Mach/altitude, climb assumptions, OEI policy, runway conditions, and environmental conditions. Show the route as provisional until the mission solver has run. A requirement such as “OEI ceiling” must display its mass, atmosphere, configuration, engine state, and residual climb criterion.

### Step 2 - Accommodation and loading

Collect design and maximum passengers, mass basis, design and maximum cargo, deck allocation, cabin architecture, exits, galleys, lavatories, crew stations, overhead-bin policy, lower-hold ULD type/count, cargo doors, accessibility, loading order, and capacity policy. Render main deck, upper deck, holds, monuments, seat allocation, exits, and unfilled capacity separately.

The wizard must distinguish requested, placed, available, and unfilled capacity. It must not hide overhead compartments or assume that a seat-count preset represents a verified evacuation or loading result.

### Step 3 - Architecture and technology

Enumerate a small supported set of discrete architecture seeds. Expose why a seed was retained or rejected: energy volume, thrust/power, runway, span, cabin, safety, structural, thermal, or airport incompatibility. Do not encode categorical choices as arbitrary continuous numbers.

### Step 4 - Shape, packaging, and balance

Edit high-leverage variables such as span, chords, sweep, twist, wing position, fuselage length/diameter, tail volume, gear location, tank volume, nacelle position, airfoil shape, and high-lift configuration. Show planform, three-view, 3D exterior, cabin, cargo, and CG changes immediately. Enforce coupled bounds and geometric validity before solver launch.

### Step 5 - Feasibility, fidelity, and evidence review

Show the hard/soft/objective/diagnostic policy for each requirement, the evaluator that owns it, the selected fidelity, the validity envelope, and known gaps. Group warnings by owner and let the user navigate back to the responsible step. A missing translator is a visible `not_evaluated` or `diagnostic` state, never a pass.

### Step 6 - Freeze, review, and launch

Present a read-only concept brief containing all requirements, cases, architecture choices, initial vector, bounds, solver options, fidelity plan, and evidence references. On launch, freeze a deep copy with revision and hashes. Later UI edits must not mutate the worker configuration.

## 4. Discipline model and ordering

The candidate evaluator should order work from cheap/local to expensive/global while preserving the coupling loops each discipline needs.

| Stage | Primary model | Important outputs | Typical early gate |
|---|---|---|---|
| 0. Materialize architecture | Discrete catalogue and seed generator | Architecture ID, assumptions, seed provenance | Unsupported topology or missing interface |
| 1. Preliminary sizing | Class-I/II mass, Breguet/mission, matching diagram, volume | MTOW/OEW/fuel, wing area, thrust/power, energy/tank closure | Non-convergent or impossible mass/volume closure |
| 2. Geometry and packaging | Parametric geometry, cabin/cargo, gear, tanks | Valid geometry, occupied/available capacity, mesh/UID map | Self-intersection, inaccessible volume, missing capacity |
| 3. Mass and balance | Component buildup, loading sequences, fuel transfer | Mass properties, CG envelope, inertia, uncertainty | CG outside envelope or nonphysical mass |
| 4. Propulsion and aero | Cycle/deck, installation, drag buildup, VLM/lifting-line, airfoil/high-lift | Net thrust/power, TSFC/energy use, forces/moments, validity | No required operating point, stall/drag/engine mismatch |
| 5. Stability and control | Neutral point, trim, control authority, modes, gust | Static margin, trim, V-n, dynamic metrics, control margins | Untrimmed case, insufficient authority, invalid derivatives |
| 6. Mission and airport performance | Segment integration and field-performance cases | Range, reserve, climb/TTC/OEI, TOFL/landing, approach, VMO/MMO, ACN | Hard requirement shortfall or unevaluated required case |
| 7. Structures and aeroelasticity | Load cases, beam/wingbox/FEA, buckling, flutter | Stress, margins, mass feedback, frequencies, flutter/divergence | Strength/stiffness/buckling/flutter failure |
| 8. Safety and certification screen | FHA/PSSA-style architecture filter, failure conditions | Hazard classes, independence, redundancy, compliance/evidence matrix | Implausible catastrophic single-failure response |
| 9. Operations, environment, economics | Scenario-based turnaround, DOC/LCC, dispatch, noise/LCA | Cost, robustness, maintenance, emissions/noise, lifecycle metrics | Policy-specific target failure; usually not a physical reject |
| 10. Rank and promote | Feasibility-first MDO and fidelity promotion | Pareto set, residuals, uncertainty, finalist plan | No hard-feasible candidate or insufficient evidence |

### 4.1 The inner sizing loop

For each architecture seed, iterate until the mass/energy/geometry/performance closure is converged or the seed is rejected:

```text
payload + mission + reserve
        -> initial mass/fuel/energy estimate
        -> wing area and thrust/power
        -> geometry, cabin, tanks, gear, and structural mass
        -> aero/propulsion performance
        -> mission fuel and field performance
        -> updated mass, CG, and loads
        -> convergence or typed rejection
```

The outer design search must not compare one candidate after closure and another with stale fuel or structural mass.

### 4.2 Cross-discipline couplings that cannot be postponed

- Cabin and cargo dimensions drive fuselage wetted area, pressurized volume, structural mass, doors, emergency egress, loading time, and CG.
- Wing area and loading drive stall, take-off/landing, cruise drag, fuel volume, wingbox loads, and structural mass.
- Propulsion placement drives nacelle/pylon drag, mass, noise, local flow, engine-out yaw/roll, wing loads, and cabin noise.
- Fuel/energy storage drives mass, volume, CG travel, thermal management, fire safety, and turnaround/infrastructure assumptions.
- Tail volume and CG envelope drive trim drag, control authority, stability, structure, and handling qualities.
- Structural mass and stiffness feed back into mission fuel, performance, flutter, and optimization ranking.
- Airport and operations assumptions drive span, gear, brakes, turnaround, cargo doors, and economic value.

## 5. Fidelity ladder

Fidelity is a property of each evaluator result, not a single setting for the whole aircraft.

| Level | Purpose | Examples | Allowed claim |
|---|---|---|---|
| L0 | Data and topology validity | Units, frames, finite values, connected geometry, capacity bookkeeping | Input is structurally interpretable |
| L1 | Fast conceptual screen | Class-I/II mass, Breguet, matching diagram, algebraic OEI, analytic controls, proxy noise/cost | Plausibility or early rejection |
| L2 | Native ALAS disciplinary analysis | VLM/lifting-line, drag buildup, cycle/mission, stability, wingbox, route solver | Preliminary analysis inside stated validity domain |
| L3 | External or validated higher fidelity | AVL/VSPAERO/MSES, engine decks/maps, refined high-lift, Nastran/FEA, aeroelastic models, evacuation/operations simulation | Higher-fidelity evidence for the selected case; still not certification by itself |
| L4 | Test/authority evidence | Wind-tunnel/flight/ground test, conformity, approved methods and findings | Outside the ordinary conceptual ALAS claim; retained as referenced evidence |

Promotion from L1/L2 to L3 should be triggered by finalist status, sensitivity, extrapolation outside a validity envelope, a close constraint, or a known model-form risk. High fidelity should not be spent on candidates that fail at L0/L1.

## 6. Feasibility and ranking contract

The existing residual vocabulary in `alas-opt` is a good foundation. Keep physical residuals separate from evaluator failures.

```text
ConstraintResidual {
    id/name,
    actual, target, direction,
    raw_residual, positive_scale,
    normalized_violation,
    policy: hard | soft | objective | diagnostic,
    kind: physical | evaluation_failure,
    load_case, evaluator, fidelity, evidence_status
}
```

Positive residual means violation. Hard physical violations and evaluator failures are not interchangeable. Soft residuals remain visible but do not block feasibility. Objectives rank only after required hard cases are evaluated.

Recommended ordering:

1. valid materialization and completed required stages;
2. zero hard requirement violations;
3. lower severity of hard physical residuals for near-feasible diagnostics;
4. lower soft-target penalty;
5. objective/Pareto ranking;
6. uncertainty and robustness preference among otherwise comparable designs.

An evaluator failure must retain its reason, inputs, tool identity, and partial outputs. A final “no feasible design” result should include evaluated-candidate count, grouped rejection reasons, near-feasible candidates, and unevaluated requirements.

## 7. Safety, certification, and environmental boundaries

### 7.1 Certification and safety

The wizard may select a regulatory reference profile and create preliminary screening obligations. It may not issue a certification finding. A safety record should retain the function, failure condition, severity class, exposure/latency, redundancy and independence assumptions, common-cause threats, crew/maintenance response, evaluator, and evidence status.

Use `preliminary_screened`, `evidence_pending`, `diagnostic`, `inconclusive`, and `not_applicable_with_rationale`. Do not use a generic `certified` Boolean.

Hard early screens may include impossible egress geometry, no credible response to a catastrophic single failure, impossible control authority, absent fire/energy containment volume, or a selected operation outside the declared envelope. Probability and compliance claims require later data, accepted methods, and authority context.

### 7.2 Environment and lifecycle

Use a tiered model: fuel/energy burn and mass first; then emissions/noise proxies; then route and operating scenario aggregation; then lifecycle/manufacturing/end-of-life and non-CO2 analysis where the required data exist. Keep climate metrics, noise, cost, and lifecycle scores separate to prevent double counting. Environmental values may be hard requirements only when a defensible evaluator and boundary are selected; otherwise they remain soft objectives or diagnostics.

## 8. Digital-thread invariants

Every run should be reproducible from a frozen study snapshot.

Minimum manifest fields:

```text
RunManifest {
    run_id, study_id, study_revision,
    source_revision, tool/executable identities,
    schema versions and hashes,
    input artifact hashes,
    design vector, bounds, seed, architecture ID,
    load-case catalogue and fidelity plan,
    stage graph and status summary,
    output artifact hashes,
    requirement/evidence links
}
```

CPACS should be the aircraft/configuration interchange boundary where supported. Solver decks, meshes, analysis tables, and report figures are derived artifacts with their own frame, unit, topology, and hash contracts. Record exact CPACS schema/revision, UID mappings, coordinate systems, SI conventions, transformations, tool versions, parser status, convergence, and warnings.

Comparing two solver results requires more than matching file extensions. At minimum compare reference area/length, moment origin, axes, Mach/Reynolds, control state, load case, mesh/geometry hash, solver identity, and quantity definitions.

## 9. Verification and validation strategy

Use four layers:

1. **Contract tests:** units, ranges, cross-field consistency, enum semantics, load-case completeness, deterministic serialization, and frozen worker configuration.
2. **Discipline parity tests:** existing ALAS golden fixtures and named presets remain unchanged when the new path is not selected.
3. **Model validation tests:** analytic limits, benchmark cases, mesh/time/frequency convergence, external-tool comparisons, and sensitivity/uncertainty campaigns.
4. **Use validation:** stakeholder, operator, cabin, maintainability, airport, and environmental scenario reviews. A converged numerical answer is not proof that the concept satisfies intended use.

Every requirement needs an evaluator, verification method, acceptance criterion, evidence status, and owner. “Not yet modeled” is a useful outcome because it exposes work rather than hiding it.

## 10. Recommended implementation sequence

### Phase 0 - Make evidence first-class

- Add a requirement registry and generalized policy/status vocabulary beside the current `DesignBrief`.
- Add named scenarios/load cases and evaluator IDs.
- Add a run manifest/evidence record with hashes and validity envelopes.
- Preserve current parity paths and keep regulatory documents reference-only unless the source terms allow local retention.

### Phase 1 - Close the architecture and sizing loop

- Implement explicit architecture seed records and discrete enumeration.
- Add mass/fuel/energy/volume closure with uncertainty ranges.
- Add feasibility gates for topology, capacity, geometry, and basic matching-diagram limits.

### Phase 2 - Make accommodation authoritative

- Resolve cabin/cargo from the same geometry used by analysis.
- Model seats, aisles, monuments, exits, overhead bins, holds, ULDs, doors, loading sequence, and CG contribution.
- Return requested/placed/available/unfilled capacity by deck and hold.

### Phase 3 - Complete coupled native discipline evaluation

- Promote load-case-aware propulsion and mission evaluation.
- Couple aero, mass/CG, stability/control, field performance, and structural mass feedback.
- Attach typed residuals and stage evidence to every result.

### Phase 4 - Add fidelity promotion and uncertainty

- Define model validity domains and promotion triggers.
- Add deterministic caching keyed by study revision, architecture, vector, load case, model revision, and tool identity.
- Add DOE/sensitivity, robust margins, and finalist L3 checks.

### Phase 5 - Add operational, environmental, and safety trade space

- Add turnaround/boarding, dispatch, maintenance, lifecycle cost, noise, fuel/energy climate, and safety-screen records.
- Keep these values separate from physical feasibility until their evaluator and policy are explicit.

### Phase 6 - Mature external evidence

- Integrate validated engine decks, high-lift/CFD, refined evacuation/operations models, FEA/aeroelasticity, and test evidence behind explicit tool contracts.
- Report the exact evidence boundary. Never collapse preliminary screening into certification.

## 11. Definition of done for a serious design wizard

A wizard launch is design-ready only when:

- intent, ConOps, architecture, requirements, scenarios, and assurance scope are present;
- all required fields are unit-valid and cross-field consistent;
- every hard requirement has a named evaluator or is explicitly blocked as not evaluated;
- the architecture seed closes preliminary payload, mass, energy, volume, and basic performance;
- cabin/cargo capacity is solved from analysis geometry and loading policy;
- the selected fidelity is inside its validity envelope or the result is marked extrapolated;
- the candidate can be frozen and reproduced from its manifest;
- failed stages produce typed diagnostics and retain partial evidence;
- results distinguish hard feasibility, soft preference, objective, diagnostic, and certification evidence; and
- the report shows what was checked, what was not checked, and what must happen next.

This is the standard the original requirements-first brief should be judged against. The existing brief supplies the interaction skeleton; these discipline notes supply the engineering depth behind each step.
