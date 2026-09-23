# Requirements-first aircraft conceptual design: research note

Research date: 2026-08-26
Repository: ALAS
Scope: TLAR and stakeholder-needs capture, requirement quality, load-case and policy semantics, architecture synthesis, traceability, verification and validation, the MBSE/MDO boundary, and the representation of certification and operational constraints without claiming to certify an aircraft.

## Executive conclusion

The state of practice is not “put a few performance numbers in an optimizer.” It is an iterative systems-engineering loop:

    stakeholder expectations -> ConOps/scenarios -> needs and TLARs
        -> validated technical requirements -> architecture and allocation
        -> candidate design -> analysis/verification evidence
        -> validation against intended use -> controlled change

For ALAS, the most important implementation consequence is that the design brief must be a canonical, typed requirements model. A requirement needs more than a number and a unit: it needs provenance, a statement kind, an applicability condition, a named load case or scenario, a policy, an evaluator, an evidence status, and an explicit assurance scope. The optimizer consumes an architecture and a coherent set of analysis-ready constraints; it should not silently invent stakeholder intent, load cases, or regulatory meaning.

The recommended certification boundary is equally important. Part 25/CS-25 clauses, FAA advisory circulars, ARP4754B/ARP4761A, and operational rules should be represented as versioned reference obligations or screening constraints. ALAS can report “screened against,” “traceability gap,” “not evaluated,” or “candidate analysis result.” It must not report “certified,” “compliant,” or “approved” without the separate authority, certification program, validated methods, conformity evidence, and approved compliance findings that those words require.

## Evidence and access policy

This note was assembled from official NASA, FAA, U.S. Government, EASA, SAE, ECSS, and open academic sources. PDFs were downloaded only when the source was public-domain, marked for public release, or explicitly open-license. Copyrighted or redistribution-unclear standards and papers are cited with their DOI or official URL but are not copied into the repository.

The two CFR PDFs are annual 2025 snapshots. They are useful for reproducible research and fixture construction, but the current U.S. rule source is the linked eCFR page. The EASA Easy Access Rules pages are consolidated reading aids and are not themselves the legal publication; the applicable current regulation and amendment must be checked separately.

The local PDF corpus is limited to:

    <repository root>\bib\requirements-systems\

No existing source files were changed for this research note.

## Findings from the state of practice

### 1. Stakeholder needs are not yet technical requirements

NASA’s systems-engineering process separates stakeholder expectations from technical requirements, then validates the requirements before allocation. NASA’s guidance treats the ConOps, architecture, requirements, and stakeholder expectations as mutually constraining artifacts that must be iterated until they are consistent. The open civil-aircraft study by He et al. reaches the same conclusion from an aircraft-design perspective: aircraft projects have many stakeholders, their voices need to be elicited and prioritized, and not every stakeholder need becomes a design requirement.

Implication for ALAS:

- Preserve the originating need and its stakeholder, source, rationale, and priority.
- Do not flatten “an airline wants low trip cost” into a hard geometric constraint.
- Capture the transformation from need to TLAR and from TLAR to derived technical requirements.
- Make trade-offs visible. A need may lead to several candidate requirements, a soft preference, or a later-stage objective.

### 2. A good requirement is a verifiable statement with context

NASA, ECSS, INCOSE, and ISO/IEC/IEEE 29148 converge on the same quality properties: clear, unambiguous, singular, feasible, necessary, consistent, uniquely identified, traceable, and verifiable. A requirement should state what is required, not prescribe an implementation prematurely. It should expose assumptions, units, tolerances, interfaces, environment, and operating conditions when they affect interpretation.

For ALAS, “range >= 3000 nmi” is incomplete unless the record also states payload, fuel/reserve convention, cruise condition, atmospheric model, speed, route assumptions, and whether the result is a preliminary analysis or a verified result. A quantity checker should reject missing dimensions and ambiguous units before a solver sees the brief.

### 3. Load cases are first-class semantics, not comments

Part 25 and CS-25 express many obligations in condition-specific form: weights, center of gravity, flight phase, configuration, altitude, temperature, engine state, load distribution, and critical points in the flight envelope. NASA and ECSS likewise require operational context and environmental assumptions to be explicit where they affect requirement meaning.

ALAS should therefore bind a performance or structural requirement to one or more named load-case records. The same requirement may have a family of cases, but the family must be enumerable and auditable. For example, “OEI ceiling” needs a critical-engine state, mass/fuel state, atmospheric condition, configuration, climb criterion, and an evaluator definition. A single global “design point” is not enough.

### 4. Policy semantics must be separate from physical meaning

The value “maximum wingspan” is a physical target. Whether it is a hard gate, a soft preference, a diagnostic, or an objective is a design-policy decision. The value and the policy must not be encoded in the same untyped field.

Recommended policy meanings:

- Hard: failure blocks a candidate or marks it infeasible.
- Soft: violation contributes a visible penalty but does not silently become a hard gate.
- Objective: used to rank already-feasible candidates; it is not a compliance pass/fail.
- Diagnostic: reported for awareness or future work; it never contributes to feasibility.

This distinction is particularly important for constraints that are not currently modeled. “ACN <= pavement PCN” may be a hard requirement in a later airport-compatibility workflow, but if ALAS has no validated pavement model it must be represented as not evaluated or diagnostic, not as an automatic pass.

### 5. Architecture synthesis precedes continuous optimization

NASA’s design-solution process calls for alternative design solutions and trade studies before irreversible decisions. The open LAMBDA framework makes a related aircraft-specific point: requirements, architecture, sizing, geometry, aerodynamics, propulsion, performance, cost, and optimization are separate but connected modules. VLA conceptual-design work uses objective trees, QFD, AHP, morphological matrices, and multi-criteria selection to establish requirements and configurations before optimizing.

ALAS should use a mixed architecture stage:

