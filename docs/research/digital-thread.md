# ALAS research baseline: a reproducible aircraft-design digital thread

Status: research/design note, prepared 2026-08-26.
Scope: CPACS and its ecosystem, geometry and solver interchange, requirements
provenance, typed evidence, reproducible campaigns, FAIR/open-science practice,
and report traceability for ALAS.

This note is intentionally implementation-neutral. It records a proposed
contract and an evidence-backed map of the current repository; it does not
change existing Rust sources, fixtures, or repository ledgers. The local
copies in bib/digital-thread are research copies. Their original copyright,
licence, and redistribution conditions remain in force.

## Executive decision

ALAS should use a four-layer digital thread:

1. The requirements authority is the validated DesignBrief plus its source and
   policy metadata. It says what the design must achieve.
2. The aircraft authority is a pinned CPACS document, with a pinned CPACS
   schema, resolved external data, UID references, coordinate convention, and
   unit convention. It says what the aircraft is.
3. OpenVSP, AVL, MSES, VSPAERO, Nastran, and in-process analyses are derived
   evaluator activities. Their decks, meshes, inputs, outputs, transcripts,
   parser decisions, and comparison contracts are retained as artifacts.
4. A run manifest and typed evidence graph connect requirements to evaluators,
   inputs, outputs, tools, environments, and report figures. It says what was
   actually demonstrated, at which fidelity, and with which limitations.

This keeps CPACS as the aircraft/configuration interchange authority without
pretending that a CPACS file is a requirements database, a solver result, or a
workflow engine. It also prevents a native solver process returning zero from
being mistaken for a physical pass.

The main proposed identifiers are:

- design snapshot: the content identity of the frozen DesignBrief, CPACS
  authority, design vector, bounds, options, and unit/schema policy;
- run: one execution of a snapshot with a tool/environment/seed/campaign
  identity;
- candidate: one design-vector evaluation in a run or campaign;
- stage: one evaluator activity and its declared inputs and outputs;
- artifact: one retained file or canonical value with a content hash;
- evidence: one typed claim about a requirement, solver result, comparison,
  validation, or visualization.

## 1. Findings from the state of the art

| Area | Evidence | Finding for ALAS |
|---|---|---|
| Common aircraft language | DLR CPACS publications and documentation | A central schema reduces pairwise interfaces, gives tools a shared hierarchy and UID graph, and carries header/process metadata. CPACS is a strong aircraft authority, not a complete campaign ledger. |
| Parametric geometry | TiGL and CPACS | Geometry should be regenerated from the frozen CPACS/design snapshot where possible. CAD, mesh, and analysis exports are derived artifacts with their own hashes and transformation records. |
| Collaborative MDO | AGILE/RCE and aero-structural integration work | A multidisciplinary run is a graph of services and transfers. The workflow must preserve data dependencies, not only the final aircraft. |
| Conceptual geometry and data transfer | OpenVSP papers and NASA import documentation | A VSP3 model, degenerate geometry, mesh, or point cloud is a useful representation at a boundary. It is not evidence that another solver received equivalent geometry until references, frames, topology, and mapping are checked. |
| Low-order aerodynamics | AVL documentation | Deck text, session commands, reference quantities, Mach, symmetry, and total-force output are part of the reproducible input. An AVL polar is admissible for overlay only after a quantity-level comparison contract. |
| High-fidelity section analysis | MSES documentation and ALAS MSES status types | MSES availability and licensing must be recorded. A sweep can be partially converged; each requested angle needs its own convergence evidence. |
| Structures | Nastran deck/output practice and ALAS Nastran adapters | A BDF/DAT deck and OP2/F06 output are solver-bound artifacts. They support structural evidence only when the unit system, coordinate frames, load case, constraints, solver version, parser, and result tables are identified. |
| Digital engineering | NASA-HDBK-1004 and NASA MBSE material | The useful principle is an authoritative source of system data/models plus explicit interoperability and requirements traceability. ALAS should implement the smallest auditable subset of that idea. |
| FAIR and provenance | FAIR, W3C PROV-O, RO-Crate, and reproducible-computing guidance | Make artifacts findable, accessible, interoperable, and reusable with stable identifiers, hashes, metadata, provenance links, machine-readable status, and a human-readable report. A small JSON graph can use PROV concepts without requiring an RDF stack. |

### 1.1 CPACS and the DLR ecosystem

The CPACS website describes CPACS as an XML-based data definition for air
transportation systems and positions it as a common model for exchanging
aircraft, rotorcraft, engines, fleets, missions, and related product/process
information. The DLR material also describes the use of CPACS with RCE for
collaborative multidisciplinary design and optimization. Alder et al. explain
the interface-count advantage of a central model and the role of XSD
validation, UID references, TiXI, TiGL, and tool-specific extensions.

The CPACS documentation makes several details operationally important:

- the CPACS coordinate system is right-handed; x points from nose towards the
  tail, y is positive towards the right wingtip, and z is upward;
- analysis data are expressed in CPACS coordinates, not in a flow-rotated
  frame;
- CPACS does not attach a unit attribute to every scalar. The documented
  convention is SI, including m, m2, m3, kg, s, K, degree, N, Nm, and W;
- UID references are part of the model graph and must be globally consistent;
- externaldata may split a document into files, but the references must be
  resolved before schema validation and cycles must be detected;
- schema version and header/process information are part of the document
  context, while tool-specific data can carry non-core extensions.

The current DLR site identifies CPACS 3.5 as the stable release and presents a
3.5.1 release candidate for review as of this note's access date. The CPACS
download page displays a schema URL containing v3_5_0. ALAS currently exposes a
CPACS 3.5 schema constant containing v3_5. These strings must not be treated as
interchangeably canonical: the run manifest should record the exact schema URL,
resolved schema bytes hash, document version, validator identity, and validation
result. A future source change should pin the chosen release explicitly.

CPACS 3 is not a transparent, byte-for-byte continuation of CPACS 2. The DLR
TiGL and CPACS papers describe incompatibilities and the cpacs2to3 conversion
path. Therefore a schema migration is a recorded transformation:

    source document hash -> migration tool/version -> target document hash

