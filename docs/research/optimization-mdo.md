# Optimization and MDO research baseline for ALAS

Status: completed research/design note, resumed from saved workspace

Date: 2026-08-26

Scope: multidisciplinary aircraft conceptual optimization, mixed
discrete/continuous architecture search, feasibility handling, multiobjective
methods, design-space parameterization, DOE, surrogate management,
multifidelity, uncertainty, sensitivity, caching, parallel evaluation, and
termination evidence.

Evidence basis: the local PDF packet in bib/optimization-mdo and the existing
ALAS requirements and optimization implementation. At resume time the target
optimization-mdo directory was absent. No new literature search was run. The
directory was populated only by copying PDFs that were already present in the
repository's categorized bibliography directories. The source URLs, access
conditions, byte counts, page counts, and SHA-256 values below are the
provenance ledger for the resulting packet.

## Executive decision

ALAS should use an explicit outer architecture catalogue plus an inner
bounded continuous search. The continuous optimizer must not invent integer
architecture topology by rounding a vector. Each candidate should be evaluated
through a staged, typed, cacheable fidelity funnel and ranked lexicographically
by comparability, required-stage completion, hard feasibility, residual
severity, soft-target penalty, and only then ordinary objectives. A robust
screen should be an explicit promotion stage: nominal feasibility is not a
claim of reliability under uncertain inputs.

The practical first implementation is:

1. Normalize and validate a requirements-first DesignBrief.
2. Enumerate or generate a small set of supported ArchitectureKey values.
3. Reject architecture seeds that cannot close basic geometry, payload, mass,
   volume, propulsion, or mission requirements at low fidelity.
4. Generate a Latin hypercube population for each surviving architecture.
5. Run feasibility-first differential evolution or CMA-ES for one objective,
   and constrained NSGA-II for a genuine multiobjective study.
6. Cache stage results by a complete canonical evaluation key and evaluate
   independent candidates in parallel with stable result ordering.
7. Promote nominally feasible finalists through higher-fidelity aero,
   performance, structure, and uncertainty scenarios.
8. Report the Pareto set, constraint margins, uncertainty statistics,
   convergence evidence, rejected-candidate groups, and unresolved
   requirements.

This matches the requirements-first contract already documented in
docs/REQUIREMENTS_FIRST_AIRCRAFT_DESIGN.md and the current foundations in
crates/alas-opt. It also preserves the central lesson of the NASA, DLR, and
academic evidence: the optimizer is only as trustworthy as the definition of
the candidate, the discipline interfaces, the failure semantics, and the
evidence attached to each result.

## 1. State of the art and method selection

### 1.1 Method choice by problem signal

| Problem signal | Evidence and strengths | ALAS recommendation |
|---|---|---|
| Unknown feasible region and mixed architecture | Priem et al. use a mixed constrained black-box Bayesian method with an uncertainty-aware upper-trust-bound feasibility criterion [O4]. DLR collaborative configuration work varies subsystem choices as well as continuous sizing variables [D2]. | Enumerate a supported architecture catalogue first. For expensive black-box refinement, use a separate surrogate for feasibility and objective, with a feasibility-aware acquisition rule. |
| Many cheap or moderate-cost continuous evaluations | Latin hypercube initialization is used in the current ALAS optimizer and in NASA wingbox/UQ studies [N2, N3]. It spreads samples across each marginal interval but does not guarantee feasible points. | Use seeded LHS for each architecture and fidelity. Preserve strata and seed in the run record. |
| Non-smooth, multimodal continuous sizing | Feasibility-first DE is robust to non-smooth constraints and does not require derivatives. The current alas-opt implementation has LHS initialization, seeded randomness, batch evaluation, and a feasibility-first method [A1]. | Default global method after architecture selection: feasibility-first DE. Use bounded variables and a fixed random seed for reproducibility. |
| Continuous variables with locally useful covariance adaptation | CMA-ES adapts a covariance model and is useful when the local landscape is non-separable or differently scaled. It remains a continuous method, not a categorical architecture solver. | Offer bounded CMA-ES as a portfolio method for a fixed architecture or a warm start from the best DE candidates. |
| Two or more objectives | NSGA-II preserves a population and uses nondominated sorting and crowding. The current implementation uses constrained dominance: feasible dominates infeasible, and lower aggregate violation dominates larger violation among infeasible points [A2]. | Use constrained NSGA-II for cost, fuel, emissions, noise, mass, or field-performance trade studies. Keep one Pareto front per architecture until a cross-architecture decision policy is applied. |
| Expensive high-fidelity black-box analyses | OpenMDAO documents DOE, surrogate models, adaptive sampling, and optimization in a reusable modular workflow [N1]. Priem et al. explicitly model feasibility uncertainty [O4]. | Use a trust-region or Bayesian surrogate only after a valid initial design set exists. Fit constraints separately from objectives and validate held-out points before promotion. |
| Smooth coupled aero-structural final refinement | OpenAeroStruct provides VLM plus beam/wingbox models and coupled adjoint derivatives [D3, D4]. Thelen et al. show that a high-fidelity gradient direction with a low-fidelity line search can reduce high-fidelity evaluations for some aero shape problems [N4]. | Use gradient or adjoint refinement only for a fixed architecture and a differentiable fidelity. Do not use it as the first mixed-topology search. |
| Nested discrete layout and continuous sizing | Stanford et al. use an outer nongradient topology/layout search and an inner gradient-based sizing optimization [N2, N3]. | Keep topology/layout outside the inner sizing solver. The same pattern applies to architecture outside geometry/mass refinement. |
| Uncertain inputs and model form | NASA design-under-uncertainty work propagates input/model uncertainty and uses analytic derivatives for efficient confidence intervals [N5]. The NASA wingbox study uses sampling-driven polynomial chaos at the outer topology level [N3]. | Promote finalists to nominal plus explicit uncertainty scenarios. Record distributions, correlations, sample set, confidence intervals, and model-form assumptions. |

### 1.2 What is and is not an algorithmic guarantee

No method in this packet guarantees a global optimum for the real aircraft
problem. LHS improves coverage, DE and CMA-ES explore nonlinear spaces,
NSGA-II approximates a Pareto front, and Bayesian optimization can reduce
expensive evaluations when the surrogate is calibrated. None of them repairs a
bad parameterization, missing constraint, non-deterministic discipline, or
untracked unit/frame mismatch.

The current alas-opt product methods are useful starting points:
feasibility_first_de, nsga2, cma_es, and turbo_1. The current turbo_1
implementation is a bounded local radial-basis surrogate with feasibility
offsets. It is not a full Gaussian-process TuRBO implementation. It must
therefore be described as an ALAS trust-region surrogate method, not as a
claim of reproducing the TuRBO paper.

## 2. Aircraft MDO architecture lessons

### 2.1 NASA and OpenMDAO

The NASA OpenMDAO framework paper presents a modular, reconfigurable,
reusable framework for multidisciplinary analysis and optimization. It
describes workflow patterns including multidisciplinary feasible, individual
discipline feasible, collaborative optimization, BLISS, design of
experiments, surrogate models, gradient and nongradient optimization, and
adaptive sampling [N1]. The design lesson for ALAS is not to reproduce
OpenMDAO's Python API. It is to make discipline contracts explicit and
composable:

- Each discipline consumes a typed snapshot and emits values, status,
  residuals, provenance, and a fidelity identifier.
- Coupling variables and convergence state are part of the evaluation record.
- A failed analysis is a typed failure, not an arbitrary large objective.
- Surrogate training data must distinguish valid, infeasible, failed, and
  incomparable evaluations.
- The workflow can change fidelity without changing the requirement and
  ranking semantics.

NASA's N+3 trade studies combine first-principles and empirical methods for
nontraditional configurations and emphasize the limits of transferring
correlations outside their calibration domain [N6]. The FLOPS weights method
is a useful baseline for conceptual Class-I/Class-II weight estimation [N7],
but its empirical domain and technology assumptions must be recorded as part
of the model identity.

NASA's VIPER work demonstrates an integrated conceptual MDAO workflow for
supersonic X-plane studies using OpenVSP and other discipline analyses [D1].
It supports the ALAS pattern of separating geometry creation, analysis
execution, and result aggregation while retaining a common conceptual
vehicle definition.

### 2.2 DLR, CPACS, TiGL, and AGILE