1. derive a small number of architecture seeds from the brief;
2. make discrete choices explicit (deck count, propulsion arrangement, cabin concept, wing arrangement, etc.);
3. establish bounds and interfaces;
4. run continuous sizing and analysis inside each architecture;
5. compare architectures with the same traceable requirement set.

The MDO boundary should begin after the architecture is coherent enough that discipline models have defined inputs, outputs, interfaces, and fidelity. MDO is then a method for exploring a design space; it is not a substitute for requirements elicitation or architecture definition.

### 6. Verification and validation are different claims

NASA distinguishes:

- verification: did the product or analysis output satisfy its technical requirements?
- validation: does the resulting product or solution satisfy stakeholder expectations in its intended use and environment?

ALAS can perform preliminary requirement verification by analysis, inspection, review, demonstration, or future test hooks. It cannot infer final validation from a converged optimizer. A candidate may satisfy a numerical target while failing usability, maintainability, airport operations, or stakeholder intent.

Every requirement should have a verification method and acceptance criterion before it is baselined. An assessment with no evaluator should be not_evaluated, not pass. An inconclusive or low-fidelity result should remain visible in the report.

### 7. Certification and operation are related but different requirement families

ARP4754B addresses aircraft and system development processes, while ARP4761A addresses the safety-assessment process. FAA AC 20-174 recognizes the earlier ARP4754A as an acceptable development-assurance method, but it is not a type certificate and it does not turn a conceptual model into an approved aircraft. FAA AC 25.1309-1A is an acceptable means of compliance for the system safety requirements under FAR 25.1309; it is not a general replacement for specific Part 25 requirements.

Part 25/CS-25 are type-certification requirements. Part 121 and EASA Air Operations are operational or operator-facing requirements. The two families may constrain the same design variable but have different authorities, applicability, evidence, and lifecycle.

Recommended ALAS language:

- certification_reference: “This candidate is screened against the cited clause and version.”
- certification_input: “This requirement may be an input to a future certification program.”
- operational_constraint: “This scenario reflects an operational rule or operator need.”
- not_a_compliance_finding: “The result is conceptual analysis and is not an approval, finding of compliance, or certificate.”

Avoid one certified: bool field. Use a typed assurance scope, regulatory basis, verification evidence, validation evidence, and current status instead.

## Proposed typed requirements model

The following is Rust-like pseudocode for the model boundary. It is intentionally a data contract rather than an implementation patch. Units, enums, and identifiers should be implemented using the repository’s existing primitives where possible.

    type RequirementId = String;
    type LoadCaseId = String;
    type EvidenceId = String;
    type Revision = String;

    enum RequirementKind {
        StakeholderNeed,
        Tlar,
        DerivedTechnical,
        ArchitectureConstraint,
        CertificationReference,
        OperationalConstraint,
        Assumption,
    }

    enum RequirementStatus {
        Draft,
        Baseline,
        Evaluated,
        Verified,
        Validated,
        Inconclusive,
        NotEvaluated,
        Superseded,
    }

    enum Direction {
        Minimum,       // actual >= target
        Maximum,       // actual <= target
        Exact,
        InSet,
        Boolean,
        Informational,
    }

    enum RequirementValue {
        Scalar {
            value: f64,
            unit: Unit,
            direction: Direction,
            tolerance: Option<f64>,
        },
        Range {
            min: Option<f64>,
            max: Option<f64>,
            unit: Unit,
        },
        Enumeration {
            values: Vec<String>,
        },
        Boolean {
            value: bool,
        },
        Text {
            value: String,
        },
    }

    enum ConstraintPolicy {
        Hard {
            margin: Option<f64>,
        },
        Soft {
            weight: f64,
            target: Option<f64>,
        },
        Objective {
            direction: Direction,
            weight: f64,
        },
        Diagnostic {
            reason: String,
        },
    }

    enum EvidenceStatus {
        UserProvided,
        Preset,
        Derived,
        Evaluated,
        Verified,
        Validated,
        Inconclusive,
        NotEvaluated,
    }

    enum VerificationMethod {
        Analysis,
        Test,
        Inspection,
        Demonstration,
        Review,
        NotApplicable,
    }

    enum AssuranceScope {
        ConceptualScreening,
        PreliminaryDesign,
        FutureCertificationProgram,
    }

    struct Provenance {
        source_id: String,
        source_location: Option<String>,
        author: Option<String>,
        rationale: String,
        assumptions: Vec<String>,
        captured_at: DateTime,
        revision: Revision,
    }

    struct Environment {
        atmosphere: String,       // e.g. ISA, ISA+10 C, hot-day basis
        altitude: Option<Length>,
        temperature: Option<Temperature>,
        wind: Option<Speed>,
        runway_or_surface: Option<String>,
    }

    enum MassState {
        Oew,
        DesignPayload,
        MaxPayload,
        Mtow,
        Mlw,
        Mzfw,
        FuelFraction(f64),
        ExplicitMass(Mass),
    }

    struct LoadCase {
        id: LoadCaseId,
        name: String,
        flight_phase: String,
        mass_state: MassState,
        payload_state: String,
        fuel_state: String,
        cg_state: String,
        environment: Environment,
        engine_state: String,      // all operating, critical OEI, etc.
        configuration: String,     // flap, gear, cabin, propulsive state
        reserve_convention: Option<String>,
        source: Provenance,
    }

    struct VerificationSpec {
        method: VerificationMethod,
        evaluator_id: String,
        acceptance_criterion: String,
        required_fidelity: String,
        evidence_refs: Vec<EvidenceId>,
        limitations: Vec<String>,
    }

    struct Requirement {
        id: RequirementId,
        kind: RequirementKind,
        title: String,
        statement: String,
        source: Provenance,
        value: RequirementValue,
        policy: ConstraintPolicy,
        applicability: String,       // always, scenario, architecture, regulation
        load_cases: Vec<LoadCaseId>,
        parent_ids: Vec<RequirementId>,
        allocated_to: Vec<String>,   // functions, components, or pipeline stages
        related_functions: Vec<String>,
        interfaces: Vec<String>,
        verification: VerificationSpec,
        assurance_scope: AssuranceScope,
        certification_relevance: String,
        status: RequirementStatus,
        evidence_status: EvidenceStatus,
    }

    struct RequirementAssessment {
        requirement_id: RequirementId,
        load_case_id: Option<LoadCaseId>,
        run_id: String,
        evaluator_id: String,
        evaluator_version: String,
        fidelity: String,
        actual: Option<RequirementValue>,
        target: RequirementValue,
        residual: Option<f64>,
        status: EvidenceStatus,
        evidence_refs: Vec<EvidenceId>,
        assumptions: Vec<String>,
        limitations: Vec<String>,
        certification_claim: bool,  // always false in ALAS conceptual runs
    }

    struct RunManifest {
        run_id: String,
        brief_revision: Revision,
        architecture_seed: String,
        candidate_id: String,
        evaluator_set: Vec<String>,
        model_versions: Vec<String>,
        load_case_ids: Vec<LoadCaseId>,
        generated_at: DateTime,
        assurance_scope: AssuranceScope,
        certification_claim: bool,  // always false in ALAS conceptual runs
    }