It is not merely a changed version string. A migration record should include
the source/target schema identifiers, warnings, unmapped fields, and whether
the result was revalidated.

### 1.2 Interoperability is a contract, not a file extension

| Boundary | Authoritative input | Derived artifacts to retain | Minimum contract before comparison |
|---|---|---|---|
| CPACS to OpenVSP/TiGL | Frozen CPACS geometry and UID graph | VSP3 model, exported mesh or degenerate geometry, mapping, stdout/stderr | Geometry version, component UID mapping, axes, transformations, units, mesh quality checks, OpenVSP/TiGL identity |
| CPACS to AVL | Wing/body/control geometry plus operating condition | AVL geometry deck, run/session commands, total-force files, transcript, parsed polar | Sref/Cref/Bref, moment reference, axes, Mach, symmetry, alpha schedule, control state, deck hash, executable identity |
| CPACS to MSES | Resolved airfoil section and section operating point | Airfoil/setup/command files, per-point transcript, polar/pressure output | Point ordering, closed/open contour policy, orientation, Mach, Reynolds, transition assumptions, alpha request, executable/license identity |
| CPACS to VSPAERO | OpenVSP lifting-surface representation and operating condition | VSPAERO mesh, setup, polar, stdout/stderr | Mesh freshness, reference area/length, moment origin, axes, Mach, alpha schedule, parser version, comparison status |
| CPACS to Nastran | Structural idealization, material/load model, load case | BDF/DAT, Case Control, F06/OP2, solver log, parsed tables | Unit system, grid and element frames, boundary conditions, load-case ID, solver/version, output-table presence, parser status |
| In-process ALAS stages | Frozen config and candidate vector | Stage JSON, residuals, warnings, numerical transcript, figures | Crate/source revision, numerical settings, seed, input hashes, explicit convergence/status, finite-value checks |

An adapter may report that it wrote a valid deck while the corresponding
solver stage is absent. A solver may report a process success while its output
is missing or physically incomparable. These are distinct evidence records.

### 1.3 Solver and evaluator status semantics

The repository already contains a useful vocabulary:

- AVL and VSPAERO distinguish not configured, rejected setup/deck,
  launch failure, timeout, solver failure, missing output, parse failure,
  completed-but-not-comparable, and completed-and-comparable;
- MSES distinguishes not run, disabled, absent, incomplete, launch failure,
  timeout, parse failure, ok, partial convergence, and error, and also records
  convergence per requested polar point;
- the HYBRD numerical kernel treats only `Converged` as kernel success and
  preserves max-evaluation, tolerance, and no-progress outcomes;
- the mission segment integration preserves that raw HYBRD status and its
  evaluation count separately from its own typed convergence basis. It may
  accept only `NoProgressSinceJacobians` or `NoProgressSinceIterations` as
  `ResidualQualifiedNoProgress`, and only after a mandatory final evaluation
  has finite scaled physical residuals no larger than `1e-8 m/s^2`, bounded
  throttle and finite attitude controls, and every aerodynamic-surrogate
  query in-domain;
- a nominal HYBRD `Converged` status in the product mission is likewise not
  sufficient by itself: the mandatory final physical residual must be finite
  and within the independently declared `1e-7 m/s^2` physical guard, controls must be finite/available, and
  aerodynamic-surrogate queries must remain in-domain. The frozen reference-
  compatibility path alone retains the translated edge-clamping convention;
- feasibility residuals distinguish physical constraints from evaluation
  failures;
- the acceptance layer distinguishes accepted, rejected, not evaluated, and
  inconclusive;
- Nastran result status currently distinguishes not run, ok, and error, which
  is useful but is not by itself a claim that a structural requirement passed.

The manifest should preserve these existing strings and add a cross-stage
taxonomy only where necessary. Do not collapse them into a single boolean.
In particular:

    process_ok != parsed_output_ok != comparable_output != requirement_pass

The last transition is made by a requirement evaluator using declared target,
load case, policy, units, and residual convention.

The residual-qualified mission outcome does not rewrite MINPACK's exit code or
globally redefine no-progress as success. `MaxEvaluations` and
`ToleranceTooSmall` remain failures even if their last iterate happens to have
a small residual, as do no-progress exits with an unavailable control,
non-finite state, out-of-domain surrogate point, or residual above the stated
physical tolerance. The segment result retains the raw kernel status,
evaluation count, final maximum absolute residual, and the integration-level
convergence basis so downstream evidence can distinguish the two claims.
Residual-qualified no-progress acceptance is a product-mission policy only.
Frozen reference-compatibility replay retains the raw MINPACK outcome and can
never be silently upgraded by the product safeguard.

A warm-started retry from the preceding dispatch iteration is a useful next
numerical-fidelity step because dispatch mass changes can alter MINPACK's path.
It requires a broader explicit state-transfer API and should be implemented as
a recorded retry, not hidden inside the residual-qualified acceptance rule.

## 2. Proposed ALAS manifest and evidence schema

The proposal is a versioned JSON sidecar, independent of CPACS schema
evolution:

    manifest_kind: alas.run
    manifest_version: 1.0
    evidence_kind: alas.evidence
    evidence_version: 1.0

Relative paths are relative to the run root. All timestamps are UTC RFC 3339.
Secrets and unrestricted environment dumps are forbidden; environment capture
uses an allowlist.

### 2.1 Run manifest

The following is a concrete shape, with illustrative values:

~~~json
{
  "manifest_kind": "alas.run",
  "manifest_version": "1.0",
  "run_id": "run-2026-08-26T102030Z-7c2a",
  "campaign_id": "campaign-wing-001",
  "created_utc": "2026-08-26T10:20:30Z",
  "status": "completed_with_diagnostics",
  "run_root": ".",
  "authority": {
    "design_brief": {
      "artifact_id": "artifact-design-brief",
      "path": "authority/design_brief.json",
      "sha256": "sha256-of-canonical-design-brief"
    },
    "aircraft": {
      "artifact_id": "artifact-aircraft-cpacs",
      "format": "cpacs",
      "path": "authority/aircraft.cpacs.xml",
      "cpacs_version": "3.5.0",
      "schema_url": "https://www.cpacs.de/schema/v3_5_0/cpacs_schema.xsd",
      "schema_sha256": "sha256-of-resolved-schema",
      "aircraft_model_uid": "aircraft-model-uid",
      "coordinate_system": "cpacs-rh-x-tailwards-y-right-z-up",
      "unit_policy": "cpacs-si-convention",
      "resolved_external_data": []
    }
  },
  "design_snapshot": {
    "id": "design-sha256",
    "canonicalization": "alas-c14n-json-1",
    "inputs": [
      "artifact-design-brief",
      "artifact-aircraft-cpacs",
      "artifact-design-vector",
      "artifact-bounds",
      "artifact-run-options"
    ]
  },
  "requirements": ["REQ.MISSION.RANGE", "REQ.PERF.TOFL"],
  "design_space": {
    "variables": [
      {
        "id": "wing.area",
        "path": "geometry.wings[0].reference_area_m2",
        "unit": "m2",
        "lower": 85.0,
        "upper": 125.0,
        "nominal": 105.0,
        "source": "design-brief-and-optimizer-bounds"
      }
    ],
    "constraints": []
  },
  "tools": [
    {
      "tool_id": "alas",
      "name": "ALAS",
      "version": "git:commit",
      "executable": "target/release/alas.exe",
      "sha256": "sha256-of-executable-or-package",
      "source_commit": "git-commit",
      "license": "repository-license"
    }
  ],
  "environment": {
    "os": "Windows",
    "architecture": "x86_64",
    "rustc": "rustc-version",
    "cargo_lock_sha256": "sha256-of-Cargo.lock",
    "container_or_image": null,
    "environment_allowlist": {
      "RUSTFLAGS": "",
      "OMP_NUM_THREADS": "1"
    }
  },
  "randomness": {
    "seed": 12345,
    "algorithm": "declared-by-optimizer",
    "parallel_workers": 1,
    "evaluation_order": "stable-index"
  },
  "stages": [
    {
      "stage_id": "stage-avl",
      "kind": "external_aerodynamics",
      "depends_on": ["stage-cpacs-validation"],
      "status": "completed_comparable",
      "tool_id": "avl",
      "input_artifacts": ["artifact-aircraft-cpacs", "artifact-avl-deck"],
      "output_artifacts": ["artifact-avl-polar", "artifact-avl-transcript"],
      "evidence_ids": ["E.AVL.COMPARE.001"],
      "error_code": null
    }
  ],
  "artifacts": [
    {
      "artifact_id": "artifact-avl-polar",
      "path": "stages/avl/total_forces.alpha",
      "media_type": "text/plain",
      "role": "solver-output",
      "size_bytes": 12345,
      "sha256": "sha256-of-file",
      "producer_stage": "stage-avl",
      "derived_from": ["artifact-avl-deck"],
      "unit_policy": "declared-in-stage-contract",
      "retention": "required"
    }
  ],
  "evidence": ["E.REQ.MISSION.RANGE.001", "E.AVL.COMPARE.001"],
  "reports": [
    {
      "report_id": "report-main",
      "path": "reports/report.json",
      "sha256": "sha256-of-report",
      "figure_ids": ["drag_breakdown", "mission_profile"],
      "claim_evidence_ids": ["E.REQ.MISSION.RANGE.001"]
    }
  ],
  "git": {
    "repository": "ALAS-rust",
    "commit": "git-commit",
    "dirty": true,
    "working_tree_diff_sha256": "sha256-of-captured-diff-or-null"
  }
}
~~~

The real schema should define required fields and enumerations, not just
document an example. At minimum, an artifact cannot be accepted without a
relative path, media type, size, SHA-256, producer or source, and a declared
retention policy. A stage cannot be accepted without a status, input/output
lists, tool identity when applicable, and evidence for any claim it makes.

The dirty-tree field is deliberate. A run made from uncommitted work is not
invalid, but it must be distinguishable from a clean, committed run. Capturing
the full diff as an artifact is preferable to storing only a boolean.

### 2.2 Evidence record

Evidence is a typed claim, not a log line. A requirement record may cite
several evidence records, but each hard requirement should have one declared
primary evaluator and a clearly identified fallback or unsupported state.

~~~json
{
  "evidence_kind": "alas.evidence",
  "evidence_version": "1.0",
  "evidence_id": "E.REQ.MISSION.RANGE.001",
  "kind": "requirement_evaluation",
  "subject": "REQ.MISSION.RANGE",
  "claim": "Achieved range satisfies the declared hard minimum.",
  "status": "pass",
  "evaluator": {
    "evaluator_id": "EVAL.MISSION.RANGE",
    "crate": "alas-mission",
    "entrypoint": "declared-module-or-function",
    "source_commit": "git-commit",
    "fidelity": "native-alas-mission"
  },
  "inputs": ["artifact-design-brief", "artifact-mission-input"],
  "outputs": ["artifact-mission-result"],
  "condition": {
    "load_case_id": "mission.design",
    "mach": 0.76,
    "altitude_m": 10500.0
  },
  "actual": {
    "value": 3050.0,
    "unit": "km"
  },
  "target": {
    "value": 3000.0,
    "unit": "km",
    "policy": "hard_minimum"
  },
  "residual": {
    "definition": "actual-minus-target",
    "value": 50.0,
    "unit": "km",
    "normalized": 0.0167
  },
  "solver": {
    "status": "converged",
    "iterations": 42,
    "exit_code": 0,
    "stdout_artifact": "artifact-mission-transcript"
  },
  "limitations": []
}
~~~

Suggested evidence status values are pass, fail, not_evaluated, inconclusive,
diagnostic, blocked, and not_applicable. Suggested stage status values retain
the more detailed existing solver vocabulary: not_configured, input_invalid,
input_missing, launch_failed, timed_out, solver_failed, output_missing,
parse_failed, completed_not_comparable, completed_comparable, converged,
partial_convergence, and error. A stage status must not be mechanically
promoted to a requirement status.

The evidence graph follows a small subset of W3C PROV concepts:

- artifacts are prov:Entity-like objects;
- stages, parsers, validators, and evaluators are prov:Activity-like objects;
- ALAS, an external executable, a user, or an organization are prov:Agent-like
  objects;
- derived_from, used, generated_by, and attributed_to are explicit links.

This provides useful graph semantics while keeping the file consumable by
Rust, Python, the GUI, and ordinary JSON tooling.

### 2.3 Manifest audit invariants

An xtask or pipeline audit should reject or downgrade a manifest when:

1. a declared artifact is absent, has a different size, or fails its hash;
2. a stage claims a fresh result without a retained input/output or transcript;
3. a solver reports process success but its parser/status is absent;
4. a comparison is marked compatible without reference area, length, frame,
   moment origin, operating condition, and quantity-level checks;
5. a hard requirement has no primary evaluator, or has a pass without evidence;
6. a physical residual is replaced by an evaluation-failure residual, or vice
   versa;
7. a number has no unit, or a CPACS boundary conversion has no source and target
   unit;
8. a report claim or figure has no evidence IDs;
9. a random seed, optimizer bounds, worker count, tool identity, schema
   identity, or source commit needed for reproduction is missing;
10. an externaldata reference was not resolved, hashed, and cycle-checked.

The existing xtask evidence check is a good starting point: it checks fixture
and generator linkage and deliberately says that presence/linkage is not
physical correctness. The proposed run audit extends that discipline to
content hashes, stage status, requirement evidence, and report claims.

## 3. Requirements-to-evaluator trace model

The requirements-first document already states the key rule: every requirement
has a value/unit, load case, policy, evidence, and status; hard requirements
govern feasibility; unsupported translators produce not-yet-evaluated or
diagnostic evidence, never a false pass.

A normalized requirement record should be:

~~~json
{
  "requirement_id": "REQ.MISSION.RANGE",
  "source_path": "design_brief.mission.design_range_km",
  "description": "Design range",
  "unit": "km",
  "load_case_id": "mission.design",
  "policy": "hard_minimum",
  "source_kind": "user_brief",
  "source_ref": "brief-field-and-ui-revision",
  "primary_evaluator_id": "EVAL.MISSION.RANGE",
  "status": "inconclusive",
  "evidence_ids": ["E.BRIEF.VALIDATION.001"],
  "unsupported_reason": null
}
~~~

The evaluator registry is a separate, versioned catalogue:

~~~json
{
  "evaluator_id": "EVAL.MISSION.RANGE",
  "crate": "alas-mission",
  "entrypoint": "declared-module-or-function",
  "inputs": ["design-brief", "mass-model", "aero-model", "propulsion-model"],
  "outputs": ["mission-result", "fuel-closure", "range-residual"],
  "fidelity": "native-alas-mission",
  "supported_policies": ["hard_minimum", "soft_target", "maximize"],
  "failure_mapping": {
    "non_converged": "inconclusive",
    "invalid_input": "blocked",
    "physical_miss": "fail"
  }
}
~~~

The initial trace catalogue should cover the following groups. The exact
function names can be filled in as each evaluator becomes a stable public
contract; the crate/module ownership is already visible in the repository.

| Requirement ID | DesignBrief or legacy source | Primary evaluator boundary | Minimum result/evidence |
|---|---|---|---|
| REQ.MISSION.RANGE | design_brief.mission.design_range_km and design_range_policy | alas-mission mission result | Achieved range, residual in km, mission convergence, reserve convention |
| REQ.MISSION.PAYLOAD | design_brief.mission.design_payload_kg and design_payload_policy | alas-payload plus alas-mass | Loaded payload, mass closure, MTOW/MZFW margin, payload residual |
| REQ.MISSION.RESERVE | design_brief.mission.reserve_fuel | alas-mission and alas-prop | Reserve mode, reserve fuel, landing fuel, fuel-closure evidence |
| REQ.ACCOM.PASSENGERS | design_brief.accommodation.design_passengers and maximum_passengers | alas-payload and cabin/accommodation model | Capacity, deck split if prescribed, load-case status |
| REQ.ACCOM.CARGO | design_brief.accommodation.design_cargo_kg and maximum_cargo_kg | alas-payload and cargo manager | Loaded/design/max cargo, hold-volume evidence, policy residual |
| REQ.ACCOM.LD3 | design_brief.accommodation.minimum_ld3_45_count | lower-hold position/capacity model | Count, geometry/position evidence, minimum residual |
| REQ.PERF.CRUISE | performance.cruise_mach, mmo, vmo_m_s and policies | alas-atmo, alas-aero, operating-envelope checks | Mach/speed values, reference atmosphere, envelope status |
| REQ.PERF.CLIMB | performance.ica_m, ttc_min, oei_ceiling_m | alas-perf and alas-prop climb evaluators | ICA/TTC/OEI actuals, convergence, residuals, engine-out condition |
| REQ.PERF.ALTITUDE | performance.maximum_cruise_altitude_m | alas-mission or alas-perf | Maximum altitude and condition-specific evidence |
| REQ.FIELD.TOFL | performance.tofl_m | field-performance evaluator | Take-off field length, configuration/load case, residual |
| REQ.FIELD.LANDING | performance.landing_distance_m | field-performance evaluator | Landing distance, configuration/load case, residual |
| REQ.FIELD.APPROACH | performance.approach_speed_m_s | high-lift and landing-performance model | Approach speed, atmosphere/configuration, residual |
| REQ.AIRPORT.SPAN | performance.span_limit_m | alas-geom plus airport constraint | Span in declared frame/unit, geometry validity |
| REQ.AIRPORT.ACN | performance.acn | pavement-load model | ACN, pavement/aircraft assumptions, residual |
| REQ.STABILITY.CG | legacy stability/mass requirements in the design brief or config | alas-mass and alas-stab | CG envelope, load case, mass-balance evidence |
| REQ.STABILITY.STATIC_MARGIN | legacy stability/mass requirements in the design brief or config | alas-stab | Static margin, reference point, derivative/condition evidence |
| REQ.STRUCT.WINGBOX | structural sizing requirements | alas-struct and Nastran adapter | Loads, material/unit model, sizing status, tables and parser |
| REQ.STRUCT.NASTRAN | Nastran feasibility requirement in the requirements brief | alas-struct Nastran boundary | Deck hash, solver status, output-table status, structural finding |