The DLR CPACS work presents a common aircraft data language and tool
interoperability pattern [D5]. TiGL creates parametric three-dimensional
aircraft geometry from CPACS and can provide CAD, visualization, and
analysis-oriented representations [D6]. The AGILE configuration studies
show system-driven collaborative configuration optimization, including
architecture choices, geometry, and subsystem interactions [D2, D7].

ALAS should use the same separation of concerns:

- DesignBrief is the requirements authority.
- ArchitectureKey is the discrete concept authority.
- A frozen design snapshot is the analysis input authority.
- Geometry, meshes, solver decks, and reports are derived artifacts with
  their own hashes.
- CPACS, when imported or exported, is an interoperability representation,
  not a replacement for requirement policy or solver evidence.
- UIDs, units, coordinate frames, schema/version, process status, and source
  hashes must be preserved.

The CPACS central-model argument also explains why a direct all-to-all
discipline interface is risky: a common exchange model avoids each discipline
having a bespoke translation relationship with every other discipline. It
does not remove the need for validation of mappings, frames, units, and
semantic identity.

### 2.3 OpenVSP and OpenAeroStruct

OpenVSP provides parametric geometry for conceptual aircraft design [D8].
Its degenerate geometry forms support different analysis abstractions while
retaining aggregate geometric properties [D9]. These are good references for
generating consistent surface, plate, stick, and point representations from a
single geometry authority.

OpenAeroStruct couples a vortex-lattice aerodynamic model with a
one-dimensional beam/wingbox structural model and provides coupled adjoint
derivatives [D3]. Its wingbox paper describes a low-fidelity, low-cost model
that is useful for early aerostructural sizing and for comparing low- and
higher-fidelity analysis [D4]. The correct ALAS interpretation is:

- Use low-fidelity coupled analysis to screen and shape the design.
- Preserve explicit geometry and fuel-volume constraints.
- Promote finalists before claiming stall, viscous, junction, aeroelastic,
  manufacturing, or certification evidence.
- Store the solver/model version and mesh/discretization settings in the
  cache key and result provenance.

### 2.4 LAMBDA and modular aircraft conceptual design

The LAMBDA framework separates requirements, weights, sizing, geometry,
aerodynamics, engine, performance, cost, emissions, and optimization modules
and permits different fidelity levels and external tools [N8]. Its case study
reports a case-specific operating-cost improvement for a truss-braced-wing
configuration; that number is evidence for the study, not a general ALAS
performance guarantee.

The ALAS architecture should adopt the modular boundary and make a
requirements-first layer explicit. The optimizer must not become the place
where missing requirements, architecture semantics, or physical closure are
silently invented.

## 3. Design-space parameterization and mixed architecture search

### 3.1 Outer discrete architecture

Use an explicit immutable key for choices that change topology, interfaces, or
the meaning of the continuous vector. A first ALAS ArchitectureKey can
contain:

    role: passenger | freighter | mixed
    deck_count: one | two
    propulsion_count: one | two | four
    propulsion_family: supported family identifier
    cabin_or_container_layout: supported layout identifier
    wing_configuration: supported configuration identifier
    landing_gear_layout: supported layout identifier
    fuel_or_energy_system: supported system identifier

Only supported catalogue values should be accepted. Unsupported values should
produce an explicit architecture rejection or unresolved result. Do not
encode an integer in a real-valued optimizer and round it after mutation:
rounding creates discontinuities, duplicates, hidden bias, and ambiguous
cache keys.

Architecture generation can be:

- Explicit enumeration for a small catalogue.
- Rule-based seed generation followed by preliminary closure checks.
- A discrete evolutionary or Bayesian outer loop when the catalogue becomes
  large, with one typed categorical value per field and a deterministic
  decoder.

The first implementation should enumerate the supported catalogue and retain
the rejection reason for every seed. This gives the user a useful explanation
when no concept is feasible.

### 3.2 Inner continuous vector

The continuous vector should use stable semantic IDs, canonical ordering, and
dimensionless normalized values. Examples include:

- wing area, aspect ratio, sweep, taper, dihedral, twist control points,
  thickness or radius controls;
- fuselage length, diameter, cabin/hold allocation, door and cargo
  dimensional controls;
- engine diameter, thrust or power rating, installation position;
- fuel or battery volume, reserve sizing, and energy-management controls;
- tail volume, control-surface size, and trim allocation;
- structural sizing variables only after the topology is frozen.

Use low-order splines or piecewise stations for smooth geometry distributions.
OpenAeroStruct's separation between B-spline control points and analysis mesh
density is a useful pattern [D3, D4]. The mesh density is a fidelity or
discretization setting, not a new design degree of freedom unless it is
explicitly part of a numerical study.

Every variable needs:

1. A stable semantic ID and unit.
2. A physical lower and upper bound.
3. A normalized mapping and inverse mapping.
4. A source of the bound, such as requirement, architecture rule, or
   technology assumption.
5. A differentiability/discontinuity flag.
6. A group for scaling, sensitivity, and reporting.
7. A fixed/derived/active status for the current architecture.

### 3.3 Identifiability and dimension control

The optimizer should not be asked to identify variables that the available
analyses cannot distinguish. Before opening a high-dimensional search:

- Remove derived variables and duplicate parameterizations.
- Freeze controls whose effect is below a declared sensitivity threshold.
- Check the local output Jacobian rank or singular values for the selected
  objectives and hard residuals.
- Report highly correlated columns, condition number, and active bounds.
- Use a low-order geometry basis before adding local shape controls.
- Keep variables with different physical effects separate even when their
  numerical scales are similar.

A design that is numerically optimal but unidentifiable should be reported as
an equivalence class or a weakly determined region, not as a precise
engineering conclusion.

### 3.4 Architecture seed screening

For each ArchitectureKey, run a deterministic low-cost closure sequence:

1. Materialize geometry and check topology, intersections, volume, and bounds.
2. Check payload/cabin/hold accommodation and door/container rules.
3. Close Class-I or Class-II mass and center-of-gravity estimates.
4. Check fuel/energy volume and basic propulsion compatibility.
5. Close mission range, reserve, climb, ceiling, and field-performance
   proxies as applicable.
6. Record hard residuals, soft residuals, and the dominant rejection group.

The seed screen is not the final optimizer. It prevents spending high-fidelity
evaluations on an architecture that cannot satisfy basic conservation,
geometry, or mission closure.

## 4. Implementable candidate-evaluation contract

The following is a proposed contract. It is intentionally close to the
existing alas-opt types so the research recommendation can be implemented
incrementally without changing every discipline at once.

### 4.1 Canonical identity

    struct CandidateKey {
        requirements_snapshot_sha256: Hash256,
        architecture_id: ArchitectureId,
        continuous_vector_hash: Hash256,
        fidelity_plan_id: FidelityPlanId,
        scenario_plan_id: ScenarioPlanId,
        evaluator_version: String,
        toolchain_id: String,
    }

The continuous vector hash is computed after unit normalization, canonical
variable ordering, bound projection, and deterministic floating-point
serialization. NaN, infinity, signed zero, and out-of-bound values must have
defined handling. A cache hit is valid only when every field that can change
the result is included.

### 4.2 Stage and status types

Each stage returns a structured result:

    enum StageStatus {
        NotRun,
        Succeeded,
        PhysicallyInfeasible,
        SolverFailed,
        ToolUnavailable,
        TimedOut,
        MalformedOutput,
        NumericalFailure,
        Cancelled,
    }

    struct StageResult {
        stage_id: StageId,
        status: StageStatus,
        fidelity_id: FidelityId,
        values: Map<ValueId, Quantity>,
        residuals: Vec<ConstraintResidual>,
        warnings: Vec<Diagnostic>,
        evidence: EvidenceRef,
        elapsed_ms: u64,
        cache: CacheState,
    }

The distinction between a physical miss and an analysis failure is essential.
A candidate with an undersized wing is a comparable physical design. A
candidate for which the solver returned a malformed file is not equivalent to
an undersized wing. Both must be recorded, but they must not be conflated in
the ranking or surrogate data.

The minimum ordered stages are:

| Stage | Required output | Early-stop rule |
|---|---|---|
| 0. Snapshot and canonicalization | Frozen DesignBrief, ArchitectureKey, normalized vector, bounds, identity hash | Stop as malformed/unresolved if required inputs or units are invalid. |
| 1. Materialization | Geometry, accommodation, topology, frame/unit checks | Stop if geometry cannot be materialized or required topology is invalid. |
| 2. Preliminary sizing | Mass, volume, propulsion/energy compatibility, CG seed | Stop or retain as physical infeasible if a hard closure is impossible. |
| 3. Native aero and trim | Aerodynamic values, trim, stability margins, solver status | Stop on required solver failure; record physical margins separately. |
| 4. Mission and performance | Range, reserves, climb/ceiling, takeoff/landing, approach speed | Stop only at the declared required fidelity; preserve all computed residuals. |
| 5. Structures or external analyses | Wingbox, load, stress, deflection, aeroelastic, or tool outputs | Required only for the applicable promotion level; tool failure never becomes pass. |
| 6. Uncertainty and robust screen | Scenario statistics, quantiles, confidence intervals, sensitivity | Required for robust-design policy; otherwise status must be NotRun, not pass. |
| 7. Aggregation and ranking data | All residuals, objectives, diagnostics, provenance, convergence fields | Candidate is comparable only if required stages completed with valid outputs. |

### 4.3 Assessment record

    struct CandidateAssessment {
        key: CandidateKey,
        architecture: ArchitectureKey,
        stages: Vec<StageResult>,
        constraints: Vec<ConstraintResidual>,
        objectives: Vec<ObjectiveValue>,
        robust_metrics: Option<RobustMetrics>,
        feasibility: FeasibilityScore,
        comparable: bool,
        required_stages_complete: bool,
        evaluation_status: EvaluationStatus,
        dominant_rejection: Option<DiagnosticCode>,
        convergence: Option<ConvergenceEvidence>,
    }

The existing alas-opt ObjectiveAssessment already wraps the legacy objective
evaluation with ConstraintResidual values and a FeasibilityScore. This
contract extends the idea to stage status, architecture, fidelity, scenario,
provenance, and robust metrics. The existing ObjectiveEvaluation fields remain
useful as a compatibility view, not as the complete evidence record.

### 4.4 Residual convention

Use a signed residual where positive means violation and non-positive means
pass. Normalize by an explicit positive scale:

    normalized_violation = max(residual, 0.0) / scale

Examples:

    minimum requirement: residual = target - actual
    maximum requirement: residual = actual - target
    equality tolerance: residual = abs(actual - target) - tolerance
    lower-bound margin: residual = lower_bound - actual
    upper-bound margin: residual = actual - upper_bound

Store actual, target, direction, unit, scale, policy, load case, scenario,
fidelity, and source requirement ID. Never infer a missing target from an
objective or treat an unsupported requirement as satisfied.

### 4.5 Semantics of not evaluated

The following states are different and must survive export:

    process_ok
    parsed_output_ok
    comparable_output
    requirement_pass

For example, a process can exit successfully while emitting an incomplete
solver deck. A parsed number can be present but refer to the wrong load case.
A comparable output can still violate a hard requirement. A NotRun robust
screen is not a robust pass. The report must display these distinctions.

## 5. Feasibility-first ranking and Pareto policy

### 5.1 Total ordering for a scalar search

Define a deterministic ranking key. A lower tuple is better after the Boolean
fields are encoded so that true means better:

    (
        comparable_and_required_complete: descending,
        hard_feasible: descending,
        robust_hard_feasible_when_required: descending,
        evaluation_failure_count: ascending,
        hard_violation_severity: ascending,
        normalized_hard_residual: ascending,
        hard_violation_count: ascending,
        normalized_soft_residual: ascending,
        objective_tuple: lexicographic or policy-defined,
        architecture_id: ascending,
        continuous_vector_hash: ascending,
    )

The intended interpretation is:

1. Valid geometry and completed required stages.
2. Zero hard requirement violations.
3. Robust hard constraints pass when robust policy is active.
4. Recoverable physical misses are preferred to failed or incomparable
   evaluations when neither is feasible.
5. Lower hard violation severity and normalized violation are preferred.
6. Lower soft-target penalty is preferred.
7. Only then compare cost, fuel, mass, emissions, or other objectives.
8. Stable identifiers break exact ties.

The current FeasibilityScore::cmp_feasibility implements the core feasible
first, failure-aware, hard-severity ordering. The current CandidateScore
comparison then moves to the ordinary objective and does not insert
normalized_soft_residual as a distinct key. The contract above identifies
that as a deliberate follow-up gap: soft residual should be added before the
ordinary objective when the requirements-first policy is enabled.

### 5.2 Reference comparison pseudocode

    fn better(a: &CandidateAssessment, b: &CandidateAssessment) -> Ordering {
        compare_desc(a.comparable && a.required_stages_complete,
                     b.comparable && b.required_stages_complete)
        .then(compare_desc(a.feasibility.is_feasible(),
                           b.feasibility.is_feasible()))
        .then(compare_desc(a.robust_pass_if_required(),
                           b.robust_pass_if_required()))
        .then(a.feasibility.failure_count.cmp(&b.feasibility.failure_count))
        .then(a.feasibility.hard_violation_severity.total_cmp(
              &b.feasibility.hard_violation_severity))
        .then(a.feasibility.normalized_hard_residual.total_cmp(
              &b.feasibility.normalized_hard_residual))
        .then(a.feasibility.hard_violation_count.cmp(
              &b.feasibility.hard_violation_count))
        .then(a.feasibility.normalized_soft_residual.total_cmp(
              &b.feasibility.normalized_soft_residual))
        .then(compare_objectives(a, b))
        .then(a.key.architecture_id.cmp(&b.key.architecture_id))
        .then(a.key.continuous_vector_hash.cmp(&b.key.continuous_vector_hash))
    }

The exact Rust field names can follow the existing types, but the ordering
must be tested with cases that isolate every term. In particular:

- A feasible high-cost candidate outranks an infeasible low-cost candidate.
- A candidate with one severe hard miss loses to one with a smaller total
  normalized miss, even when the objective is better.
- An analysis failure does not become a very expensive physical design.
- A lower soft-target penalty wins before ordinary objective comparison.
- Tied assessments sort identically across worker counts and process runs.

### 5.3 Multiobjective and cross-architecture decisions

For a multiobjective run, use constrained dominance:

- Any comparable feasible candidate dominates an infeasible candidate.
- Between infeasible candidates, lower aggregate normalized hard violation
  dominates higher violation.
- Among equally feasible candidates, Pareto dominance applies to declared
  objectives.
- Equal objective vectors use crowding distance and then the stable
  CandidateKey.

Maintain fronts per architecture during exploration. A global front can be
reported only after objectives are normalized to a common policy and all
members are evaluated at a comparable fidelity and scenario plan. If
architectures have different required constraints, report the per-architecture
fronts and a separate decision layer rather than hiding incomparable concepts
inside one front.

The final report should include:

- all feasible nondominated candidates;
- the selected decision and the decision policy;
- the nearest infeasible candidates with their dominant residuals;
- architecture seed survival counts;
- the fidelity and uncertainty level used for each comparison.

## 6. DOE, search, surrogate management, caching, and parallelism

### 6.1 DOE and initialization

Use a deterministic LHS in normalized coordinates. For N samples and each
dimension, generate one point in each stratum, then apply an independent
seeded permutation. Include the baseline design and any requirement-derived
seed points, but record them as seeded exceptions to the LHS population.

An LHS is a coverage design, not evidence of convergence. Report:

- number of strata and dimensions;
- seed and random stream;
- baseline/seed points added;
- bound projection count;
- unique candidate count;
- feasible count and rejection groups;
- marginal and pairwise coverage diagnostics where useful.

For an architecture with a very small feasible region, use adaptive enrichment
near the best feasible and near the feasibility boundary, while retaining
space-filling points to avoid premature collapse.

### 6.2 Search portfolio

Recommended portfolio for the current ALAS problem:

1. LHS plus deterministic baseline and architecture seeds.
2. Feasibility-first DE as the broad search.
3. CMA-ES for a continuous local refinement when covariance adaptation is
   helpful.
4. NSGA-II for a declared multiobjective study.
5. Feasibility-aware Bayesian or trust-region surrogate for expensive
   promoted fidelity.
6. Gradient/adjoint refinement only where derivatives and model validity are
   demonstrated.

Run methods independently or as a staged portfolio. Do not merge objectives,
penalties, or fidelity levels without recording the policy that made them
comparable.

### 6.3 Surrogate management

Use separate models or outputs for:

- each hard residual or a calibrated feasibility probability;
- each soft residual;
- each objective;
- evaluation cost or failure probability when useful.