### Required invariants

1. Every non-informational numeric requirement has a dimensioned value and direction.
2. Every scenario-dependent requirement has at least one named load case.
3. Every derived requirement traces to a parent requirement, assumption, or accepted self-derived rationale.
4. Every baselined requirement has a verification method, evaluator, and acceptance criterion.
5. Every result records the actual, target, residual, load case, model/evaluator version, and limitations.
6. NotEvaluated is a visible state and cannot satisfy a hard requirement.
7. Diagnostic requirements never gate feasibility.
8. Soft constraints and objectives are evaluated only after hard feasibility is known.
9. A regulatory reference includes jurisdiction, document/version, clause, applicability rationale, and evidence scope.
10. No conceptual run emits a certification or compliance verdict. The certification_claim field is an invariant, not a user-editable option.

### Load-case and scenario semantics

The load-case record should be reusable by performance, mass, stability, structures, airport, and operational evaluators. The minimum semantic tuple is:

    requirement -> load-case family -> physical state -> evaluator -> evidence

At minimum, ALAS should be able to distinguish:

| Dimension | Representative values |
|---|---|
| Mass | OEW, design payload, maximum payload, MTOW, MLW, MZFW, explicit fuel fraction |
| Payload | design passengers, maximum passengers, design cargo, maximum cargo, mixed payload |
| Fuel | mission start, reserve remaining, alternate reserve, specified fraction |
| CG | forward, aft, design CG, explicit station |
| Phase | cruise, climb, takeoff, OEI climb, approach, landing, taxi, structural maneuver |
| Environment | ISA, hot day, altitude, wind, runway/surface |
| Engine state | all operating, critical engine inoperative, degraded or failed |
| Configuration | clean, takeoff, approach, landing, gear state, cabin/deck arrangement |
| Policy context | hard constraint, soft preference, objective, diagnostic |

A load-case family should expand into concrete cases before evaluation. For example, “TOFL at MTOW” may expand into sea-level standard day, hot-day, runway-slope, wind, flap, and surface cases. If only the standard-day case is evaluated, the report must say so.

### Residual and feasibility semantics

Use a normalized residual with the sign convention “positive means violation”:

    minimum requirement:
        residual = (target - actual) / scale

    maximum requirement:
        residual = (actual - target) / scale

    exact requirement:
        residual = abs(actual - target) / scale

Choose scale = max(abs(target), tolerance, epsilon) with a documented dimensional normalization. A hard requirement is feasible only if all required concrete cases have a result, all hard residuals are at or below zero after the selected margin, and no evaluator reports failure. A missing result is not zero residual.

For a first implementation, a candidate result should be one of:

    Feasible
    Infeasible
    NotEvaluated
    Inconclusive
    EvaluatorFailed

The aggregate candidate status should preserve the worst meaningful state and list the individual residuals. Do not collapse Inconclusive into Infeasible without a reason, and do not collapse NotEvaluated into Feasible.

## Proposed verification matrix

This matrix is a starting contract for the requirements-first workflow. It is not a certification compliance matrix. “Analysis” means a model-based conceptual analysis; “test” is a future or external evidence hook unless a validated test model is available.