The current RequirementsAcceptanceSummary is a useful human-facing grouped
summary for brief, mission, accommodation, field performance, and structures.
It should remain, but each grouped status should link to the individual
requirement records and evidence IDs. In particular, the existing
requirements-first comments correctly keep mission, accommodation, and field
checks inconclusive until the brief-to-legacy translation is implemented.
That is the right conservative status, not a temporary nuisance to hide.

## 4. Geometry, configuration, and design-space versioning

### 4.1 Freeze boundaries

At run launch, freeze and hash:

- the serialized DesignBrief and the effective legacy AlasConfig;
- the CPACS document after externaldata resolution;
- CPACS schema bytes, schema URL/version, and validator;
- the design vector, variable ordering, bounds, scales, and constraint policy;
- pipeline options, solver selections, timeouts, tolerances, and output policy;
- tool executable/package identities and licence/access classification;
- Cargo.lock, ALAS commit, clean/dirty state, and captured diff where dirty;
- operating system, architecture, compiler/runtime, and allowlisted environment;
- random seed, optimizer algorithm, worker count, and candidate order.

The design snapshot hash must include all values that can change a result. A
run ID must not be used as a substitute for content identity. A useful
candidate cache key is:

    sha256(design_snapshot_id + canonical_design_vector + evaluator_id
           + evaluator_version + input_artifact_hashes + operating_point)

If an evaluation is reused from cache, record the cache key, original run ID,
and a replayed/reused status. Do not silently turn a cache hit into a fresh
solver run.

### 4.2 Versioning rules

Use distinct fields for:

- model version: the document's human/process version;
- schema identity: exact schema version, URL, namespace, and hash;
- design snapshot: the canonical content identity;
- tool version: executable/package/version/build hash;
- run identity: one execution context;
- artifact identity: content hash and producer;
- evidence version: the semantics of the evidence record schema.

For CPACS, keep stable UIDs across a design lineage where the entity remains
the same, and create a new UID when an entity is replaced. Record parent
snapshot and migration edges. Do not infer lineage from a file name or a
timestamp.

### 4.3 Units and frames

ALAS should keep SI-normalized internal values, but every boundary record must
state:

    source_unit -> normalized_unit -> reporting_unit

For dimensionless coefficients, record the reference area, reference length,
moment origin, axis convention, and coefficient definition. For all geometry
handoffs, record the transform from CPACS coordinates to tool coordinates and
back. “Same number” is not “same physical quantity”.

The existing alas-units coverage/parity tests and the CPACS SI convention
should feed the same manifest. A conversion test is evidence of a conversion
implementation; it is not evidence that a particular aircraft requirement
passed.

## 5. Reproducible campaigns and failure preservation

A campaign manifest should include:

- campaign ID, objective/constraint definition, optimizer, version, and seed;
- initial design snapshot, bounds, scaling, population/iteration settings;
- deterministic candidate index and parent/variation metadata;
- every candidate vector, not only feasible or best candidates;
- stage-by-stage statuses, residuals, cache keys, and retained artifacts;
- parallel worker count, evaluation order, timeout policy, and retry policy;
- ranking policy separating physical violation from evaluation failure;
- final selection rationale and all hard requirements' evidence IDs.

The existing alas-opt ConstraintResidual already distinguishes a physical
residual from an EvaluationFailure and keeps normalized violation separate.
The campaign record should expose that distinction instead of reducing a
failed external evaluation to a large arbitrary physical number.

Use three reproducibility levels in reports:

1. bitwise: same executable, dependency tree, machine/runtime assumptions, and
   deterministic ordering reproduce bytes;
2. numerical: outputs match declared tolerances under a documented environment;
3. evidentiary: the same requirements, evaluator contracts, status semantics,
   and physical conclusions can be reconstructed even if bytes differ.

The third level matters for external solvers and parallel floating-point
reductions. It is not a license to omit tool versions, tolerances, or seeds.

Recommended run layout:

    outputs/<run-id>/
      manifest.json
      authority/design_brief.json
      authority/aircraft.cpacs.xml
      authority/schema/cpacs_schema.xsd
      stages/<stage-id>/inputs/
      stages/<stage-id>/outputs/
      stages/<stage-id>/logs/
      evidence/<evidence-id>.json
      reports/report.json
      reports/figures/<figure-id>.(svg|pdf|png)
      checksums.sha256

The manifest is the index; the files remain inspectable without a database.
An optional RO-Crate export can package the same run for sharing.

## 6. Report and visual traceability

ALAS already has stable FigureDescriptor IDs and required-stage metadata, and
the report path uses a shared scene for vector/PDF output. Extend that
contract with a report sidecar:

~~~json
{
  "figure_id": "mission_profile",
  "required_stage": "mission",
  "artifact_id": "artifact-mission-plot",
  "sha256": "sha256-of-figure",
  "source_scene_artifact": "artifact-mission-scene",
  "claim_evidence_ids": ["E.REQ.MISSION.RANGE.001"],
  "display_status": "accepted",
  "limitations": []
}
~~~

Every table row, headline metric, and figure that could be read as a design
claim should point to evidence. A report should visibly distinguish:

- accepted requirement evidence;
- rejected physical requirement evidence;
- not evaluated or inconclusive evidence;
- diagnostic solver output;
- completed but not comparable external output;
- visual context that carries no acceptance claim.

The figure registry's required stage provides the first guardrail. The
manifest/report sidecar adds the second: a reader can follow

    report claim -> figure/table -> evidence -> evaluator
      -> output artifact -> input artifact -> tool/environment

Do not include a native polar in a comparison plot merely because it parsed.
Use the existing comparable/not-comparable gate and annotate the report with
the rejected comparison reason when useful. Keep raw solver output available
from the report, but do not turn transcript text into a fabricated physical
coefficient.

## 7. Mapping to the existing ALAS repository