Do not train one objective surrogate on all failures encoded as an arbitrary
large number. Use typed labels: valid feasible, valid infeasible, failed,
unresolved, and not evaluated. A failure classifier can be useful, but its
prediction must never become a requirement pass.

For a surrogate-controlled loop:

1. Generate an initial design set with a declared fidelity.
2. Fit and cross-validate the models.
3. Select points using objective improvement plus feasibility uncertainty or
   a constraint-boundary criterion.
4. Evaluate the selected points with the real analysis.
5. Update the model and keep an untouched validation set.
6. Reset or shrink the trust region when validation error or feasibility
   misclassification exceeds its declared threshold.
7. Promote only after the surrogate has demonstrated acceptable error on
   holdout and boundary points.

Priem et al.'s upper trust bound feasibility concept is a useful reference for
learning the feasible region rather than assuming that a failed constraint
prediction is safe [O4]. The current ALAS turbo_1 radial-basis model should
adopt the same explicit validation and trust-region evidence even if the model
remains simpler.

### 6.4 Cache contract

Cache at stage granularity where possible:

    stage_cache_key =
        requirements_snapshot_sha256
        + architecture_id
        + upstream_artifact_hashes
        + stage_id
        + fidelity_id
        + scenario_id
        + evaluator_version
        + toolchain_id
        + numerical_tolerance_id

Store:

- canonical inputs and their hashes;
- outputs and units;
- residuals and statuses;
- tool stdout/stderr and exit status when relevant;
- runtime and resource information;
- source/model/version metadata;
- cache creation time and invalidation reason.

Cache both successful and deterministic physical-infeasible stages. Cache
failures only when the failure class is known to be deterministic for the same
key. Transient network, license, resource, or tool-start failures need a
retry/invalidation policy. A cache hit must be semantically identical to a
fresh run; it must not skip required provenance.

The current alas-opt search code has history and a local mesh-correction cache,
but no general candidate/stage memoization contract. This is a high-value
implementation gap.

### 6.5 Parallel evaluation

Independent candidates should be evaluated in batches. The current
DesignObjective path already supports a worker count and stable merge order
for CPU-bound analyses. Preserve that behavior:

- Assign each candidate an immutable sequence index.
- Use independent deterministic child RNG streams or no per-candidate RNG.
- Return results in input order, not completion order.
- Make external-tool workspaces unique per CandidateKey.
- Limit nested parallelism so workers do not oversubscribe the machine.
- Record worker count, tool concurrency, retries, and cancellation.
- Verify that one worker and many workers produce the same ordered
  assessments, hashes, and rankings.

Parallel speedup is evidence about execution, not about optimizer quality.
Report wall time, CPU time, worker count, cache hits, and evaluations per
fidelity.

## 7. Multifidelity funnel and promotion rules

### 7.1 Recommended funnel

    requirements and architecture
            |
            v
    algebraic sizing, geometry, accommodation, basic closure
            |
            v
    Class-I/Class-II mass, CG, payload, mission proxies
            |
            v
    native low-fidelity aero, trim, stability, field performance
            |
            v
    low-fidelity wingbox, loads, structural margins
            |
            v
    robust/UQ screen with declared scenarios
            |
            v
    external high-fidelity aero/structures and final comparison

The requirements-first document already calls for cheap deterministic,
cacheable, parallel early stages followed by native aero/mission and optional
external tools. The evidence supports that funnel:

- LAMBDA and OpenMDAO show modular, replaceable discipline/fidelity
  interfaces [N1, N8].
- OpenAeroStruct supplies a useful coupled low-fidelity benchmark [D3, D4].
- NASA rapid structural analysis targets conceptual-stage speed and robustness
  [N9].
- Thelen et al. show that multifidelity gradient strategies can save
  high-fidelity evaluations for some aero shape problems, but may cost more
  for structural sizing [N4].
- NASA nested wingbox studies use different optimization roles for layout and
  sizing [N2, N3].

### 7.2 Promotion and cross-fidelity rules

Promotion is not automatic because a low-fidelity result has a low objective.
Promote a candidate when it is:

- comparable and complete at the current required level;
- hard-feasible or among the best boundary candidates under the declared
  policy;
- sufficiently diverse in architecture and objective space;
- useful for calibrating a low/high fidelity discrepancy;
- selected for uncertainty or sensitivity evidence.

When comparing low and high fidelity:

- Keep each fidelity's raw objectives and residuals.
- Do not compare raw objective values across fidelity as if identical.
- Estimate bias and scatter on paired points, with a validation set.
- Require high-fidelity re-evaluation of the selected candidate and its
  nearest competitors.
- Preserve low-fidelity feasibility as a screening result, not final proof.

### 7.3 Fidelity and requirement policy

Each requirement should declare the minimum fidelity needed for a pass. If no
available fidelity can evaluate it, use status unresolved or diagnostic.
Never convert unavailable evidence to a passing value. A final claim should
show the requirement, load case/scenario, actual, target, residual, fidelity,
solver status, and evidence reference.

## 8. Uncertainty, robust design, and reliability-based design

### 8.1 Uncertainty taxonomy

At minimum, distinguish:

- aleatory input uncertainty: weather, winds, payload variation, runway
  conditions, manufacturing variation;
- epistemic input uncertainty: incomplete technology or regression knowledge;
- model-form uncertainty: choice of aerodynamic, structural, propulsion, or
  weight model, including discretization and solver assumptions;
- numerical uncertainty: convergence tolerance, mesh, interpolation, and
  floating-point effects.

NASA design-under-uncertainty work explicitly treats uncertainty in
conceptual-design optimization and reports the computational cost of robust
optimization [N5]. The NASA wingbox-under-uncertainty study demonstrates that
mixed topology/continuous design and uncertainty propagation can be combined
with outer-level surrogate/infill search [N3]. The NASA model-form uncertainty
knowledge-base work is a reminder that analysis-choice uncertainty should be
captured as context and credibility evidence, not only as a numerical error
bar.

### 8.2 Scenario and probability contract

For a minimum requirement, define a signed margin r = actual - minimum. For a
maximum requirement, use r = maximum - actual. For an equality with tolerance,
use r = tolerance - abs(actual - target). A chance constraint is:

    P(r >= 0) >= p_target

or an equivalent lower quantile requirement. For every robust metric, record:

- input distributions, bounds, and correlations;
- aleatory/epistemic/model-form labels;
- scenario generation method: LHS, Monte Carlo, quadrature, or PCE;
- sample count, random seed, and common-random-number policy;
- mean, standard deviation, selected quantiles, and confidence intervals;
- worst or tail scenario and its residual;
- model/fidelity used for each scenario;
- whether the estimate is exploratory or decision-grade.

Common random numbers are useful when comparing nearby designs because they
reduce comparison noise. They must be seeded and identified in the scenario
plan. PCE or analytic gradients can reduce cost for smooth models, but their
validity should be checked against held-out samples.

### 8.3 Robust objective and hard constraints

A robust scalar objective can be defined as:

    J_robust = E[J] + lambda_risk * Risk(J)

where Risk may be standard deviation, an upper quantile, CVaR, or a declared
cost of failure. This objective is subordinate to robust hard constraints
when those constraints are policy-hard. A low expected fuel burn must not
outrank a design with a negative robust hard margin.

Use two explicit modes:

- nominal mode: robust metrics NotRun unless requested;
- robust mode: required uncertainty scenarios and chance/quantile
  constraints participate in feasibility.

Do not silently mix nominal and robust rankings. The mode, scenario plan, and
confidence policy belong in CandidateKey.

### 8.4 Validation and sensitivity

The Soton takeoff study validates a takeoff-performance uncertainty model
against flight-test data and uses Monte Carlo/sensitivity analysis [N10]. For
ALAS, it supports a validation pattern: compare the uncertainty model to
available evidence before using it to reject or select a concept.

Use a progressive sensitivity plan:

1. Morris or one-at-a-time screening for obvious inactive variables.
2. Correlation/rank analysis for initial samples.
3. Sobol or variance-based indices for a validated surrogate or PCE.
4. Derivative and adjoint sensitivities for smooth coupled models.
5. Local perturbation and active-bound analysis around finalists.

Sensitivity is not identifiability. Report both: a variable can be sensitive
but confounded with another variable, or locally insensitive while important
in another architecture or load case.

## 9. ALAS mapping

### 9.1 Existing implementation mapping