| Matrix ID | Requirement family | Primary load case | Evaluator and method | Evidence / acceptance | Conceptual limitation |
|---|---|---|---|---|---|
| VM-001 | Design range with reserve | cruise, design payload, mission fuel start/reserve | Mission solver; analysis | achieved range and reserve residual; hard only when mission model is valid | reserves, winds, diversion, and fuel model must be explicit |
| VM-002 | Design and maximum payload | design/max payload mass states | Mass and payload model; analysis plus inspection | mass closure, payload capacity, and CG residuals | does not establish operator loading approval |
| VM-003 | Passenger/cargo/LD3 accommodation | cabin architecture, loaded and empty cases | Cabin/layout and mass model; inspection plus analysis | seat/cargo/ULD count and dimensional capacity | human factors, evacuation, and full cabin certification are outside |
| VM-004 | Cruise Mach and MMO/VMO | cruise altitude, ISA/hot-day family, clean configuration | Aero/performance envelope; analysis | actual speed limit and margin | requires validated aero/flight-envelope methods for later use |
| VM-005 | ICA, TTC, OEI ceiling | MTOW or specified fuel, critical OEI, altitude/temperature | Performance solver; analysis | climb gradient/rate and ceiling residuals | engine-out procedures and operational approval are not inferred |
| VM-006 | Takeoff field length | MTOW, runway/surface, wind, temperature, configuration | Takeoff model; analysis | ground-roll/TOFL residual | runway friction, obstacle, and validation assumptions matter |
| VM-007 | Landing distance and approach speed | MLW, landing configuration, runway/environment family | Landing/approach model; analysis | landing distance and approach-speed residuals | landing demonstration and operational dispatch data are not supplied |
| VM-008 | Wingspan / airport gate envelope | architecture and airport scenario | Geometry and airport-compatibility evaluator; inspection/analysis | span and envelope residual | only a screen until airport data and obstacle/gate definitions are authoritative |
| VM-009 | ACN or pavement compatibility | MTOW/landing mass and specified pavement basis | Airport/pavement model; analysis | ACN versus stated pavement threshold | NotEvaluated or diagnostic if pavement model is absent |
| VM-010 | CG, static margin, trim | forward/aft CG, relevant phase/configuration | Stability and trim evaluator; analysis | static-margin, trim, and control residuals | model fidelity and control-law assumptions must be disclosed |
| VM-011 | Structural preliminary sizing | critical maneuver/gust/load-factor cases | Structural sizing / wingbox; analysis | stress, strain, buckling, or sizing residuals | not a substantiation package or conformity evidence |
| VM-012 | Systems/safety reference | architecture functions and failure assumptions | Review and traceability analysis | cited ARP/AC/Part/CS clauses mapped to functions and gaps | no safety assessment, DAL assignment, or finding of compliance |
| VM-013 | Operational scenario | route, airport, crew, payload, dispatch conditions | Scenario review and performance analysis | scenario assumptions and operational residuals | does not replace AOC/operator approval or operational control |
| VM-014 | Requirement quality | all baselined requirements | Requirements linter and review | unique ID, unit, direction, source, parent, load case, evaluator, acceptance criterion | automated checks cannot replace stakeholder review |
| VM-015 | Validation of the brief | ConOps and intended-use scenarios | Stakeholder walkthrough / review | stakeholder acceptance record and unresolved questions | validation is a project decision, not an optimizer result |

The matrix should be stored as data or generated from the typed requirements model so that the report, GUI, and evaluator use one source of truth. A row may have several concrete load cases and evidence records; the table above is the human-facing baseline.

## Certification and operational representation

### Recommended regulatory-basis record

    struct RegulatoryBasis {
        id: String,
        jurisdiction: String,          // FAA, EASA, other
        authority: String,
        document: String,              // Part 25, CS-25, AC 25.1309-1A, etc.
        version_or_amendment: String,
        clause: String,
        applicability: String,
        requirement_ids: Vec<RequirementId>,
        evidence_scope: AssuranceScope,
        current_source_url: String,
        status: String,                // reference, screened, gap, not evaluated
        compliance_claim: bool,        // always false for ALAS conceptual runs
    }

Keep the regulatory record separate from the physical requirement. One regulatory clause may allocate to several technical requirements, while one technical requirement may be motivated by a regulation, an operator, a customer, or a design assumption.

### Three reporting layers

1. Design requirement result: “The candidate’s preliminary model predicts 2,960 nmi against a 3,000 nmi target under load case LC-CRZ-01.”
2. Assurance trace result: “Requirement R-CRZ-001 is mapped to the cited Part 25/CS-25 or development-assurance reference; verification evidence is analysis at fidelity F1.”
3. Certification boundary statement: “This is not a certification finding. Final compliance requires the applicable authority basis, approved means of compliance, validated methods, conformity inspections, tests, analyses, and program records.”

The second layer is useful for gap management. It must not be rendered as the third layer’s conclusion.

### Suggested status vocabulary for reports

Use screened, traceable, verified_by_conceptual_analysis, not_evaluated, inconclusive, evaluator_failed, and future_certification_input. Reserve compliant, approved, certified, and finding_of_compliance for an actual certification workflow outside ALAS.

## Explicit mapping to REQUIREMENTS_FIRST_AIRCRAFT_DESIGN.md

The repository brief already contains the right conceptual seams. The following mapping turns each seam into a requirements-and-systems-engineering artifact.

| Repository brief section or step | Requirements-engineering interpretation | ALAS data/evaluator consequence |
|---|---|---|
| Purpose: requirements-first rather than optimizer-first | Start from intent, ConOps, needs, and TLARs; do not treat optimization as elicitation | DesignBrief is the canonical input and architecture seed is explicit |
| TLAR contract: value/unit | Dimensioned RequirementValue with direction and tolerance | reject unitless or dimensionally inconsistent inputs |
| TLAR contract: load case | Named LoadCase / load-case family | every scenario-dependent requirement expands before evaluation |
| TLAR contract: policy | ConstraintPolicy with hard/soft/objective/diagnostic semantics | hard gates precede score; diagnostics never gate |
| TLAR contract: evidence/status | provenance and EvidenceStatus | user/preset/derived/not evaluated/inconclusive states remain visible |
| Mission/performance inputs | performance requirements with route, mass, fuel, reserve, environment, and phase | mission solver returns residuals and evidence, not a bare boolean |
| Accommodation inputs | payload, cabin, cargo, ULD, and passenger-mass requirements | cabin/mass evaluator shares canonical payload states and CG cases |
| Step 1: intent and baseline | stakeholder needs, user workflow, baseline provenance, or ground-up intent | create RequirementKind::StakeholderNeed and Tlar; record source and rationale |
| Step 2: mission and route | ConOps scenarios and mission load-case families | route, reserves, airports, and operating environment become explicit inputs |
| Step 3: payload and cabin | stakeholder need to capacity/TLAR transformation | distinguish capacity, mass, geometry, comfort, and operational assumptions |
| Step 4: shape and balance | architecture synthesis, allocation, and design-variable bounds | discrete architecture stage before continuous sizing; trace constraints to geometry/stability |
| Step 5: feasibility | verification matrix and staged requirement evaluator | compute hard residuals first; preserve NotEvaluated and Inconclusive |
| Step 6: review and launch | baseline review, V&V plan, run manifest, and certification disclaimer | freeze brief revision, model versions, load cases, and evidence scope |
| Algorithm: architecture/load-case definition | system design and logical decomposition | emit architecture seed and concrete cases before analysis |
| Algorithm: initial geometry and bounds | design-solution definition and requirement allocation | every bound has a rationale and parent requirement |
| Algorithm: preliminary sizing/feasibility | model-based verification of candidate requirements | no scoring of candidates with unhandled hard requirements |
| Algorithm: analysis | discipline-level verification evidence | evaluator registry identifies methods, versions, fidelity, and limitations |
| Algorithm: optimization | MDO search over a previously coherent design space | objectives rank feasible candidates; they do not repair ambiguous requirements |
| Candidate residuals | quantitative verification result | residual sign convention and scale are stable across reports |
| Staged evaluator | verification plan implementation | stage-level failures are typed; external-tool failures carry evidence and limitations |
| Multi-fidelity | evidence quality and model maturity | fidelity is recorded per assessment; high-fidelity work is required for finalist escalation |
| Explicit “not evaluated” | honest coverage accounting | missing evaluator is not a pass or a zero residual |
| Two workflows: baseline and ground-up | provenance branch, not two schemas | baseline supplies evidence/constraints; ground-up supplies stakeholder intent and architecture alternatives |
| Mapping to pipeline crates | allocation of requirement ownership | GUI captures; config stores; pipeline freezes; opt evaluates; discipline crates provide evidence; report/viz disclose status |
| Recommended boundaries | MBSE-like data model around existing solver components | avoid a second copy of DesignBrief; use adapters and a trace graph |