| Existing owner | Current capability | Proposed digital-thread responsibility |
|---|---|---|
| crates/alas-config | DesignBrief, policies, units/validation, schema metadata | Serialize a canonical brief snapshot; assign stable requirement IDs; record source/policy/load-case metadata |
| crates/alas-pipeline | CPACS import/export, stage orchestration, PipelineOptions, result aggregation | Own run root, manifest assembly, frozen snapshot, stage graph, CPACS schema identity, and artifact hashes |
| crates/alas-pipeline/src/cpacs.rs | CpacsRunManifest with CPACS input/version/UIDs, stage statuses, artifact paths | Evolve it compatibly into the run-manifest authority; add schema hash, artifact records, tool/environment identity, and evidence references |
| crates/alas-pipeline/src/avl.rs | Explicit AVL status and quantity-level comparability | Serialize status, deck/session/output hashes, comparison rejection reason, and comparable quantities as stage evidence |
| crates/alas-pipeline/src/vspaero.rs | Explicit VSPAERO status and quantity-level comparability | Same contract as AVL, including mesh/setup freshness and reference definitions |
| crates/alas-aero/src/mses.rs | Sweep status and per-point convergence diagnostics | Emit one evidence item per requested point plus a sweep-level partial/complete status |
| crates/alas-exec | Tool discovery, process launch, timeout, stdout/stderr capture | Record executable path identity, version/build hash if available, launch environment, exit code, timeout, and log artifacts |
| crates/alas-opt | Candidate generation, history, residuals, evaluation-failure category | Persist candidate IDs, vectors, parent/order/cache keys, all residuals, and failure evidence |
| crates/alas-math/src/hybrd.rs | Detailed nonlinear-solver termination status | Map only Converged to solver success; retain exact termination outcome and numerical settings |
| crates/alas-mission/src/solve.rs | Segment-level integration of the HYBRD result with physical controls and aerodynamic-domain evidence | Retain raw HYBRD status/evaluations; record `Minpack` versus bounded `ResidualQualifiedNoProgress` basis and final maximum absolute scaled force residual |
| crates/alas-struct | Nastran deck/result boundary and result status | Add load-case/unit/frame/parser evidence around existing NotRun/Ok/Error results |
| crates/alas-acceptance | Grouped requirements acceptance status | Keep the human summary; add per-requirement records and linked evidence IDs |
| crates/alas-report and alas-viz | Stable figure registry and shared scenes/PDF output | Emit report/figure sidecars with source artifact hashes and claim evidence IDs |
| crates/alas-gui | Wizard and result presentation | Show requirement policy, evidence status, solver diagnostics, and source links beside the aircraft/result |
| xtask/src/evidence.rs | Golden manifest/generator/fixture linkage checks | Add opt-in or versioned audits for hash integrity, schema identity, stage/evidence linkage, and report claims |
| golden/ and docs/PORTING.md | Parity and provenance ledger | Treat each fixture as a reproducible evidence package with rights, generator, runtime, hashes, and consumer links |

The existing CpacsRunManifest is intentionally small and path-oriented. The
recommended implementation is to extend it with a versioned nested manifest or
to introduce a new serializable RunManifest while retaining a compatibility
view for existing consumers. Do not make external solver output optional
metadata after the fact; adapters should construct their stage evidence at the
same boundary where they know whether output is fresh, parsed, and comparable.

The first implementation can stay inside existing crate boundaries. A new
cross-cutting alas-evidence crate is justified only after the JSON contract is
stable; otherwise it risks becoming a second source of types for statuses,
units, and requirements.

## 8. Incremental implementation sequence

### Phase 1: contract and capture

1. Pin the CPACS release/schema URL selected by ALAS and record the resolved
   schema hash.
2. Define manifest/evidence JSON schemas and status enumerations.
3. Add canonical serialization/hash helpers with explicit float rejection for
   NaN and infinity.
4. Extend the pipeline manifest with design-brief hash, CPACS hash/schema,
   commit/dirty state, tool identities, environment allowlist, and artifact
   hashes.
5. Serialize existing AVL/VSPAERO/MSES/Nastran statuses without changing their
   physical behavior.

### Phase 2: requirements and campaigns

1. Assign stable requirement IDs to the requirements-first brief.
2. Add evaluator registry entries and per-requirement evidence.
3. Persist all optimizer candidates and distinguish physical residuals from
   evaluator failures.
4. Record seed, order, worker count, retries, cache keys, and candidate lineage.
5. Keep grouped RequirementsAcceptanceSummary as a report/UI projection.

### Phase 3: report and audits

1. Add report/figure sidecars and evidence links to stable FigureDescriptor IDs.
2. Add manifest checks for path/hash/unit/status/evidence completeness.
3. Add a reproducibility fixture with a small CPACS input, a deterministic
   in-process stage, one declared unavailable external solver, and a report.
4. Extend xtask/golden checks only after coordinating any required
   docs/PORTING.md ledger rows. This research task deliberately does not alter
   the existing ledger or source files.

### Phase 4: sharing

1. Export a run as an RO-Crate-compatible package when a user requests a
   shareable research artifact.
2. Include a README, licence/rights notes, software citation, machine-readable
   manifest, checksums, and a human-readable report.
3. Preserve restricted or licensed solver components as references and
   metadata, not as redistributed binaries or manuals.

## 9. Source registry

Access date for the registry: 2026-08-26. The URL column is the exact source
URL used for the local copy or citation. SHA-256 is over the local file bytes.

### 9.1 Legally accessible local research copies

The NASA NTRS records used here identify the publications as NASA work/public
use or public-release material where applicable. The DLR records are
open-access/preprint records. FAIR and Sandve are open-access/CC BY sources.
The AVL primer is MIT-hosted; its local copy is retained for research
reference, but no redistribution permission is inferred from hosting alone.