| ALAS artifact | Current behavior | Research contract / next increment |
|---|---|---|
| docs/REQUIREMENTS_FIRST_AIRCRAFT_DESIGN.md | Defines TLAR/DesignBrief policy, architecture before continuous optimization, staged candidate evaluation, residual semantics, fidelity funnel, no-feasible diagnostics, and traceability. | Treat this document as the normative workflow policy. Add explicit robust-mode and fidelity-per-requirement fields when the data model is extended. |
| crates/alas-opt/src/evaluator.rs | ObjectiveEvaluation is the legacy scalar view. ObjectiveAssessment adds ConstraintResidual values and FeasibilityScore. ObjectiveEvaluator can override assessment evaluation. | Add architecture/fidelity/scenario/status/provenance to the assessment sidecar while preserving legacy compatibility. |
| crates/alas-opt/src/feasibility.rs | ConstraintResidual uses positive violation, scale, hard/soft, physical/evaluation-failure kind. FeasibilityScore aggregates hard/soft/failure metrics. CandidateScore supports feasibility-first policy. | Insert soft residual before ordinary objective in requirements-first ranking. Add explicit comparability and robust-hard fields. Test every ordering term. |
| crates/alas-opt/src/history.rs | Stores design vectors, validity, cost, aerodynamic values, and reject reason; detailed residuals are not serialized in the basic history. | Persist stage statuses, residuals, architecture IDs, fidelity/scenario IDs, cache state, and convergence evidence in a versioned sidecar. |
| crates/alas-opt/src/differential_evolution_optimizer.rs | Uses bounded variables, seed, worker count, population/iteration controls, LHS initialization, and batch evaluation. | Add complete CandidateKey/caching, architecture loop, promotion policy, and run summary metrics. |
| crates/alas-opt/src/search_methods/constrained_de.rs | Feasibility-first DE with LHS and Lampinen-style constrained comparison reference. | Use as the default inner continuous method and add mixed-architecture orchestration outside it. |
| crates/alas-opt/src/search_methods/nsga2.rs | Minimization objectives and feasibility-aware dominance; retains Pareto candidates. | Extend ParetoCandidate with residuals, architecture, fidelity, robust metrics, and evidence refs. |
| crates/alas-opt/src/search_methods/cma_es.rs | Bounded covariance-adapting evolution strategy with feasibility-first behavior. | Use for fixed-architecture continuous refinement and compare against DE under identical seeds/budgets. |
| crates/alas-opt/src/search_methods/turbo.rs | Bounded local RBF surrogate with feasibility offsets. | Add holdout validation and describe it as an ALAS RBF trust-region method until a true GP TuRBO implementation exists. |
| crates/alas-opt/src/differential_evolution_sampling.rs | LHS population helper and population convergence check. | Record strata, seed, max spread, stagnation, and unique/cache-hit counts; do not use LHS alone as convergence evidence. |
| crates/alas-pipeline | Normal workflow owner; should freeze the requirements snapshot, architecture, vector, bounds, options, and stage plan before alas-opt. | Make the frozen snapshot hash and stage/fidelity plan part of CandidateKey and report provenance. |
| crates/alas-geom, alas-mass, alas-payload, alas-aero, alas-mission, alas-perf, alas-stab, alas-struct | Discipline owners for geometry, mass, payload, aero, mission, performance, stability, and structures. | Keep each discipline pure or explicitly state external side effects, units, failure states, and fidelity. |
| docs/research/digital-thread.md | Establishes CPACS, provenance, frames, units, UIDs, process status, and derived-artifact rules. | Apply the same hash/status discipline to optimization cache and result exports. |

### 9.2 Requirements-to-discipline mapping

| Requirement family | Primary ALAS evidence | Optimization treatment |
|---|---|---|
| Range, reserves, mission closure | alas-mission and mission requirements | Hard residual at declared mission fidelity; robust scenarios for winds, payload, temperature, and reserve policy. |
| Payload, cabin, hold, LD3/container fit | alas-payload, alas-geom, mass closure | Architecture-dependent feasibility before expensive aero. |
| Mach, MMO/VMO, atmosphere, aero envelope | alas-atmo, alas-aero, stability | Use load-case/scenario residuals; do not pass unsupported envelope checks. |
| ICA, TTC, OEI, ceiling | alas-perf and propulsion | Stage-specific hard residuals; failed external tool is not a pass. |
| Takeoff, landing, Vapp, runway | alas-perf and airport model | Promote finalists and validate uncertainty model where possible. |
| Span, geometry, wetted/planform bounds | alas-geom | Hard geometry residuals at materialization. |
| ACN/pavement | performance/airport evidence where implemented | Diagnostic or unresolved until the appropriate data/model exists. |
| CG, static margin, trim | alas-mass, alas-stab, alas-aero | Hard/soft policy from the DesignBrief and load case. |
| Wingbox, stress, deflection, aeroelasticity | alas-struct and external tools | High-fidelity promotion with explicit model/fidelity identity. |

## 10. Verification plan

The verification plan is intentionally mapped to existing alas-opt behavior and
the requirements-first document. It can be implemented as unit tests,
property tests, golden fixtures, and end-to-end evidence without modifying
unrelated source crates.

| ID | Verification target | Test/evidence | Acceptance |
|---|---|---|---|
| V1 | Canonical DesignBrief and units | Same semantic brief serialized in different field order/unit input. | One normalized snapshot hash and one ordered vector; invalid units fail before optimization. |
| V2 | Architecture separation | Enumerate a small catalogue with different deck, engine count, propulsion family, and layout choices. | Every ArchitectureKey is explicit; no continuous vector contains hidden rounded topology; seed and rejection reason are recorded. |
| V3 | Bounds and vector identity | Property-test normalize/inverse mapping, fixed variables, lower/upper limits, NaN/infinity, and out-of-bound policy. | Round-trip within declared tolerance; invalid values are typed failures; cache identity is deterministic. |
| V4 | LHS strata | Exercise latin_hypercube_population with a known seed and dimension count. | Each dimension uses every stratum once; baseline/seed exceptions are explicitly marked; repeated seed gives identical population. |
| V5 | Residual signs | Test minimum, maximum, equality, geometry, and evaluation-failure residuals. | Positive means violation; zero/negative passes; scales are positive and normalized severity is finite. |
| V6 | Feasibility-first scalar rank | Compare feasible/high-cost, infeasible/low-cost, severe/small hard miss, soft penalty, and failed evaluations. | Comparator follows the contract; objective cannot outrank hard feasibility; failure is not an arbitrary objective. |
| V7 | Soft residual policy | Construct equal-hard-feasibility candidates with different normalized soft residuals and objective. | Requirements-first policy ranks lower soft penalty first; legacy behavior is explicitly labeled if retained. |
| V8 | Stage short circuit | Inject invalid geometry, impossible volume, nonconverged aero, missing output, and unavailable external tool. | Later stages are NotRun; status and dominant rejection remain visible; no missing value is treated as a pass. |
| V9 | No-feasible diagnostic | Run an objective whose all candidates violate different hard constraints. | Return NoFeasibleDesign with counts, best near-feasible candidates, residual groups, and history evidence. |
| V10 | NSGA-II constrained dominance | Use known two-objective feasible/infeasible points. | Feasible points dominate infeasible points; infeasible ordering uses violation; returned front is nondominated and deterministic. |
| V11 | Method determinism | Run DE, feasibility-first DE, NSGA-II, CMA-ES, and turbo_1 with fixed seed and budget. | Same assessment hashes, ranking, and termination evidence across repeated runs. |
| V12 | Parallel equivalence | Evaluate the same batch with workers = 1 and workers > 1. | Stable input-order merge; same results, hashes, diagnostics, and ranking; no shared-workspace collision. |
| V13 | Cache equivalence | Fresh and cached evaluation for the same complete CandidateKey; change one key component at a time. | Cache hit is numerically and semantically identical; changing snapshot, fidelity, scenario, tolerance, or evaluator version misses. |
| V14 | Failure-cache policy | Repeat deterministic physical failure and transient tool failure. | Deterministic physical result may be reused; transient failure follows retry/invalidation policy and is not mislabeled physical infeasibility. |
| V15 | Fidelity promotion | Use paired low/high-fidelity fixtures with a known discrepancy and an unseen high-fidelity point. | Raw levels remain separate; promotion re-evaluates finalists; no low-fidelity pass is exported as high-fidelity evidence. |
| V16 | Surrogate validation | Train objective and residual models on an initial set; hold out boundary and interior points. | Misclassified feasibility and prediction error are measured; trust region shrinks or rejects acquisition when thresholds fail. |
| V17 | Robust scenario reproducibility | Generate LHS/Monte Carlo scenarios with distributions, correlations, seed, and common random numbers. | Same scenario plan reproduces quantiles; scenario metadata and sample count are serialized. |
| V18 | Chance/quantile constraints | Use a synthetic margin distribution with known quantile and failure probability. | P(r >= 0) and quantile policy agree within sampling confidence; negative robust hard margin cannot be selected. |
| V19 | Sensitivity and identifiability | Use a known linear/collinear fixture and a nonlinear aircraft proxy. | Sensitive variables, rank deficiency, correlations, active bounds, and confidence/limitations are reported. |
| V20 | Convergence evidence | Run a converging and a stagnating population with identical budgets. | Report evaluations, unique/cache hits, feasible count, best feasible key, population spread, improvement/stagnation, and termination reason. |
| V21 | External benchmark regression | Use AGARD 445.6 as a structural/aeroelastic reference where the corresponding analysis path exists [N11]. | Regression targets, units, mesh/model identity, and tolerances are documented; benchmark result is not generalized to transport design. |
| V22 | Requirements traceability | Build a candidate report from a DesignBrief containing hard, soft, objective, and diagnostic policies. | Every evaluated requirement has actual/target/residual/status/fidelity/evidence; unsupported requirements are unresolved or diagnostic. |
| V23 | End-to-end architecture funnel | Run multiple architecture seeds through low-fidelity screen, inner search, promotion, and report. | Architecture survival, rejection groups, Pareto results, final high-fidelity evidence, and no-feasible explanation are reproducible. |