### Recommended implementation order

P0, requirements contract:

- Make DesignBrief the sole canonical source for captured intent.
- Add typed units, direction, policy, provenance, status, and load-case references.
- Add a requirements linter for missing source, units, parent trace, evaluator, and acceptance criterion.
- Add a run manifest containing brief revision, architecture seed, evaluator versions, load cases, and assurance scope.

P1, verification and feasibility:

- Add a verification-matrix/evaluator registry.
- Produce one assessment record per requirement and concrete load case.
- Implement residual normalization and explicit NotEvaluated / Inconclusive states.
- Gate hard constraints before soft penalties and objectives.

P2, architecture and traceability:

- Introduce architecture seeds and discrete architecture variables.
- Record requirement allocation to functions, interfaces, geometry, mass, performance, stability, and structures.
- Generate requirement-to-evaluator and requirement-to-report trace views.

P3, MBSE and assurance views:

- Export a tool-agnostic requirements/architecture view suitable for SysML or another model repository.
- Add a regulatory-basis catalog with jurisdiction/version/clause and applicability.
- Add validation walkthroughs for ConOps and stakeholder needs.
- Add finalist-only high-fidelity evidence hooks; keep conceptual reports clearly labeled.

## Source register

The register records the evidence claim used in this note. A source is not treated as authoritative merely because it is convenient to download; the access and rights note explains why it is or is not in the local corpus.

### Downloaded public or open-license PDFs