| ID | Local file | Metadata | Bytes | SHA-256 | Exact source URL |
|---|---|---|---:|---|---|
| SRC.DLR.CPACS.2020 | bib/digital-thread/alder2020_cpacs.pdf | Alder, Moerland, Jepsen, Nagel; Recent Advances in Establishing a Common Language for Aircraft Design with CPACS; Aerospace Europe Conference, 2020 | 2161382 | 330eb559eb2c97574cfa87a78536f996f2fdfc913a793d860c089ba2e0ddb8cb | https://elib.dlr.de/134341/1/AEC2020_174.pdf |
| SRC.DLR.TIGL.2019 | bib/digital-thread/siggel2019_tigl.pdf | Siggel, Kleinert et al.; TiGL – An Open Source Computational Geometry Library for Parametric Aircraft Design; Mathematics in Computer Science, 2019 | 4908445 | d627a242b522fc809d8c7b99c30eafd097294d422a4618b00a88dba3e14a67cb | https://elib.dlr.de/124524/1/1810.10795.pdf |
| SRC.DLR.AGILE.2019 | bib/digital-thread/walther2019_agile_aerostructural.pdf | Walther et al.; Integration aspects of the collaborative aero-structural design of an unmanned aerial vehicle; CEAS Aeronautical Journal, 2019 | 1785054 | 2d033627638cd56467baa2119e035347adea3fe1e821a3da57ac63e9b119816b | https://elib.dlr.de/129061/1/Walther2019_Article_IntegrationAspectsOfTheCollabo.pdf |
| SRC.NASA.OPENVSP.2010 | bib/digital-thread/hahn2010_openvsp.pdf | Hahn; Vehicle Sketch Pad: a Parametric Geometry Modeler for Conceptual Aircraft Design; AIAA 2010-657, NASA NTRS, 2010 | 5707615 | 1ead1a613da9087cf249e86f26cc227b4f6bbceeb4b92ca6c485b7521f5d864b | https://ntrs.nasa.gov/api/citations/20100003046/downloads/20100003046.pdf |
| SRC.NASA.VSP.DG.2016 | bib/digital-thread/vsp2016_degenerate_geometry.pdf | Multi-Disciplinary, Multi-Fidelity Discrete Data Transfer Using Degenerate Geometry Forms; NASA, 2016 | 1297307 | 7f97375051b1cb919a570e6f203786440a0517ef8268aa0f15489895ed336143 | https://ntrs.nasa.gov/api/citations/20160010160/downloads/20160010160.pdf |
| SRC.NASA.OPENMDAO.2014 | bib/digital-thread/openmdao2014_framework.pdf | Heath, Gray et al.; OpenMDAO: Framework for Flexible Multidisciplinary Design, Analysis and Optimization; NASA, 2014 | 379940 | 1c0c01839fb1cd4a761d55bab99b6ceca963f68c0dac1bcfe1c08e2d76d756fa | https://ntrs.nasa.gov/api/citations/20140016748/downloads/20140016748.pdf |
| SRC.NASA.DE.1004 | bib/digital-thread/nasa_hdbk_1004.pdf | NASA-HDBK-1004; Digital Engineering; NASA, 2020 | 2684683 | 14d40998e65da81710a0c69676a71129bb454ed0183fb44569a5fc472e0182b9 | https://standards.nasa.gov/sites/default/files/standards/NASA/Baseline/0/2020_04_01_nasa_hdbk_1004_approved.pdf |
| SRC.NASA.SE.6105 | bib/digital-thread/nasa_sp_2016_6105_rev2_api.pdf | NASA/SP-2016-6105 Rev 2; NASA Systems Engineering Handbook; NASA, 2016 revision / NTRS 2017 record | 4122125 | 3153ae2e53e29452d5997efafe280a5f05cd21b43a047e988a17e1dd5207a38e | https://ntrs.nasa.gov/api/citations/20170001761/downloads/20170001761.pdf |
| SRC.FAIR.2016 | bib/digital-thread/fair2016.pdf | Wilkinson et al.; The FAIR Guiding Principles for scientific data management and stewardship; Scientific Data, 2016 | 299385 | 47fca15e91f7a4c3b644d289bb282e56adbe7244093f990a6f6fde285b7619e1 | https://escholarship.org/content/qt55x7k3p6/qt55x7k3p6.pdf |
| SRC.REPRO.2013 | bib/digital-thread/sandve2013_reproducible_computational_research.pdf | Sandve, Nekrutenko, Taylor, Hovig; Ten Simple Rules for Reproducible Computational Research; PLOS Computational Biology, 2013 | 131527 | 35648a3880bdddc8b7bd35f504ee5ccbd1ad3856d206acd181db | https://journals.plos.org/ploscompbiol/article/file?id=10.1371/journal.pcbi.1003285&type=printable |
| SRC.MIT.AVL.3.36 | bib/digital-thread/avl_user_primer_3_36.pdf | Drela; AVL User Primer, version 3.36; MIT-hosted documentation | 445276 | 63a5566c19282991559107ab3fa77e8eb1645b1ed03a4724a8a4d68563b72988 | https://web.mit.edu/drela/Public/web/avl/AVL_User_Primer.pdf |

Note: an initial request for the NASA systems-engineering handbook was saved
as bib/digital-thread/nasa_sp_2016_6105_rev2.pdf but begins with HTML and is not
a PDF. It is retained because this task forbids deletion; it is not used as
source evidence. The valid API response is the *_api.pdf file listed above.

### 9.2 Citation-only or web documentation sources

These sources are cited for current documentation, licensing, schema, or
historical context. They were not copied locally when a manual was
license-restricted, rights were unclear, or the web page itself was the
authoritative current source.