Existing tests in crates/alas-opt/tests and the search-method unit tests
already cover parts of V4, V6, V9, V10, V11, and V12. The verification work
should extend those tests with the staged contract, cache keys, robust
scenario plans, and serialization sidecars rather than bypassing current
compatibility APIs.

### 10.1 Convergence and termination evidence

Every optimization run should record:

- algorithm, version, configuration, seed, and worker count;
- architecture ID and continuous-vector definition;
- population size, iteration/generation, and evaluation budget;
- total evaluations, unique evaluations, cache hits, and failures;
- feasible count per generation and first-feasible evaluation;
- best feasible candidate key and best near-feasible candidate key;
- hard violation count, normalized hard severity, soft penalty, and objective
  history;
- population spread, maximum coordinate spread, and objective spread;
- Pareto hypervolume or epsilon indicator for multiobjective runs;
- surrogate validation error and feasibility misclassification where applicable;
- UQ sample count, quantile confidence, and robust-margin history;
- termination reason: budget, stagnation, spread, target reached, cancellation,
  or failure.

The current DE code uses population standard deviation as a convergence
signal. That is useful but insufficient by itself: a population can collapse
around an infeasible point or have small standard deviation while a coordinate
still sits at a meaningful bound. Add maximum normalized coordinate spread,
feasibility progress, objective improvement over a window, and a minimum
diversity rule.

For a final engineering decision, convergence evidence should include
independent seeds or method restarts, a high-fidelity recheck of the selected
candidate and close competitors, and a statement of remaining model and
uncertainty limitations.

## 11. Known gaps and unresolved implementation decisions

1. General candidate/stage memoization is not present in alas-opt. The
   existing mesh-correction cache is local and does not define a complete
   aircraft evaluation identity.
2. Basic history does not serialize the complete residual vector, stage
   status, architecture, fidelity, scenario, or evidence sidecar.
3. CandidateScore currently compares feasibility and then ordinary objective;
   normalized soft residual is not a separate requirements-first key.
4. ParetoCandidate is a compact legacy result and does not yet carry the full
   residual, robust, provenance, or architecture record.
5. The current product-method interface selects algorithms, but a complete
   mixed categorical architecture driver and per-architecture funnel are not
   yet a single end-to-end contract.
6. turbo_1 is an RBF local surrogate, not a full GP TuRBO implementation.
7. Robust/UQ evaluation, chance constraints, model-form uncertainty, and
   confidence intervals are not yet a general alas-opt service.
8. High-fidelity external aero/structures and their licensing/tool
   availability remain configuration-dependent.
9. The exact CPACS schema/version and the authoritative mapping from every
   ALAS field to CPACS entities must be pinned before using CPACS as an
   interchange artifact.
10. Aircraft empirical methods such as FLOPS, N+3, and Class-II regressions
    need applicability tags and calibration/technology assumptions; they
    cannot be treated as universal truth for novel configurations.
11. Robust rankings need an explicit policy for whether quantile/chance
    constraints are hard at the initial screen or only after finalist
    promotion. This note recommends nominal screening first and robust hard
    ranking at the declared robust promotion stage.

## 12. Citation-only and unresolved sources

These sources are relevant to the research question but are not included in
the local PDF packet because an authoritative, clearly reusable PDF was not
available in the saved workspace or the rights status was not clear. They
remain citation leads, not downloaded implementation evidence.

| Source | Relevance | Status/reason |
|---|---|---|
| Martins and Lambe, Multidisciplinary Design Optimization: A Survey, 2013, DOI 10.2514/1.J051895 | MDO architectures, gradient, decomposition, and multidisciplinary optimization taxonomy. | Citation-only; publisher rights/local PDF not established. |
| Gray et al., OpenMDAO: An Open-Source Framework for Multidisciplinary Design, Analysis, and Optimization, Structural and Multidisciplinary Optimization, 2019, DOI 10.1007/s00158-019-02211-z | Modern OpenMDAO architecture and implementation context. | Citation-only; NASA metadata is available but publisher PDF rights were not established. The local packet contains the earlier NASA OpenMDAO framework report [N1]. |
| Di Bianchi et al., A framework for design under uncertainty in aircraft conceptual design, 2021, DOI 10.1017/aer.2020.134 | Aircraft design uncertainty and decision-making. | Citation-only; no clearly reusable local PDF in the saved packet. |
| Fioriti et al., AGILE review, Progress in Aerospace Sciences, 2020, DOI 10.1016/j.paerosci.2020.100648 | Collaborative aircraft design and MDO process evidence. | Citation-only; publisher rights/local PDF not established. |
| Multiobjective off-design aircraft/propulsion study, Aerospace Science and Technology, 2022, DOI 10.1016/j.ast.2022.107662 | Multiobjective off-design trade methods. | Citation-only; publisher rights/local PDF not established. |
| Raymer, Roskam, and Torenbeek conceptual-design books | Weight, sizing, geometry, and preliminary design methods. | Copyrighted references; cite and acquire through lawful institutional access, do not mirror. |
| AGARD multidisciplinary design and optimization workshop material, including the 1991 Sobieszczanski-Sobieski paper | Foundational MDO process and formulation history. | NTRS record indicates restricted availability; no local copy. |
| AGARD-R-740 and related special-course material | Aircraft conceptual design and engineering methods. | Rights/republication restrictions or third-party content are unclear; citation-only. |
| DLR restricted aeroservoelastic and hybrid-electric configuration studies | Nested aero-structural and energy-system optimization examples. | Relevant records exist, but saved rights/download evidence was not sufficient for packet inclusion. |
| MSES and other proprietary/manual-based aero methods | Higher-fidelity aerodynamic promotion. | Do not redistribute manuals or decks; integrate only when the user supplies a lawful installation and license. |
| Borgia et al., 2026 robust conceptual-design work; Liu et al., ICAS 2024 | Recent robust optimization and aircraft design context. | Citation leads from prior research notes; full source/rights verification intentionally deferred. |

## 13. Local PDF source ledger

Rights notes are conservative access notes, not legal advice. NASA NTRS
distribution labels are reported from the saved research ledgers; they do not
override rights in third-party figures or embedded material. DLR eLib,
institutional preprints, and arXiv copies are retained for research and
attribution. No source below should be redistributed from ALAS unless its
license or permission expressly allows it.

All hashes below were recomputed on 2026-08-26 for the files in
bib/optimization-mdo.