| ID | Source, authors, year | DOI / official URL | Access and rights note | Evidence claim used |
|---|---|---|---|---|
| R1 | NASA Systems Engineering Handbook, Rev. 2, Steven R. Hirshorn, Linda D. Voss, Linda K. Bromley, 2017 | [NASA NTRS PDF](https://ntrs.nasa.gov/archive/nasa/casi.ntrs.nasa.gov/20170001761.pdf) | NASA/NTRS; public-use/public-distribution notice; local file nasa-sp-2016-6105-rev2.pdf | Stakeholder expectations, ConOps, requirements, architecture, verification, validation, requirements management, bidirectional traceability, and iterative SE engine |
| R2 | NASA NPR 7123.1D, NASA Office of the Chief Engineer, 2023 | [Official PDF](https://nodis3.gsfc.nasa.gov/npg_img/N_PR_7123_001D_/N_PR_7123_001D_.pdf); [Chapter 1](https://nodis3.gsfc.nasa.gov/displayDir.cfm?Internal_ID=N_PR_7123_001D_&page_name=Chapter1) | NASA directive; official government publication; local file nasa-npr-7123-1d.pdf | Current NASA requirements for stakeholder expectations, technical requirements, logical decomposition, design solution definition, product V&V, requirements management, tailoring, and iterative processes |
| R3 | NASA Systems Modeling Handbook for Systems Engineering, NASA-HDBK-1009A, NASA Office of the Chief Engineer, technical editor/author named in PDF as Brenda K. Bailey, 2025 | [NASA standards PDF](https://standards.nasa.gov/system/files/tmp/2025-03-12-NASA-HDBK-1009A.pdf); [standard record](https://standards.nasa.gov/standard/NASA/NASA-HDBK-1009) | Approved for public release, distribution unlimited; local file nasa-hdbk-1009a.pdf | Tool-agnostic MBSE metamodel; SysML requirements/structure/behavior/parametric views; requirements tables, traceability, verification matrices, and V&V artifacts |
| R4 | FAA AC 20-174, Development of Civil Aircraft and Systems, Federal Aviation Administration, 2011 | [FAA PDF](https://www.faa.gov/documentLibrary/media/Advisory_Circular/AC_20-174.pdf); [FAA record](https://www.faa.gov/regulations_policies/advisory_circulars/index.cfm/go/document.information/documentID/1019527) | U.S. Government FAA advisory circular; local file faa-ac-20-174.pdf | Recognizes ARP4754A as an acceptable development-assurance method and emphasizes aircraft/system functions, operating environment, requirements validation, design verification, and certification-process assurance; it is not a certificate |
| R5 | FAA AC 25.1309-1A, System Design and Analysis, Federal Aviation Administration, 1988 | [FAA PDF](https://www.faa.gov/documentLibrary/media/Advisory_Circular/AC_25.1309-1A.pdf) | U.S. Government FAA advisory circular; local file faa-ac-25-1309-1a.pdf | Acceptable means of compliance for FAR 25.1309(b), (c), and (d); links failure severity/probability, monitoring, warnings, and crew action; does not replace specific Part 25 requirements |
| R6 | Code of Federal Regulations, Title 14, Part 25, annual 2025 edition, Office of the Federal Register / FAA, 2025 | [GovInfo PDF](https://www.govinfo.gov/content/pkg/CFR-2025-title14-vol1/pdf/CFR-2025-title14-vol1-part25.pdf); [current eCFR Part 25](https://www.ecfr.gov/current/title-14/part-25); [FAA transport regulations](https://www.faa.gov/aircraft/air_cert/design_approvals/transport/transport_regs) | U.S. Government publication; local annual snapshot cfr-2025-title14-part25.pdf; current law must be checked in eCFR | Part 25 organizes certification conditions for flight, performance, weights/CG, controllability, structures, systems, and equipment; requirements are condition-specific |
| R7 | Code of Federal Regulations, Title 14, Part 121, annual 2025 edition, Office of the Federal Register / FAA, 2025 | [GovInfo PDF](https://www.govinfo.gov/content/pkg/CFR-2025-title14-vol3/pdf/CFR-2025-title14-vol3-part121.pdf); [current eCFR Part 121](https://www.ecfr.gov/current/title-14/part-121) | U.S. Government publication; local annual snapshot cfr-2025-title14-part121.pdf; current rule must be checked in eCFR | Operational/operator rules, approved weight-and-balance control, airworthiness/aircraft requirements, manuals, dispatch, crew, and performance are separate from Part 25 type-certification requirements |
| R8 | Stakeholder’s Needs Analysis Methodology for Civil Aircraft Projects, Yuwei He, Haomin Li, Xinai Zhang, Yaming Shi, Fudong Chen, 2021 | [DOI 10.1088/1742-6596/1827/1/012117](https://doi.org/10.1088/1742-6596/1827/1/012117); [open PDF](https://iopscience.iop.org/article/10.1088/1742-6596/1827/1/012117/pdf) | IOP Journal of Physics: Conference Series; open access CC BY 3.0; local file he-et-al-2021-stakeholder-needs-aircraft.pdf | Aircraft stakeholders are numerous; needs should be elicited, classified, prioritized, and transformed into quantifiable technical requirements using methods such as QFD/AHP rather than copied directly |
| R9 | A Framework for Aircraft Conceptual Design and Multidisciplinary Optimization, Saeed Hosseini, Mohammad Ali Vaziry-Zanjany, Hamid Reza Ovesy, 2024 | [DOI 10.3390/aerospace11040273](https://doi.org/10.3390/aerospace11040273); [MDPI article](https://www.mdpi.com/2226-4310/11/4/273); [PDF](https://mdpi-res.com/d_attachment/aerospace/aerospace-11-00273/article_deploy/aerospace-11-00273-v4.pdf) | MDPI open access, CC BY 4.0; local file hosseini-et-al-2024-lambda-framework.pdf | LAMBDA separates requirements, weight, sizing, geometry, aerodynamics, engine, performance, cost, emissions, and optimization; modular interfaces and multiple fidelities support a controlled MDO boundary |
| R10 | Digital Requirements Engineering with an INCOSE-derived SysML Meta-model, James S. Wheaton, Daniel R. Herber, 2024 | [DOI 10.48550/arXiv.2410.21288](https://doi.org/10.48550/arXiv.2410.21288); [arXiv PDF](https://arxiv.org/pdf/2410.21288) | Author preprint on arXiv; CC BY-SA 4.0 per arXiv record; local file wheaton-herber-2024-digital-requirements-sysml.pdf | A requirements metamodel can encode quality attributes, V&V relationships, architecture links, and an authoritative source of truth; digital traceability is valuable but tool integration remains a challenge |

### Standards, regulations, and guidance cited without local PDF

| ID | Source, authors, year | DOI / official URL | Access and rights note | Evidence claim used |
|---|---|---|---|---|
| R11 | SAE ARP4754B, Guidelines for Development of Civil Aircraft and Systems, SAE International, revised 2023 | [DOI 10.4271/ARP4754B](https://doi.org/10.4271/ARP4754B); [SAE Mobilus record](https://saemobilus.sae.org/standards/arp4754b-guidelines-development-civil-aircraft-systems) | SAE proprietary/paywalled; no PDF downloaded | Current development-process guidance covering aircraft/system functions, operating environment, requirements validation, design verification, certification, and product assurance; excludes detailed software/hardware and safety-assessment methods |
| R12 | SAE ARP4761A, Guidelines for Conducting the Safety Assessment Process on Civil Aircraft, Systems, and Equipment, SAE International, revised 2023 | [DOI 10.4271/ARP4761A](https://doi.org/10.4271/ARP4761A); [SAE Mobilus record](https://saemobilus.sae.org/standards/arp4761a-guidelines-conducting-safety-assessment-process-civil-aircraft-systems-equipment) | SAE proprietary/paywalled; no PDF downloaded | Companion safety-assessment process for civil aircraft/systems/equipment; supports certification planning, applies to new designs and changes, and does not by itself establish a certificate or security assessment |
| R13 | EASA Certification Specifications for Large Aeroplanes CS-25, EASA, current Easy Access Rules page with consolidated revision/amendments | [EASA CS-25 page](https://www.easa.europa.eu/en/document-library/easy-access-rules/easy-access-rules-large-aeroplanes-cs-25); [online CS-25](https://www.easa.europa.eu/en/document-library/easy-access-rules/online-publications/easy-access-rules-large-aeroplanes-cs-25) | EASA consolidated Easy Access Rules page says it is not an official publication; copyright/redistribution conditions unclear; no PDF downloaded | CS-25 is structured into general, flight, structures, design/construction, powerplant, equipment, and flightcrew-interface information; clauses such as CS 25.321 make weights, altitudes, load distributions, and critical cases explicit |
| R14 | EASA Easy Access Rules for Air Operations, EASA, current page dated March 2026 | [EASA Air Operations](https://www.easa.europa.eu/en/document-library/easy-access-rules/easy-access-rules-air-operations); [online publication](https://www.easa.europa.eu/en/document-library/easy-access-rules/online-publications/easy-access-rules-air-operations?erules-id=ERULES-1963177438-11820) | EASA consolidated page; license/redistribution unclear; no PDF downloaded | Operational requirements cover operator/AOC, management, training, flight operations, and related AMC/GM; they should be modeled separately from type-certification clauses |
| R15 | FAA Introduction to Part 121 Air Carrier Certification, Federal Aviation Administration, current web guidance | [FAA page](https://www.faa.gov/licenses_certificates/airline_certification/air_carrier/intro_to_certification) | Official FAA web guidance; no PDF needed | FAA certification is a process for showing an applicant can design, document, implement, and audit safety-critical processes; ALAS conceptual analysis is not that certification process |
| R16 | ECSS-E-ST-10C Rev.1, System Engineering: General Requirements, European Cooperation for Space Standardization / ESA, 2017 | [ECSS record](https://ecss.nl/standard/ecss-e-st-10c-rev-1-system-engineering-general-requirements-15-february-2017/); [online PDF](https://ecss.nl/wp-content/uploads/2017/02/ECSS-E-ST-10C-Rev.1(15February2017).pdf) | ECSS PDF carries ESA/ECSS copyright and license/disclaimer; no PDF downloaded | Customer-system-supplier model, tailoring, system concept and design artifacts, requirements consolidation, requirements justification, and requirements traceability matrix |
| R17 | ECSS-E-ST-10-06C, Technical Requirements Specification, ECSS/ESA, 2009 | [ECSS record](https://ecss.nl/standard/ecss-e-st-10-06c-technical-requirements-specification/); [online PDF](https://ecss.nl/wp-content/uploads/standards/ecss-e/ECSS-E-ST-10-06C6March2009.pdf) | ECSS copyright/license conditions unclear; no PDF downloaded | Requirements should be self-contained, verifiable, tolerance-aware, concise, positive, and state what rather than how; requirements specification types and tailoring are explicit |
| R18 | ECSS-E-ST-10-02C Rev.1, Verification, ECSS/ESA, 2018 | [ECSS record](https://ecss.nl/standard/ecss-e-st-10-02c-rev-1-verification-1-february-2018/) | ECSS copyright/license conditions unclear; no PDF downloaded | Verification strategy/program and artifacts such as verification plan, verification control document, test report, and review-of-design report inform ALAS’s matrix design |
| R19 | ISO/IEC/IEEE 29148:2018, Systems and software engineering: Life cycle processes: Requirements engineering, ISO/IEC/IEEE, 2018 (current after review) | [ISO record](https://www.iso.org/cms/render/live/en/sites/isoorg/contents/data/standard/07/20/72089.html) | Normative standard is paywalled; no PDF downloaded | Defines requirements-engineering processes, information items, content, and format; use as a normative reference, not as a locally redistributed copy |
| R20 | INCOSE Guide for Writing Requirements, version 4, INCOSE Requirements Working Group, 2022/2023 publication | [INCOSE summary](https://portal.incose.org/communities/chapters/chaptersdetail/chapters/requirements-engineering/guide-to-writing-requirements); [download page](https://www.incose.org/docs/default-source/working-groups/requirements-wg/gtwr/incose_rwg_gtwr_v4_040423_final_drafts.pdf?sfvrsn=5c877fc7_2) | Publicly discoverable guide but copyright/redistribution terms are unclear; no PDF downloaded | Practical guidance on needs, requirements sets, attributes, and well-formed requirement construction |

### Open academic and technical sources cited without local PDF

| ID | Source, authors, year | DOI / official or author URL | Access and rights note | Evidence claim used |
|---|---|---|---|---|
| R21 | Process of Establishing Design Requirements and Selecting Alternative Configurations for Conceptual Design of a VLA, B.-Y. Bae, S. Kim, J.-W. Lee, N. Van Nguyen, B.-C. Chung, 2017 | [DOI 10.1016/j.cja.2017.02.018](https://doi.org/10.1016/j.cja.2017.02.018); [publisher page](https://www.sciencedirect.com/science/article/pii/S1000936117300572) | Open-access article page; direct PDF redistribution was not retained because the publisher download was not reliably accessible in this run | Uses objective trees, AHP, QFD, morphological matrices, and TOPSIS to turn design requirements into alternative-configuration decisions before conceptual selection |
| R22 | A Requirements Elicitation Process for a Purposeful General Aviation Aircraft Design Based on Emerging Economies, A. Khandoker, M.A. Hamid, A.S. Shahriar, A.T.S. Rahman, G. Gessl, 2021/2022 | [DOI 10.1017/aer.2021.91](https://doi.org/10.1017/aer.2021.91); [Cambridge record](https://www.cambridge.org/core/journals/aeronautical-journal/article/abs/requirements-elicitation-process-for-a-purposeful-general-aviation-ga-aircraft-design-based-on-emerging-economies/76B4172B35A00C3D2DE15812AEB4DCD4) | Publisher access; no local PDF because redistribution rights were unclear | Uses functional, physical, and behavioral viewpoints, QFD, SysML use cases, and constraint/cost analysis; identifies attainability, clarity, and verifiability as requirement qualities |
| R23 | Multidisciplinary Design Optimization: A Survey of Architectures, Joaquim R.R.A. Martins, Andrew B. Lambe, 2013 | [DOI 10.2514/1.J051895](https://doi.org/10.2514/1.J051895); [author/lab record](https://mdolab.engin.umich.edu/bibliography/Martins2013) | Author/lab record and publisher literature; redistribution conditions unclear; no local PDF | Surveys MDO architectures and decomposition strategies; supports an explicit interface and architecture boundary before discipline optimization |
| R24 | OpenMDAO: An Open-Source Framework for Multidisciplinary Design, Analysis, and Optimization, Justin S. Gray, John T. Hwang, Joaquim R.R.A. Martins, Kenneth T. Moore, Bret A. Naylor, 2019 | [DOI 10.1007/s00158-019-02211-z](https://doi.org/10.1007/s00158-019-02211-z); [NASA NTRS record](https://ntrs.nasa.gov/citations/28870101429650) | NASA record is public metadata; publisher PDF rights unclear and no local copy was found | Open, composable discipline interfaces and optimization infrastructure are useful after the requirements and architecture model has defined meaningful inputs and outputs |
| R25 | A Framework for Enhanced Decision-Making in Aircraft Conceptual Design Optimisation Under Uncertainty, D.H.B. Di Bianchi, N.R. Sêcco, F.J. Silvestre, 2021 | [DOI 10.1017/aer.2020.134](https://doi.org/10.1017/aer.2020.134); [publisher PDF record](https://www.cambridge.org/core/services/aop-cambridge-core/content/view/87B42E1E19A65BBCDDB9F7C343854B8B/S0001924020001347a.pdf) | Publisher article; no local PDF because redistribution rights were unclear | Uses constraint-satisfaction probability, robustness/margins, and uncertainty visualization; supports separating target feasibility from confidence and model maturity |
| R26 | A Systems-Theoretic Articulation of Stakeholder Needs and System Requirements, Alejandro Salado, 2021 | [DOI 10.1002/sys.21568](https://doi.org/10.1002/sys.21568); [Wiley record](https://incose.onlinelibrary.wiley.com/doi/pdf/10.1002/sys.21568) | Publisher article; no local PDF because redistribution rights were unclear | Clarifies distinctions among needs, requirements, elicitation, derivation, and decomposition; supports separate schema kinds rather than one generic requirement string |
| R27 | Requirements Analysis of a Quad-Redundant Flight Control System, John Backes, Darren Cofer, Steven Miller, Michael W. Whalen, 2015 | [DOI 10.1007/978-3-319-17524-9_7](https://doi.org/10.1007/978-3-319-17524-9_7); [arXiv record](https://arxiv.org/abs/1502.03343); [UMN repository](https://conservancy.umn.edu/bitstreams/634c6a38-85e1-40bf-b086-a2553d24f008/download) | Open preprint/repository link; Springer redistribution status unclear, so no local PDF | Assumption-guarantee contracts and compositional verification connect formal requirements to architecture interfaces in an aircraft flight-control example |
| R28 | FRETting about Requirements: Formalised Requirements for an Aircraft Engine Controller, Marie Farrell, Matt Luckcuck, Oisin Sheridan, Rosemary Monahan, 2021 | [arXiv record](https://arxiv.org/abs/2112.04251) | Open preprint record; no local PDF retained | Pattern-based formalization can expose ambiguity in natural-language aircraft requirements and relate parent/child requirements to formal analyses |
| R29 | Requirement-Based Design Verification Processes for Commercial Airplanes, SAE technical paper 2025-99-0350, SAE International, 2025 | [SAE paper record](https://saemobilus.sae.org/papers/requirement-based-design-verification-processes-commercial-airplanes-2025-99-0350) | SAE paper is paywalled/proprietary; no PDF downloaded | Recent industry discussion emphasizes operational needs as design inputs and the effect of requirement precision, authenticity, and completeness on verification and product quality |

## Local PDF manifest and SHA-256 hashes

Hashes were computed with SHA-256 on 2026-08-26 after download. Paths are relative to bib/requirements-systems/.

| File | SHA-256 |
|---|---|
| cfr-2025-title14-part121.pdf | FEBC3FD1C4C26A671D1019510C0FF4C39A3734C9BDCAA7D3B48E81972555E92D |
| cfr-2025-title14-part25.pdf | 780D4E5017DEEAE40A65271FA479714690D7E1689DC0AD9F60626F4D628D49DE |
| faa-ac-20-174.pdf | DED7FB7C8B9404B791DBA76CE96FEAEFC6ABD5343C760ABC7D239D0C14BAC53C |
| faa-ac-25-1309-1a.pdf | 7D590700259749384AE405DF8DA449C480AADF02DBC52A8432D6751EDB9515D6 |
| he-et-al-2021-stakeholder-needs-aircraft.pdf | A10C961AA44F720003FF9BF16F2E9F3F18B53CDBC7E8E58D6E71C8C66A0FFA69 |
| hosseini-et-al-2024-lambda-framework.pdf | BFB483C7439FE283100652311675B8336BE38D925A93BCEBE907A8C20E15582E |
| nasa-hdbk-1009a.pdf | 0433F3E9D7DE8999182E2F64584FF3CBBCEC507B2152AADD4BC48206F16F2CF9 |
| nasa-npr-7123-1d.pdf | 686B0D55D492BFFE7E750A15523F339C5DE41CB253499828B4FE9F2924810E40 |
| nasa-sp-2016-6105-rev2.pdf | 8EEB4887A4DC57A23049DA7DD2ED556833CF98E214B240468D987873164FF688 |
| wheaton-herber-2024-digital-requirements-sysml.pdf | 3360F06534283B21BD2C136ECCD0413F7F5928D826E4BEB2F5083A4444B10C65 |

## Practical acceptance checklist for the ALAS brief

Before a brief is considered ready for candidate generation, verify:

- Every stakeholder need has a stakeholder, source, rationale, and disposition.
- Every TLAR has a unit, direction, value or range, tolerance, policy, and provenance.
- Every scenario-dependent TLAR has a named load case or load-case family.
- Payload, fuel, reserve, CG, environment, configuration, and engine state are explicit where relevant.
- Every derived requirement has a parent trace or accepted self-derived rationale.
- Every hard requirement has an evaluator, method, acceptance criterion, and required fidelity.
- Soft preferences and objectives are separated from feasibility gates.
- Regulatory and operational references have jurisdiction, version, clause, and applicability.
- The candidate report can show actual, target, residual, status, evidence, model version, and limitations.
- Missing coverage is reported as NotEvaluated; it is never silently treated as pass.
- The run manifest states that the result is conceptual screening and makes no certification claim.