| ID | Metadata and rights/access note | Exact source URL |
|---|---|---|
| SRC.DLR.CPACS.WEB | CPACS homepage; current ecosystem, stable/review release information; DLR website | https://dlr-sl.github.io/cpacs-website/ |
| SRC.DLR.CPACS.GET | CPACS download page; release and schema URL; CPACS repository is Apache-2.0 | https://dlr-sl.github.io/cpacs-website/pages/get-cpacs.html |
| SRC.DLR.CPACS.DOCS | CPACS documentation; coordinates, SI convention, externaldata, UID and validation behavior | https://www.cpacs.de/documentation/CPACS_2_3_0_Docs/html/89b6a288-0944-bd56-a1ef-8d3c8e48ad95.htm |
| SRC.DLR.CPACS.REPO | DLR CPACS schemas/examples/docs repository; Apache-2.0; committed pixi.lock and validation checks support reproducible schema tests | https://github.com/DLR-SL/CPACS |
| SRC.DLR.AGILE.POSTER | Collaborative Aircraft Design – The AGILE Paradigm; DLR poster, cited for roles/requirements/tools/process/MDO/decision framing | https://elib.dlr.de/134928/1/Collaborative%20Aircraft%20Design%20-%20The%20AGILE%20Paradigm.pdf |
| SRC.DLR.CPACS.KBE.2024 | Advanced Collaborative Aircraft Design: Complementing CPACS with Knowledge-Based Engineering and Custom Geometry Parametrization; DLRK 2024, DOI 10.25967/630078 | https://elib.dlr.de/210045/ |
| SRC.NASA.OPENVSP.IMPORT | NASA OpenVSP File Import documentation; current import formats, geometry links, mesh and quality caveats | https://www.nasa.gov/reference/openvsp-file-import/ |
| SRC.MIT.AVL.WEB | MIT AVL home/documentation and sample inputs; tool identity and native deck boundary | https://web.mit.edu/drela/Public/web/avl/ |
| SRC.MIT.MSES.WEB | MIT MSES home/manual; commercial licensing by MIT Technology Licensing Office, so the manual was not copied here | https://web.mit.edu/drela/Public/web/mses/ |
| SRC.NASA.NASTRAN.REPO | NASA NASTRAN-93 repository; legacy solver source/manual boundary, cited without copying manuals into the research set | https://github.com/nasa/NASTRAN-93 |
| SRC.NASA.AGARD.1991 | Sobieszczanski-Sobieski; Multidisciplinary Design and Optimization, AGARD workshop paper; NTRS record flags restricted availability, so no local copy | https://ntrs.nasa.gov/api/citations/19930005416/downloads/19930005416.pdf |
| SRC.NASA.AGARD.740 | AGARD-R-740, Fundamentals of Fighter Aircraft Design; citation-only because NTRS indicates NTIS/republication restrictions | https://ntrs.nasa.gov/api/citations/19880011742/downloads/19880011742.pdf |
| SRC.NASA.MBSE.2019 | NASA Model-Based Systems Engineering Strategy; authoritative baselines, requirement tracing, interoperability and digital-twin context | https://ntrs.nasa.gov/api/citations/20190032332/downloads/20190032332.pdf |
| SRC.NASA.DE.STRATEGY | NASA Digital Engineering Strategy Overview; citation-only public NTRS record | https://ntrs.nasa.gov/citations/20230012351 |
| SRC.W3C.PROV.O | W3C Recommendation; PROV-O maps the provenance model to OWL2/RDF; web specification copyright terms apply | https://www.w3.org/TR/prov-o/ |
| SRC.ROCRATE.1.3 | RO-Crate 1.3 specification; JSON-LD research package metadata, Apache-2.0 specification | https://www.researchobject.org/ro-crate/specification/1.3/index.html |
| SRC.ROCRATE.1.2.DATA | RO-Crate data-entity metadata and external-resource/persistence guidance | https://www.researchobject.org/ro-crate/specification/1.2/data-entities.md |

Rights rule for ALAS: retain original rights and licence metadata per source;
do not redistribute MSES software/manuals, commercial solver binaries, or
restricted AGARD/NTRS material merely because a URL is reachable. For a shared
campaign, package the ALAS manifest and hashes plus a source URL and access
instruction when the external tool cannot legally be bundled.

## 10. Risks and open decisions

1. CPACS schema URL spelling and release pinning need a repository decision:
   choose the exact CPACS 3.5 schema release, verify it in CI, and store its
   hash. Do not leave this implicit in a broad “3.5” label.
2. CPACS's SI convention is not a substitute for boundary metadata. The
   manifest must still state units and conversions for every adapter.
3. OpenVSP, AVL, MSES, and Nastran have different file, version, and licensing
   realities. A manifest can identify a missing or restricted tool without
   pretending that its result was evaluated.
4. Nastran structural result status needs more detail around load cases,
   missing tables, parser failures, and unit/frame validation.
5. Parallel optimization may alter floating-point reduction order and candidate
   order. Record both the intended seed and actual scheduling policy.
6. “Reproducible” should be reported with the bitwise, numerical, or
   evidentiary level, not as an unqualified badge.
7. Report visual polish can hide evidence gaps. Every claim-bearing figure
   needs a status and evidence link, and an unavailable solver must be visible
   as unavailable or not comparable.
8. The current worktree contains pre-existing dirty/deleted files, including
   repository-ledger material. This note does not attempt to normalize that
   state; implementation should be done in a separate change with explicit
   fixture/ledger review.

## 11. Practical review checklist

Before calling a design result reproducible and traceable, verify:

- the DesignBrief, effective config, CPACS, schema, bounds, options, and design
  vector have stable hashes;
- CPACS externaldata is resolved and validated, with UID/reference errors
  retained as evidence;
- every tool has a version/build/access identity and every external stage has
  fresh input/output/transcript artifacts;
- axes, reference quantities, operating conditions, load cases, and units are
  recorded at each interchange boundary;
- process status, parser status, comparability, numerical convergence, and
  requirement acceptance remain separate;
- every hard requirement has a primary evaluator, an evidence status, and a
  residual or an explicit not-evaluated reason;
- every campaign candidate, failed stage, retry, and cache reuse is retained;
- report claims and figures link back to evidence and artifact hashes;
- the package includes rights notes, exact URLs, source licences where known,
  checksums, and enough environment/tool metadata to rerun or explain
  differences.

## References used for the design recommendations

The source registry above is the authoritative bibliography for this note.
The most central sources are the DLR CPACS documentation and CPACS/TiGL papers,
NASA's digital-engineering and systems-engineering handbooks, NASA OpenVSP and
OpenMDAO papers, MIT AVL/MSES documentation, and the FAIR, PROV-O, RO-Crate,
and reproducible-computing specifications/guidance.