| ID | Local PDF | Metadata | Source URL | Access/rights note | Pages | Bytes | SHA-256 |
|---|---|---|---|---|---:|---:|---|
| N1 | [nasa-openmdao-2014-framework.pdf](../../bib/optimization-mdo/nasa-openmdao-2014-framework.pdf) | C. M. Heath and J. S. Gray, OpenMDAO: Framework for Flexible Multidisciplinary Design, Analysis and Optimization Methods, NASA/GRC-E-DAA-TN14348, 2014 report/record. | [NASA NTRS 20140016748](https://ntrs.nasa.gov/citations/20140016748) and [official PDF](https://ntrs.nasa.gov/api/citations/20140016748/downloads/20140016748.pdf) | NTRS saved ledger: GOV_PUBLIC_USE_PERMITTED, public distribution; no third-party material indicated. | 13 | 379940 | 1c0c01839fb1cd4a761d55bab99b6ceca963f68c0dac1bcfe1c08e2d76d756fa |
| D5 | [dlr-cpacs-2020-common-language.pdf](../../bib/optimization-mdo/dlr-cpacs-2020-common-language.pdf) | C. Alder, M. Moerland, F. Jepsen, and B. Nagel, Recent Advances in Establishing a Common Language for Aircraft Design with CPACS, Aerospace Europe Conference, 2020. | [DLR eLib PDF](https://elib.dlr.de/134341/1/AEC2020_174.pdf) | Official/institutional DLR copy; retain attribution and verify permission before redistribution. | 14 | 2161382 | 330eb559eb2c97574cfa87a78536f996f2fdfc913a793d860c089ba2e0ddb8cb |
| D6 | [dlr-tigl-2019-parametric-geometry.pdf](../../bib/optimization-mdo/dlr-tigl-2019-parametric-geometry.pdf) | M. Siggel, A. Kleinert et al., TiGL - An Open Source Computational Geometry Library for Parametric Aircraft Design, Mathematics in Computer Science, 2019. | [DLR eLib copy](https://elib.dlr.de/124524/1/1810.10795.pdf) and [arXiv record](https://arxiv.org/abs/1810.10795) | Author-posted/institutional research copy; journal-version rights are separate. Do not infer a blanket redistribution license. | 23 | 4908445 | d627a242b522fc809d8c7b99c30eafd097294d422a4618b00a88dba3e14a67cb |
| D7 | [dlr-agile-2019-aerostructural-integration.pdf](../../bib/optimization-mdo/dlr-agile-2019-aerostructural-integration.pdf) | J.-N. Walther et al., Integration Aspects of the Collaborative Aero-Structural Design of an Unmanned Aerial Vehicle, CEAS Aeronautical Journal, 2019. | [DLR eLib PDF](https://elib.dlr.de/129061/1/Walther2019_Article_IntegrationAspectsOfTheCollabo.pdf) | Official/institutional DLR copy; attribution retained; separate permission required for redistribution of the article/figures. | 11 | 1785054 | 2d033627638cd56467baa2119e035347adea3fe1e821a3da57ac63e9b119816b |
| D2 | [dlr-agile-2016-collaborative-configuration.pdf](../../bib/optimization-mdo/dlr-agile-2016-collaborative-configuration.pdf) | P. S. Prakasha, P. D. Ciampa, and B. Nagel, Collaborative Systems Driven Aircraft Configuration Design Optimization, ICAS, 2016. | [DLR eLib record](https://elib.dlr.de/110984/) | DLR eLib refereed open-access conference copy; proceedings permission/attribution applies; no blanket redistribution conclusion. | 14 | 1405905 | 31690f18dac69e86ce3c5696176ddd44e3ad45907416bd073b7a89ad18cdada0 |
| D8 | [nasa-openvsp-2010-parametric-geometry.pdf](../../bib/optimization-mdo/nasa-openvsp-2010-parametric-geometry.pdf) | A. S. Hahn, Vehicle Sketch Pad: A Parametric Geometry Modeler for Conceptual Aircraft Design, AIAA 2010-657, NASA NTRS 20100003046, 2010. | [NASA NTRS 20100003046](https://ntrs.nasa.gov/citations/20100003046) and [official PDF](https://ntrs.nasa.gov/api/citations/20100003046/downloads/20100003046.pdf) | NTRS saved ledger: PUBLIC/GOV_PUBLIC_USE_PERMITTED; NASA attribution and third-party content caveat apply. | 11 | 5707615 | 1ead1a613da9087cf249e86f26cc227b4f6bbceeb4b92ca6c485b7521f5d864b |
| D9 | [nasa-openvsp-2016-degenerate-geometry.pdf](../../bib/optimization-mdo/nasa-openvsp-2016-degenerate-geometry.pdf) | E. D. Olson, Multi-Disciplinary, Multi-Fidelity Discrete Data Transfer Using Degenerate Geometry Forms, NASA/NF1676L-22875, 2016. | [NASA NTRS 20160010160](https://ntrs.nasa.gov/citations/20160010160) and [official PDF](https://ntrs.nasa.gov/api/citations/20160010160/downloads/20160010160.pdf) | NTRS saved ledger: PUBLIC/GOV_PUBLIC_USE_PERMITTED; retain NASA attribution and inspect embedded material rights. | 14 | 1297307 | 7f97375051b1cb919a570e6f203786440a0517ef8268aa0f15489895ed336143 |
| D1 | [nasa-viper-2019-integrated-mdao.pdf](../../bib/optimization-mdo/nasa-viper-2019-integrated-mdao.pdf) | J. A. Garcia, J. V. Bowles, D. J. Kinney, J. E. Melton, and X. J. Jiang, VIPER Integrated MDAO Analysis for Conceptual Design of Supersonic X-Plane Vehicles, NASA/TM-2019-220239, 2019. | [NASA NTRS 20190027157](https://ntrs.nasa.gov/citations/20190027157) | NTRS saved ledger: PUBLIC/PUBLIC_USE_PERMITTED; retain NASA attribution and third-party material caveat. | 12 | 712195 | cdf78608f45abe2e21d77a340025a23a81d32a69f85c5be9c3e7b9fd3f3cc877 |
| D3 | [openaerostruct-2018-coupled-aerostructural.pdf](../../bib/optimization-mdo/openaerostruct-2018-coupled-aerostructural.pdf) | J. P. Jasa, J. T. Hwang, and J. R. R. A. Martins, Open-source coupled aerostructural analysis and optimization with OpenAeroStruct, Structural and Multidisciplinary Optimization, 2018, DOI 10.1007/s00158-018-1912-8. | [University of Michigan author copy](https://websites.umich.edu/~mdolaboratory/pdf/Jasa2018a.pdf) | Author/university-hosted preprint; final publication rights are separate; research use and attribution only. | 16 | 813943 | 4b7aa8ddfbbc94536a02614b23b2e434fedd19f69f4992920e511c133f472315 |
| D4 | [openaerostruct-2018-wingbox.pdf](../../bib/optimization-mdo/openaerostruct-2018-wingbox.pdf) | S. S. Chauhan and J. R. R. A. Martins, Low-fidelity aerostructural optimization of aircraft wings with a simplified wingbox model using OpenAeroStruct, 2018. | [University of Michigan author copy](https://websites.umich.edu/~mdolaboratory/pdf/Chauhan2018b.pdf) | Author/university-hosted preprint; published-version rights are separate; research use and attribution only. | 12 | 407701 | 590b6feb552c13e56530436ed974a93508b97aac808f67624e14a1c2a9e9b416 |
| N8 | [lambda-2024-conceptual-design-mdo.pdf](../../bib/optimization-mdo/lambda-2024-conceptual-design-mdo.pdf) | M. Hosseini, S. Vaziry-Zanjany, and H. Ovesy, LAMBDA: A Modular Framework for Aircraft Conceptual Design, Aerospace 11(4), 273, 2024, DOI 10.3390/aerospace11040273. | [MDPI article](https://www.mdpi.com/2226-4310/11/4/273) and [PDF](https://mdpi-res.com/d_attachment/aerospace/aerospace-11-00273/article_deploy/aerospace-11-00273-v2.pdf) | MDPI article marked CC BY 4.0; retain license, attribution, and source notice. | 33 | 11650905 | 02a0bcd8b2e38040c0766f1a802828bff25c10fbbf77a4dae131bd3d6e0659cf |
| N4 | [thelen-2022-multifidelity-aeroelastic-optimization.pdf](../../bib/optimization-mdo/thelen-2022-multifidelity-aeroelastic-optimization.pdf) | A. Thelen, A. Bryson, B. K. Stanford, and K. Beran, Multi-Fidelity Gradient-Based Optimization for High-Dimensional Aeroelastic Configurations, Algorithms 15, 131, 2022, DOI 10.3390/a15040131. | [MDPI article](https://www.mdpi.com/1999-4893/15/4/131) | MDPI article marked CC BY 4.0; retain license, attribution, and source notice. | 35 | 6479698 | d4521fc19f8fc28270052357ea4ed14891f7cf4cc5587be76d25911d24c53973 |
| N2 | [nasa-2019-nested-wingbox-optimization.pdf](../../bib/optimization-mdo/nasa-2019-nested-wingbox-optimization.pdf) | B. K. Stanford, C. V. Jutte, and C. A. Coker, Sizing and Layout Design of an Aeroelastic Wingbox through Nested Optimization, 2019. | [NASA NTRS 20190000445](https://ntrs.nasa.gov/citations/20190000445) | NTRS saved ledger: PUBLIC/PUBLIC_USE_PERMITTED; retain NASA attribution and inspect third-party material. | 14 | 8347477 | c22273fa119334ee7eafb0549f23dc90d18d58f1f92be11634dac8a6b4e326b3 |
| N3 | [nasa-2020-wingbox-under-uncertainty.pdf](../../bib/optimization-mdo/nasa-2020-wingbox-under-uncertainty.pdf) | B. K. Stanford and S. Roy, Sizing and Topology Design of an Aeroelastic Wingbox Under Uncertainty, NASA NTRS 20200002634, 2020 record. | [NASA NTRS 20200002634](https://ntrs.nasa.gov/citations/20200002634) | NTRS saved ledger: PUBLIC/GOV_PUBLIC_USE_PERMITTED; no third-party content indicated in saved notes. | 14 | 7962640 | dfa486345be6aa5597a871a705a28e04ff77f1a4d433db3b6a9c555ccb0350d0 |
| N5 | [nasa-2024-design-under-uncertainty.pdf](../../bib/optimization-mdo/nasa-2024-design-under-uncertainty.pdf) | B. D. Phillips, J. N. Schmidt, E. D. Aretskin-Hariton, and R. D. Falck, Design Under Uncertainty for Conceptual Aircraft Design Leveraging Analytical Gradients, NASA NTRS 20240014863, 2024 record. | [NASA NTRS 20240014863](https://ntrs.nasa.gov/citations/20240014863) and [official PDF](https://ntrs.nasa.gov/api/citations/20240014863/downloads/Phillips_SciTech_rev2.pdf?attachment=true) | NTRS saved ledger: GOV_PUBLIC_USE_PERMITTED, public distribution; no third-party material indicated in saved notes. | 17 | 6944507 | 43cc62f314a886810af2a023aa313b3bdf1aa1889817c70ae315808bfb48dbb5 |
| N9 | [nasa-2015-rapid-robust-structural-analysis.pdf](../../bib/optimization-mdo/nasa-2015-rapid-robust-structural-analysis.pdf) | L. B. Eldred, S. L. Padula, and W. Li, Enabling Rapid and Robust Structural Analysis During Conceptual Design, NASA/TM-2015-218687, 2015. | [NASA NTRS 20150002820](https://ntrs.nasa.gov/citations/20150002820) | NTRS saved ledger: PUBLIC/GOV_PUBLIC_USE_PERMITTED; no third-party content indicated in saved notes. | 26 | 1152507 | 9f85856bc7f4fca594d3bedb3dd6703ed610aa2485b58afdd1240731f5dcbdc6 |
| N6 | [nasa-2010-n-plus-3-trade-studies.pdf](../../bib/optimization-mdo/nasa-2010-n-plus-3-trade-studies.pdf) | E. M. Greitzer et al., N+3 Aircraft Concept Designs and Trade Studies, NASA/CR-2010-216794/VOL1, 2010. | [NASA NTRS 20100042401](https://ntrs.nasa.gov/citations/20100042401) and [official PDF](https://ntrs.nasa.gov/api/citations/20100042401/downloads/20100042401.pdf) | NASA public-use/distribution status in saved ledger; empirical correlations and third-party material remain subject to their own terms. | 189 | 14248201 | 3f2db87824637ecea952b33c7807bf48e99af71b91ed3202a32cbf7d62a6a599 |
| N7 | [nasa-2017-flops-weights.pdf](../../bib/optimization-mdo/nasa-2017-flops-weights.pdf) | D. P. Wells, B. L. Horvath, and L. A. McCullers, The Flight Optimization System Weights Estimation Method, NASA/TM-2017-219627/VOL1, 2017. | [NASA NTRS 20170005851](https://ntrs.nasa.gov/citations/20170005851) and [official PDF](https://ntrs.nasa.gov/api/citations/20170005851/downloads/20170005851.pdf) | NTRS saved ledger: PUBLIC_USE_PERMITTED, public distribution; no third-party material indicated in saved notes. | 91 | 881689 | 819a48fc9c8f34f14595d93f3e3d54dc8454298e83e64048c14ac7bda00bb51d |
| N11 | [agard-445-6-nasa-1987-benchmark.pdf](../../bib/optimization-mdo/agard-445-6-nasa-1987-benchmark.pdf) | E. Carson Yates Jr., AGARD Standard Aeroelastic Configurations for Dynamic Response: Candidate Configuration I - Wing 445.6, NASA/TM-100492, 1987. | [NASA NTRS 19880001820](https://ntrs.nasa.gov/citations/19880001820) | NTRS saved ledger: PUBLIC/GOV_PUBLIC_USE_PERMITTED; public NASA copy used instead of unclear-rights mirrors. | 78 | 3496860 | 7145d3283d6859f7a685a5546515d9f2bdc93cbdefb4a8bcd65002ad54c2dd75 |
| O4 | [priem-2020-mixed-constrained-bayesian-optimization.pdf](../../bib/optimization-mdo/priem-2020-mixed-constrained-bayesian-optimization.pdf) | R. Priem, N. Bartoli, Y. Diouane, and A. Sgueglia, Upper Trust Bound Feasibility Criterion for Mixed Constrained Bayesian Optimization with Application to Aircraft Design, arXiv:2005.05067v2, 2020. | [arXiv record](https://arxiv.org/abs/2005.05067) | arXiv author preprint; use with attribution and retain arXiv terms; no separate publisher redistribution right inferred. | 59 | 7434897 | c39dc9f495736422aaaeddd3873855cee7f3f05988bd6c377e618288c14da819 |
| N10 | [soton-2021-takeoff-uncertainty-validation.pdf](../../bib/optimization-mdo/soton-2021-takeoff-uncertainty-validation.pdf) | A. Sobester, Flight-Test Validation of a Takeoff Performance Uncertainty Model, Journal of Aircraft, 2021, DOI 10.2514/1.C036180. | [University of Southampton repository copy](https://eprints.soton.ac.uk/452175/2/1.c036180_5.pdf) and [DOI](https://doi.org/10.2514/1.C036180) | Institutional author-manuscript/publisher-access copy; AIAA copyright applies. Research/citation use only; do not redistribute. | 13 | 4612733 | 924a81e768cfbb95a6602b64a65edf225819e12ca619f8d6f1afe73ee71594e9 |

## 14. References by evidence ID

The evidence IDs in this note point to the local ledger above:

- N1: OpenMDAO framework and workflow patterns.
- N2/N3: nested mixed topology/continuous wingbox search and uncertainty.
- N4: multifidelity gradient-based aeroelastic optimization.
- N5: conceptual aircraft design under uncertainty with analytical gradients.
- N6/N7: NASA N+3 trade studies and FLOPS weights.
- N8: LAMBDA modular conceptual-design MDO framework.
- N9: rapid and robust conceptual structural analysis.
- N10: takeoff uncertainty model validation against flight-test evidence.
- N11: AGARD 445.6 benchmark configuration.
- D1: NASA VIPER integrated conceptual MDAO.
- D2/D7: DLR AGILE collaborative configuration and aerostructural
  integration.
- D3/D4: OpenAeroStruct coupled aero-structural analysis and wingbox model.
- D5/D6: CPACS and TiGL common data/geometry ecosystem.
- D8/D9: OpenVSP parametric geometry and degenerate geometry transfer.
- O4: mixed-constrained Bayesian optimization with an
  uncertainty-aware feasibility criterion.
- A1: current alas-opt feasibility-first DE implementation in
  crates/alas-opt/src/search_methods/constrained_de.rs.
- A2: current alas-opt constrained NSGA-II implementation in
  crates/alas-opt/src/search_methods/nsga2.rs.

The citation-only list in Section 12 is intentionally separate from this
ledger. It identifies useful follow-up literature without implying that a
rights-cleared local PDF is present.
