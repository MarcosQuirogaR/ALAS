# Aerodynamics research and staged-fidelity plan for ALAS

Status: resumed from the existing workspace archive on 2026-08-26. The literature search was not restarted. The local source archive is bib/aerodynamics/; every PDF listed below was checked as a PDF and hashed with SHA-256 after download.

This memo is an implementation-oriented synthesis for conceptual aircraft design. It is not a certification method and it does not turn an inviscid or empirical result into a high-fidelity claim. The central recommendation is to make the aerodynamic evaluator return a coefficient, a declared scope, a validity assessment, and typed diagnostics together.

## Executive conclusions

1. ALAS should keep a multi-fidelity funnel. Use atmosphere plus Class-I/II drag buildup and coarse in-process lifting methods for large candidate populations; reserve coupled viscous section analysis, external 3D solvers, and RANS/LES for survivors and finalists.
2. A total drag number must remain decomposable. At minimum, retain parasite/profile, induced, wave/compressibility, trim, interference/excrescence, and propulsion increments separately. A single CD without provenance is not suitable for a requirements-first optimizer.
3. VLM and lifting-line are valuable for attached-flow lift, span loading, induced drag, and stability trends. They do not predict skin friction, shock drag, stall, separated wakes, high-lift maximum lift, or propeller slipstream physics by themselves.
4. Prandtl-Glauert corrections are useful as a limited subsonic reporting correction. They are not a transonic drag model. Drag divergence and buffet need a section or 3D compressible method, calibrated data, or an explicit diagnostic that the phenomenon is not evaluated.
5. High-lift and powered-lift requirements must never pass because a clean-wing model returned a finite number. If deflected geometry, powered flow, or a validated high-lift translator is unavailable, the requirement remains not_evaluated or diagnostic.
6. The cache is part of the physics contract. Its key must include geometry, reference quantities, atmosphere, flight state, control state, propulsion state, solver/version, mesh, model constants, and output frame. A result from a different scope or normalization is a cache miss, not an approximate hit.
7. CFD is evidence, not an oracle. Verification, grid/time sensitivity, turbulence-model sensitivity, experimental comparison, and model-form uncertainty must be retained with the result. The NASA CRM and high-lift workshop history show why this matters even for mature RANS workflows.

## 1. Evidence base and design principles

The archive combines four kinds of evidence:

- NASA reports and technical memoranda for conceptual drag methods, vortex-lattice methods, propeller interaction, common validation geometries, CFD verification/validation, model-form uncertainty, and aerodynamic database construction.
- DLR eLib papers for TAU RANS, transition prediction, high-lift validation, unsteady high-lift deployment, and powered-lift database work.
- Solver documentation and solver papers for AVL, MSES, VSAERO, SU2, and NeuralFoil.
- AGARD and workshop references for reusable validation cases. AGARD-AR-303 is cited but not mirrored locally because the NTRS record warns that portions may be copyright-protected and the official NATO download endpoint was unavailable during this run.

The most transferable lesson across the sources is to separate:

1. code verification: did the implementation solve the stated numerical problem;
2. solution verification: did the selected mesh, timestep, iteration tolerance, and discretization resolve that problem;
3. validation: does the model represent the physical experiment or flight data;
4. prediction: is the new aircraft within the validated applicability envelope.

That separation is the basis for the typed result contract proposed below.

## 2. What the state of practice models

### 2.1 Drag buildup

For conceptual design, the useful decomposition is

CD = CD_profile_or_parasite + CD_induced + CD_wave + CD_trim + CD_interference_or_excrescence + CD_propulsion.

The terms are not all obtained at the same fidelity:

- Parasite drag is usually estimated from wetted area, Reynolds number, Mach number, surface state, form factors, and interference factors. A flat-plate correlation is a useful first estimate, but it does not know the actual pressure distribution, local separation, gaps, protuberances, or contamination.
- Induced drag comes from the 3D lift distribution. A conceptual estimate may use CL^2 / (pi * AR * e), a lifting-line solution, a VLM near-field/Trefftz result, or a higher-order flow solution.
- Wave drag depends strongly on thickness distribution, sweep, lift coefficient, local Mach number, shock position, and shock-induced separation. A Korn-style rise is useful for screening but should be labelled semi-empirical and bounded.
- Trim drag is the drag required to satisfy the moment and control constraints. It is not a universal percentage of total drag. It depends on CG, tail volume, tail efficiency, elevator or stabilizer setting, and the lift carried by the trimming surface.
- Interference and excrescence drag are configuration-dependent. A generic Q factor is an uncertainty parameter until it has been calibrated against a comparable configuration.
- Propulsive drag and lift increments depend on nacelle/pylon geometry, slipstream velocity, swirl, propeller loading, power setting, installation, and whether the propeller is in front of a deflected high-lift system.

NASA-CR-137928, Advanced airfoil design empirically based transonic aircraft drag buildup technique, is a useful historical example of a buildup organized around aircraft geometry and design Mach/lift. The method is evidence for using a parameterized, decomposable buildup, not evidence that one empirical curve is universal for every modern aircraft.

The current ALAS code has two distinct paths that should remain distinct:

- alas-aero::analysis::AeroAnalysis combines in-process VLM outputs with a Raymer-style parasite estimate and a Korn-style wave-rise estimate.
- alas-aero::drag_buildup is the mission Fidelity_Zero chain. It carries component-level fields, per-wing compressibility entries, a viscous lift-dependent induced term, excrescence drag, trim correction, spoiler increment, and explicit reference-area aggregation.

The two paths use different abstractions and should be labelled in persisted output. Their coefficients should not be silently summed or compared as if they were the same model.

### 2.2 Lifting-line and vortex-lattice methods

Lifting-line methods reduce each lifting surface to a spanwise circulation distribution. They are inexpensive, expose the effect of aspect ratio, taper, twist, camber-derived zero-lift angle, and loading shape, and are excellent for induced drag and first-order design sensitivities. Their assumptions include steady attached flow, small perturbations, thin lifting surfaces, and a prescribed or idealized wake.

VLM discretizes the lifting surfaces into vortex elements and solves a linear influence system. Compared with a single lifting line, it can represent multiple surfaces, non-planar geometry, dihedral, wing-tail interaction, and wake geometry more directly. It remains primarily an inviscid method. It does not resolve boundary layers or shocks, and nonlinear post-stall results are outside its basic contract.

NASA-SP-405, Vortex-Lattice Utilization, documents the use of VLM for finite wings, wing-body combinations, arbitrary non-planar configurations, induced-drag optimization, preliminary design, and propulsion-related applications. NASA-CR-2865, A generalized vortex lattice method for subsonic and supersonic flow applications, shows how generalized formulations extend the geometry and Mach scope, but that historical extension should not be interpreted as a guarantee for a modern aircraft without case-specific verification.

NASA's Minimum trim drag design for interfering lifting surfaces using vortex-lattice methodology is directly relevant to ALAS: minimum trim drag is a coupled loading and moment problem, not merely a parasite correction. Its method is a good precedent for keeping wing, canard, tail, and winglet interference in one 3D lifting calculation when the geometry and flow assumptions allow it.

The current ALAS implementations map well to this role:

- alas-aero::vlm is the in-process f64 panel solve and force/moment integrator.
- alas-aero::vorlax is a deliberately separate mission VLM kernel with a different formulation and numeric type.
- alas-aero::fourier_lifting_line is an independent Prandtl/Fourier cross-check with explicit assumptions and no profile, wave, fuselage, moment, or stall model.
- alas-aero::avl is an external AVL protocol/parser for lifting-surface geometry, trim, derivatives, Trefftz drag, and span loading.
- alas-aero::vspaero is an external VSPAERO protocol/parser with explicit reference, frame, and geometry-scope metadata.

The desired ALAS behavior is to preserve independent methods as independent evidence. Agreement between two inviscid methods is useful for implementation and discretization confidence; it is not validation of viscous or transonic drag.

### 2.3 Panel methods

Low-order panel methods solve a potential-flow boundary-value problem on bodies and/or lifting surfaces. Their advantage is a better representation of bodies, thickness, and geometry-induced pressure than a thin lifting-surface VLM at modest cost. Their limitation is the same physical boundary: without a boundary-layer and separation model, they do not predict total viscous drag or separated flow reliably.

NASA-CR-4023, Program VSAERO theory document, describes a nonlinear arbitrary-configuration panel method using source/doublet singularities and an iterative treatment of the flow. The correct ALAS use is as a geometry and pressure/interference method with explicit viscous and separation limitations. It should not be used to replace a viscous drag or high-lift model merely because it includes a fuselage.

OpenVSP/VSPAERO verification examples provide a practical mesh-sensitivity route for theoretical bodies, ellipsoids, delta wings, and other simple configurations. The OpenVSP V&V examples should be treated as solver verification cases. They do not establish that a particular aircraft's drag is validated.

### 2.4 Compressibility and transonic drag rise

For swept subsonic wings, a Prandtl-Glauert-type correction can be written approximately as

beta = sqrt(1 - (M * cos(Lambda))^2).

It can adjust a linear lift slope or a reported angle of attack within an attached, subsonic envelope. It becomes poorly conditioned as the normal Mach number approaches one and does not predict shock location, shock strength, drag divergence, buffet, or shock-induced separation.

A conceptual Korn-style estimate uses an approximate drag-divergence Mach number such as

M_DD = kappa / cos(Lambda) - (t/c) / cos(Lambda)^2 - CL / (10 * cos(Lambda)^3),

followed by a calibrated rise such as K * (M - M_DD)^4 after onset. This is a useful screening term because it responds to sweep, thickness, technology factor, and lift. It is not a substitute for a pressure distribution or a validated transonic polar. The rise should be bounded, marked as semi-empirical, and compared with a section or 3D solver for finalists.

The current ALAS AeroAnalysis behavior is appropriately narrow if it is reported honestly:

- swept_pg_beta clamps the normal Mach component below the singularity.
- compressible_report_alpha moves a reported alpha toward zero-lift alpha but deliberately leaves the lift-based drag polar unchanged.
- wave_drag is zero below a configured onset and uses a Korn-style rise after an estimated drag-divergence Mach.
- drag_buildup::compressibility_drag_wing reports a per-wing crest-critical Mach, divergence Mach, and rise, then converts each wing's coefficient through its own reference area.

These are useful low-fidelity terms. The evaluator should emit CompressibilityCorrectionOnly or TransonicRiseSemiEmpirical diagnostics whenever the result is used near the boundary, and it should never label the outcome as shock-resolved.

### 2.5 Airfoil polars, transition, and separation

Two-dimensional section polars are the bridge between geometry and a realistic 3D conceptual model:

- XFOIL/MSES-style viscous-inviscid coupling can represent pressure, skin friction, transition, boundary-layer growth, laminar separation bubbles, and, within its assumptions, transonic effects.
- A section polar must carry Reynolds number, Mach number, N-factor or transition criterion, forced trips, surface roughness assumptions, and control deflection. A clean polar is not a high-lift polar.
- Section data must be integrated with a 3D loading method. Applying one root-section CD to an entire wing is a model choice and should be visible.
- Near separation, nonlinear interpolation and convergence behavior are physical/model diagnostics, not just numerical inconveniences.

The MSES manual and Drela-Giles paper support using MSES as a finalist or critical-section tool. The current ALAS mses module already preserves a strong boundary: it drives external mset/mses/mplot, returns per-point convergence evidence, retains transition stations, and distinguishes Ok, PartialConvergence, Absent, Incomplete, Timeout, ParseFailure, and other statuses.

NeuralFoil is a fast learned/physics-informed surrogate for airfoil screening. Its paper reports a broad trained input space and speedups over XFOIL, but the correct engineering interpretation is still bounded interpolation within the training and validation distribution. Its smoothness and guaranteed output are useful to an optimizer; they are not proof that a post-stall or transonic result is physically correct for an arbitrary new airfoil. ALAS should use it for cheap section exploration, then recheck selected sections with MSES, experiment, or higher-order CFD.

### 2.6 High-lift systems

High-lift aerodynamics is a separate regime, not a small additive correction to clean-wing lift:

- slats and flaps change geometry, circulation, pressure recovery, wake topology, and stall onset;
- gaps, overlaps, seals, surface roughness, transition, and Reynolds number can change the result materially;
- maximum lift is controlled by separation and often has a broad or hysteretic response;
- deployment is unsteady and may have transient loads and different flow states from the final deployed configuration;
- tail effectiveness and trim change with the high-lift wake.

The NASA High-Lift CRM validation paper and the High-Lift Prediction Workshop retrospective show that high-lift CFD remains sensitive to grid, turbulence model, transition/separation treatment, and experimental uncertainty. The DLR HiLiftPW-1 contribution and DLR transition paper provide useful solver and transition evidence; the DLR unsteady deployment paper shows why a static clean/deflected comparison is not the whole deployment problem.

For ALAS, CLmax, stall margin, take-off field length, landing distance, and approach speed need an explicit high-lift state. If the geometry has no deflectable mesh and no section or aircraft high-lift translator, the output is NotEvaluated, not clean-wing CLmax.

### 2.7 Interference and trim

Interference has at least three distinct meanings and they should not be collapsed:

1. inviscid mutual induction between lifting surfaces;
2. pressure and boundary-layer interference at wing-body, tail-body, nacelle-pylon, or flap junctions;
3. propulsion-induced interference from propeller slipstream, jet exhaust, inlet capture, or powered-lift blowing.

VLM/AVL can address the first, and some geometry-aware empirical factors can approximate parts of the second. A generic interference factor is not a replacement for a geometry-resolved solver or calibration database.

Trim should be solved at the requested load case. A minimal longitudinal trim contract is:

1. choose CG, reference point, mass, and flight condition;
2. solve for alpha and one or more control variables subject to CM = 0;
3. calculate total lift, drag, and control increments;
4. retain the control setting, residual moment, and whether the local linearization was valid.

The current ALAS configuration has an explicit small-incidence probe for closed-form trim and stability estimates, tail efficiency, fuselage destabilizing contribution, drag-polar fit windows, and separate fine final resolution. Those are useful hooks, but a fixed trim_drag_correction_factor must be reported as a calibration or margin, not as a solved trim drag.

### 2.8 Propulsive effects

The NASA propeller-slipstream reports identify the main coupled effects:

- increased local dynamic pressure;
- changed effective angle of attack and lift slope;
- altered span loading and induced drag;
- swirl and radial velocity gradients;
- thrust and normal-force components from inclined propeller inflow;
- interaction with nacelle, pylon, flaps, and tail;
- strong dependence on RPM, nacelle position/inclination, and propeller diameter.

The transonic slipstream report notes that axial velocity increments can increase wave drag substantially even when the total lift or wave-drag effect of swirl is smaller in an idealized case. The propeller high-lift semispan-wing test shows that powered high-lift lift augmentation is strongly coupled to nacelle inclination and deployment. The recent X-57 database report is an important architecture example: more than 2,500 CFD cases were reduced and combined across multiple NASA centers and solvers to represent free flight, ground effect, dynamic derivatives, cruise propulsion, and distributed high-lift propulsion.

The current ALAS flowunsteady contract is deliberately limited to lifting surfaces and explicitly carries scope/frame metadata. It does not include bodies or propulsion, and its control regions record whether deflection was actually applied to geometry. That is the correct safety behavior. A powered-lift evaluator should be a new explicit scope, with propeller geometry or a calibrated actuator/slipstream model, power schedule, local flow model, and validation cases. It should not be inferred from an unpowered VLM polar.

### 2.9 RANS, URANS, hybrid RANS/LES, and LES

The NASA CFD Vision 2030 report places high-fidelity CFD in a broader workflow involving turbulence, transition, separated flow, uncertainty, mesh adaptation, and multidisciplinary design. Rumsey's turbulence-modeling V&V paper emphasizes that RANS model-form error is a major source of uncertainty; DNS and LES are too expensive for routine aircraft design, while hybrid methods improve some scale-resolving behavior at higher cost and complexity.

DLR TAU demonstrates a modern industrial/research solver scope: compressible RANS, hybrid RANS/LES, LES, transition models, steady and time-accurate methods, mesh adaptation/deformation, Chimera, adjoint methods, and HPC execution. The DLR high-lift CRM hybrid RANS/LES work also shows that conclusions can be sensitive to spatial and temporal discretization, initialization, and the RANS background model.

Recommended ALAS allocation:

- do not place RANS/LES in the inner population loop;
- run RANS/URANS only after mass, geometry, low-order aero, trim, mission, and basic structure feasibility;
- use hybrid RANS/LES or LES only for finalists and questions that require resolved unsteadiness or separated-wake evidence;
- require a mesh/time/model sensitivity record and a comparable experiment or benchmark whenever the result is used as acceptance evidence.

## 3. ALAS capability map and gaps

| ALAS capability | Current role | Safe claim | Important limitation |
| --- | --- | --- | --- |
| alas-atmo | Atmosphere and freestream state | Supplies a reproducible SI condition | Atmosphere is not an aero validation |
| alas-aero::analysis | VLM plus parasite and Korn-style wave estimate | Fast aircraft-level hybrid estimate | Parasite is lift-independent; wave rise is empirical; no stall or powered flow |
| alas-aero::drag_buildup | Mission Fidelity_Zero component buildup | Decomposable parasite, induced, compressibility, excrescence, trim/spoiler fields | Its inputs and aggregation are a distinct mission model |
| alas-aero::vlm | In-process f64 vortex lattice | Attached-flow lift, moment, loading, induced drag | Inviscid; no viscous drag, shock, separation, or high lift |
| alas-aero::vorlax | Separate mission VLM kernel | Independent mission solver path | Different formulation and f32 arithmetic; do not silently merge results |
| alas-aero::fourier_lifting_line | Independent lifting-line check | Span loading and Trefftz induced drag cross-check | No profile/wave drag, fuselage, moment, stall, or interference between isolated surfaces |
| alas-aero::avl | External AVL deck and typed output parser | Lifting-surface trim, derivatives, Trefftz loading | Current ALAS scope omits slender bodies and viscous drag |
| alas-aero::vspaero | External VSPAERO setup and polar parser | Independent external inviscid 3D evidence | Reference/frame/geometry metadata is mandatory; inviscid drag must not be overlaid on hybrid total drag |
| alas-aero::mses | External 2D viscous/inviscid solver | Critical-section polar, pressure, transition, and convergence evidence | Section only; tool availability and convergence are explicit |
| alas-aero::neuralfoil | Fast section surrogate | Broad section screening and smooth optimization inputs | Learned envelope; must be rechecked for finalists and extrapolation |
| alas-aero::flowunsteady | Versioned adapter contract | Time-averaged external lifting-surface evidence when an adapter exists | No built-in solver; no body/propulsion; controls may be declared but not applied |
| alas-config::AnalysisConfig | Coarse/fine mesh, sweep, trim probes, polar fit windows | Existing separation between search and reported analysis | Resolution is not a validation statement |
| alas-config::DragModelConfig | Explicit parasite, interference, margin, Korn parameters | Makes empirical assumptions inspectable | Values still need calibration and uncertainty treatment |
| alas-stab and alas-perf | Stability, trim, performance, mission consumers | Convert aero outputs into requirements checks | They must know whether each coefficient is valid for the load case |
| alas-exec | External process execution | Reproducible solver invocation boundary | Tool version, environment, and deck hashes must be retained |
| alas-report / alas-viz | Evidence presentation | Show breakdowns, envelopes, diagnostics, and comparisons | A plot must not hide a not_evaluated state |

The missing architectural seam is a common evaluator result around these existing modules. It should carry the scope and validity of each result without requiring the individual solvers to pretend to have a broader physics model.

## 4. Proposed staged evaluator

### 4.1 Typed contract

The following is a design sketch, not a request to modify source files in this research task.

~~~rust
enum AeroFidelity {
    Screening,
    NativeVlm,
    LiftingLineCrossCheck,
    SectionSurrogate,
    SectionViscous,
    ExternalLiftingSurface,
    HighLiftOrPowered,
    CfdRans,
    CfdHybridLes,
}

enum AeroStatus {
    Evaluated,
    Partial,
    NotEvaluated,
    OutOfEnvelope,
    NonConverged,
    InvalidInput,
    ToolUnavailable,
    ScopeMismatch,
    ReferenceMismatch,
}

enum EvidenceKind {
    Analytic,
    SemiEmpirical,
    InProcessNumerical,
    ExternalSolver,
    Calibrated,
    ExperimentalComparison,
}

struct ValidityEnvelope {
    mach: Range,
    reynolds_per_m: Range,
    alpha_deg: Range,
    beta_deg: Range,
    cl: Option<Range>,
    sweep_deg: Range,
    thickness_to_chord: Range,
    geometry_scope: GeometryScope,
    control_scope: ControlScope,
    propulsion_scope: PropulsionScope,
    validation_level: ValidationLevel,
}

struct AeroRequest {
    geometry_hash: Hash,
    reference_area_m2: f64,
    reference_chord_m: f64,
    reference_span_m: f64,
    moment_reference_m: [f64; 3],
    load_case: LoadCaseId,
    condition: FlightCondition,
    controls: ControlState,
    propulsion: PropulsionState,
    fidelity: AeroFidelity,
    requested_outputs: OutputMask,
}

struct AeroResult {
    status: AeroStatus,
    evidence: EvidenceKind,
    validity: ValidityAssessment,
    coefficients: Option<AeroCoefficients>,
    breakdown: Option<DragBreakdown>,
    trim: Option<TrimEvidence>,
    diagnostics: Vec<AeroDiagnostic>,
    provenance: SolverProvenance,
}
~~~

The important properties are:

- coefficients and breakdown are optional. A missing or invalid result is not represented by a plausible zero.
- validity is attached to each result, not only to the solver configuration.
- provenance contains solver name/version, input/deck hash, mesh/discretization, model constants, reference frame, and source of each correction.
- a Partial result can contain converged points and non-converged points without claiming a complete polar;
- an external result with the wrong reference or frame is ReferenceMismatch, even if all numeric columns are finite.

### 4.2 Validity envelope

The evaluator should store a model-specific envelope in the result. Suggested initial policy envelopes are deliberately conservative and must be calibrated against ALAS benchmarks:

| Fidelity | Initial policy envelope | Do not claim |
| --- | --- | --- |
| Screening buildup | steady conceptual aircraft; subsonic to low transonic screening; moderate attached-flow alpha; declared Reynolds and geometry ranges | validated drag rise, stall, high-lift, or powered performance |
| Native VLM | thin lifting surfaces, steady attached flow, small-to-moderate alpha/beta, normal Mach sufficiently below one for incompressible assumptions | viscous, shock, separation, high-lift, or propulsive drag |
| Fourier lifting-line | symmetric isolated surfaces, small-angle attached incompressible flow | surface interference, bodies, moment, stall, profile drag, or wave drag |
| NeuralFoil/XFOIL-like section | only the section shape, Reynolds, Mach, transition model, and alpha region represented in the solver/training envelope | full aircraft behavior or arbitrary extrapolation |
| MSES | two-dimensional viscous/inviscid section, with the selected Mach, Reynolds, transition, and control setup | 3D junctions, finite-span interference, full-aircraft trim, or propulsive effects |
| AVL/VSPAERO | declared lifting-surface geometry, reference, frame, and solver regime | viscous total drag or a body/propulsion scope not present in the case |
| High-lift/powered | only when deflection, gaps, propulsion state, and validation/calibration data are present | clean-wing substitution for CLmax, TOFL, landing, or powered lift |
| RANS/URANS | the specific geometry, mesh family, turbulence/transition model, boundary conditions, and convergence evidence used | universal accuracy outside the validated case family |
| Hybrid RANS/LES/LES | a finalist problem with demonstrated time/space resolution and a defined unsteady objective | routine population-level optimization or automatic truth |

The numerical bounds should be data, not scattered if statements. A boundary crossing should create a diagnostic and force either a lower-fidelity estimate with a warning or a NotEvaluated result.

### 4.3 Stage sequence

#### Stage 0 - normalize and reject malformed cases

Input:

- materialized aircraft geometry;
- SI reference quantities;
- load case and mass/CG;
- atmosphere and flight state;
- control and propulsion states.

Checks:

- finite positive reference area, chord, and span;
- valid airfoil coordinates and section order;
- unambiguous reference frame and moment origin;
- valid atmosphere and Reynolds inputs;
- explicit control, high-lift, power, and ground-effect flags.

Output: Evaluated geometry metadata or InvalidInput. No aerodynamic coefficient is emitted on failure.

#### Stage 1 - conceptual screening

Use alas-atmo, the existing drag buildup, coarse geometry checks, and a cheap induced-drag estimate. If the candidate has usable lifting geometry, use the coarsest native VLM or a lifting-line estimate for a first CL and CDi.

Outputs:

- required lift coefficient for each load case;
- parasite component estimates and wetted areas;
- induced-drag estimate and span-efficiency proxy;
- semi-empirical wave-rise estimate where applicable;
- a complete breakdown with the terms that were not modeled explicitly;
- preliminary trim/stability feasibility only if the required surfaces are present.

This stage is allowed to reject impossible payload/wing-loading/mass combinations before spending work on airfoil or CFD analysis. It is not allowed to certify high-lift or transonic requirements.

#### Stage 2 - native in-loop 3D aero

Run coarse alas-aero::vlm, stability probes, trim probes, and the existing AeroAnalysis hybrid corrections. Keep the current coarse/fine split: the optimizer uses the cheap resolution; the final report uses the configured fine resolution and does not silently reuse a coarse polar.

Recommended in-loop checks:

- linearity of CL(alpha) over the fit interval;
- finite and consistent CM(alpha) and CM(CL) slopes;
- residual moment after trim;
- VLM matrix/geometry errors;
- consistency between near-field and Trefftz induced drag where both exist;
- agreement trend with the independent Fourier lifting-line result.

If the polar fit window contains too few valid points, return PolarFitInsufficient and preserve the raw points. Do not widen the window silently across stall or shock-induced nonlinearities.

#### Stage 3 - section polar enrichment

Use NeuralFoil or an equivalent fast section surrogate for broad shape/Re/Mach/control screening. For finalists or regions with high sensitivity, run MSES at root, kink, tip, tail, and high-lift sections as appropriate.

The 3D evaluator should record which section polar generated each profile increment. Transition stations, forced trips, convergence state, and extrapolation flags are part of the evidence. A 2D section result should not be interpreted as a full-aircraft pressure or interference result.

#### Stage 4 - external 3D evidence

Dispatch only after lower stages pass or when explicitly requested:

- AVL for lifting-surface trim, stability derivatives, span loading, and Trefftz induced drag;
- VSPAERO for an independent potential-flow setup and mesh/refinement comparison;
- VORLAX or another mission-specific kernel as an independent implementation path;
- FLOWUnsteady only through its reviewed, versioned adapter contract.

Before merging or comparing outputs, require equal reference area, chord, span, moment origin, frame, geometry scope, Mach convention, alpha/beta, controls, and propulsion state. A VSPAERO inviscid CD must not be plotted as if it were the ALAS hybrid total CD.

#### Stage 5 - high-lift and powered-lift evidence

This stage is required for:

- CLmax and stall margin;
- approach speed;
- take-off and landing field length;
- powered-lift or distributed-propulsion requirements;
- trim and tail effectiveness with slats/flaps or slipstream.

The minimum accepted input is a geometry/control/power state that actually changes the mesh or the local-flow model. The result must declare flap/slat deflection, gap/overlap assumptions, transition/roughness assumptions, propeller RPM or thrust state, and whether the tail is in the modified wake.

When these inputs are absent, return:

status = NotEvaluated, diagnostic = HighLiftNotModeled or PropulsionNotModeled, policy = diagnostic.

Do not substitute the clean-wing polar and do not add an undocumented CLmax margin.

#### Stage 6 - finalist CFD

For selected concepts, run steady or unsteady RANS with:

- at least one grid refinement or solution-adaptation comparison;
- convergence histories and force/moment residuals;
- turbulence and transition model declarations;
- boundary conditions, wall treatment, and transition/trip state;
- deformation/control/propulsion state;
- sensitivity to initialization or timestep where separation is important;
- comparison against a benchmark or experiment where possible.

Use hybrid RANS/LES or LES only when the design question requires resolved unsteadiness or separated-wake physics and the computational budget supports it. Persist raw solver outputs and reduced coefficients separately.

#### Stage 7 - reduced aerodynamic database

The database stage converts repeated cases into a reusable, versioned product:

- raw solver cases remain immutable;
- reduced tables retain all coefficient breakdowns and diagnostics;
- interpolation is allowed only inside the declared envelope;
- surrogate predictions retain training domain, version, error metrics, and uncertainty;
- every table row is traceable to a geometry and solver hash.

### 4.4 Typed diagnostics

Suggested stable diagnostic codes:

| Code | Meaning | Default consequence |
| --- | --- | --- |
| InvalidGeometry | Geometry cannot define a valid aero case | blocking |
| InvalidReference | Area, lengths, moment origin, or frame missing/inconsistent | blocking |
| MachOutsideEnvelope | Requested Mach is outside this model's validity range | warning or not evaluated |
| ReynoldsOutsideEnvelope | Section/aircraft Reynolds number is outside the data range | warning or not evaluated |
| AlphaOutsideEnvelope | Alpha is outside the attached/validated interval | warning; never certify stall |
| BetaOutsideEnvelope | Sideslip is not represented by the model | warning or not evaluated |
| CompressibilityCorrectionOnly | Alpha/lift slope was corrected but shock physics was not solved | warning |
| TransonicRiseSemiEmpirical | Wave drag is an empirical rise, not a shock-resolved result | warning |
| ViscousDragSemiEmpirical | Parasite drag came from a buildup rather than a boundary-layer solve | warning |
| BodyInterferenceOmitted | Model includes lifting surfaces but omits body/junction flow | warning |
| TrimNotSolved | Coefficients are untrimmed or residual moment exceeds tolerance | warning or hard residual |
| HighLiftNotModeled | Clean configuration cannot evaluate high-lift requirement | diagnostic |
| PropulsionNotModeled | Power/slipstream/jet interaction is absent | diagnostic |
| StallNotModeled | No physically valid post-stall model is active | diagnostic |
| ExternalToolUnavailable | Requested external solver is missing or disabled | diagnostic |
| ExternalPartialConvergence | Some external points converged, others did not | partial |
| PolarFitInsufficient | Fit window has too few valid points | warning |
| ReferenceMismatch | Comparable-looking outputs use different references or frames | blocking for comparison |
| GridSensitivityUnquantified | A higher-fidelity result has no resolution evidence | warning |
| ModelFormUnquantified | Turbulence/transition/interference uncertainty has no estimate | warning |
| ExtrapolatedSurrogate | A learned/interpolated value is outside its training/data hull | not evaluated |
| NonFiniteOutput | Solver returned NaN, infinity, or malformed output | blocking |

Severity should be separate from status. For example, ViscousDragSemiEmpirical can be an informational warning for a preliminary objective but a blocking diagnostic for a requirement explicitly asking for validated transonic drag.

## 5. Reusable aerodynamic database and cache

### 5.1 Cache key

The minimum canonical cache key should include:

~~~text
geometry_hash
geometry_schema_version
reference_area_m2, reference_chord_m, reference_span_m
moment_reference_m and coefficient_frame
load_case_id and mass_cg_hash
altitude, temperature, pressure, density, Mach, Reynolds
alpha, beta, body_rates
control_surface_state and applied_geometry_hash
propulsion_state, RPM/thrust/power schedule, ground_effect_state
fidelity and solver_id/version
mesh/discretization/iteration/timestep settings
drag_model_config_hash and atmosphere_model_version
requested_output_mask
~~~

Use canonical serialization before hashing. Round only at a declared policy precision; otherwise two physically distinct requests can collide. Include the external executable version and the input deck hash. A solver executable update is a cache miss even if its command line is unchanged.

### 5.2 Record layout

Each cached case should have:

1. request.json: canonical typed request;
2. provenance.json: solver/version, executable hash, deck hash, environment, source model, and rights/provenance where external data is used;
3. result.json: coefficients, breakdown, trim, derivatives, validity, and diagnostics;
4. raw solver outputs and plots where licensing permits;
5. validation.json: benchmark comparison, grid/time sensitivity, calibration source, and uncertainty fields;
6. a stable status and created_at timestamp.

The reduced table must preserve None/missing values. It must never fill a failed CDwave, CLmax, or trim coefficient with zero just to make interpolation rectangular.

### 5.3 Sampling and refinement

Start with a small set of anchor cases:

- clean cruise at design Mach and design lift;
- low-speed clean points;
- alpha sweeps that bracket the expected operating range;
- trim and stability perturbations;
- design payload and maximum payload cases;
- high-lift and powered anchors only when the configuration exists;
- one or more benchmark geometries for each solver path.

Add samples where:

- a requirement residual is sensitive to an aero output;
- the derivative of CL, CD, or CM changes rapidly;
- an interpolation uncertainty exceeds its limit;
- the validity envelope boundary is crossed;
- a solver changes status or becomes non-converged;
- two independent methods disagree beyond the declared comparison tolerance;
- a design variable changes the local Reynolds/Mach or control state enough to invalidate an old section table.

For high-dimensional design spaces, use a structured design-of-experiments seed plus adaptive refinement. Do not create an enormous Cartesian product of all geometry and flight variables by default.

### 5.4 Interpolation and surrogates

- Interpolate only inside the data hull and per configuration state. Do not blend powered and unpowered data without an explicit model.
- Use shape-preserving or bounded interpolation for monotone portions of CL(alpha) and CD where appropriate.
- Keep discontinuities or regime boundaries as separate local models: clean versus high-lift, attached versus separated, power-off versus power-on, subsonic versus transonic.
- A learned surrogate must return an uncertainty or error estimate and an ExtrapolatedSurrogate diagnostic outside its training hull.
- Use leave-one-out or held-out validation by geometry, not only by random operating point. A surrogate can interpolate a polar well while failing on a new planform.
- If an interpolated point is outside the envelope or has inadequate uncertainty, schedule a real solver case rather than silently widening the model.

The X-57 aerodynamic database is a useful process precedent: thousands of CFD cases from multiple solvers were reduced into a flight-simulation model with separate configurations, ground effect, dynamic derivatives, controls, and propulsion states. ALAS can use the same separation of raw evidence and reduced runtime tables at a smaller scale.

## 6. Mapping to requirements-first aircraft design

The requirements document defines each TLAR with value/unit, load case, policy, and evidence/status. Aerodynamic evaluation should follow the same four-part contract.

| Requirement | Aero inputs and output | Current ALAS state | Policy |
| --- | --- | --- | --- |
| Design range and reserve | Mission consumes valid CD, CL, trim, and propulsion data at each segment | Mission/performance path exists; aero validity must be carried into segment results | Hard/soft only when the required aero states are evaluated |
| Design and maximum payload | Mass/CG/load case sets required lift and trim | alas-mass and alas-payload own the load case | Aero reports the exact load case used |
| Cruise Mach | Atmosphere, Mach, Reynolds, sweep, thickness, CL, wave-rise status | AeroAnalysis has PG reporting and Korn-style screening | Mark semi-empirical near drag rise; require finalist evidence for hard transonic claims |
| MMO/VMO | Mach/EAS operating envelope and compressibility/structural margins | Atmosphere and operating checks can consume the limit | Do not infer a validated MMO from a low-order drag curve |
| ICA and time to climb | Climb performance consumes drag and thrust at changing altitude/Mach | alas-perf/mission own the calculation | Residual inherits aero status |
| OEI ceiling | One-engine-out thrust and asymmetric aero/trim state | Propulsion and performance own the translator | Diagnostic until engine-out aero/trim and propulsion effects are modeled |
| Maximum cruise altitude | Residual climb rate at the stated atmosphere and load case | Mission/performance requirement | No pass if drag is out of envelope or untrimmed |
| TOFL, landing distance, approach speed | CLmax, high-lift drag, trim, powered lift, ground effect | Full high-lift translator is not present in the current clean lifting mesh | NotEvaluated/diagnostic rather than clean-wing pass |
| Wingspan limit | Geometry span and airport constraint; span also changes induced drag | alas-geom and requirements layer | Hard geometric residual; aero is supporting evidence |
| ACN | Pavement/load model | Outside alas-aero | Keep separate, but retain the same evidence/status pattern |
| CG and static margin | CM(alpha), derivatives, trim control state, tail wake efficiency | VLM/AVL/stability probes and alas-stab | Hard or soft only with frame/reference and trim residuals |
| Wingbox/Nastran feasibility | Aero loads, span loading, load cases | alas-struct owns the structural stage | Use resolved load provenance and do not treat VLM loads as validated ultimate loads |
| Passenger/cargo accommodation | Geometry and mass alter lift/trim but capacity is not an aero coefficient | alas-payload and geometry own accommodation | Keep capacity and aero load case distinct |

Every requirement residual should be able to point to the aero case IDs that produced its coefficients. If a translator is absent, retain:

~~~text
ConstraintResidual {
    name: approach_speed,
    actual: None,
    target: 135 KCAS,
    direction: maximum,
    normalized_violation: None,
    policy: diagnostic,
    evidence: not_evaluated,
    diagnostics: [HighLiftNotModeled, StallNotModeled]
}
~~~

When a value is available, use the requirements document's residual style and add aero_case_id, fidelity, validity, and diagnostics. A hard requirement must not be made feasible by a low drag objective.

## 7. Verification, validation, and uncertainty plan

### 7.1 Verification ladder

1. Unit tests for singularity kernels, panel geometry, influence matrices, linear algebra failure modes, coefficient frames, and reference-area conversions.
2. Analytic or near-analytic checks for flat plates, elliptic loading, Prandtl lifting-line trends, and zero-lift/camber behavior.
3. Mesh refinement for native VLM and external AVL/VSPAERO comparisons. Track CL, CM, CDi, loading, and stability derivatives separately.
4. Section solver checks for NACA 0012, RAE 2822, NLR 7301, OAT15A, and multi-element high-lift cases where the input data and boundary conditions are available.
5. Aircraft validation cases such as DLR-F4, DLR-F6, NASA CRM, and the NASA trapezoidal high-lift wing. Follow the exact geometry, reference, Mach, Reynolds, transition, and tunnel correction definitions.
6. Propulsion/high-lift cases only with a declared propeller geometry, operating state, and comparable experimental or CFD data.

### 7.2 Validation record

For each validation comparison retain:

- observable and reference quantity;
- geometry and flow-condition identity;
- experiment source, measurement uncertainty, and tunnel corrections;
- solver and model configuration;
- grid/time resolution and convergence;
- predicted value, measured value, signed error, and normalized error;
- whether the comparison tests a trend, an absolute value, or a derivative;
- applicability statement for ALAS use.

Do not reduce all validation into a single global percentage. A method can have good lift-slope agreement and poor separated drag, or good induced drag and no valid wave drag.

### 7.3 Uncertainty buckets

Store at least:

- input uncertainty: mass, CG, atmosphere, geometry, surface roughness, transition;
- numerical uncertainty: grid, panel density, timestep, iteration tolerance, interpolation;
- model-form uncertainty: turbulence, transition, separation, interference, wave-rise correlation, propeller model;
- experimental uncertainty: bias, repeatability, wall interference, instrumentation;
- ensemble spread: disagreement among independently configured methods at a common scope.

NASA's model-form uncertainty knowledge-base work is a useful precedent for recording applicability and evidence rather than assigning an unexplained global error bar. For ALAS, an uncertainty estimate can begin as a documented interval or sensitivity envelope and become calibrated as benchmark history accumulates.

## 8. Unresolved and citation-only sources

These sources remain useful but were not copied into the local PDF archive:

| Source | Use in this memo | Why citation-only |
| --- | --- | --- |
| AGARD Fluid Dynamics Panel Working Group 4, A Selection of Experimental Test Cases for the Validation of CFD Codes, Volume 2, AGARD-AR-303-VOL-2, 1994, ISBN 92-836-1003-2, <https://ntrs.nasa.gov/citations/19950011431> | 39 validation cases across 2D airfoils, 3D wings, slender bodies, delta wings, and complex configurations | NTRS says public distribution but portions may include copyright-protected material; official NATO download endpoint was unavailable |
| William L. Oberkampf and Timothy G. Trucano, Verification and Validation in Computational Fluid Dynamics, SAND2002-0529, 2002, DOI <https://doi.org/10.2172/793406>, <https://www.osti.gov/servlets/purl/793406> | Terminology and separation of verification, validation, error, and uncertainty | Public-release source was identified, but the OSTI PDF endpoint timed out during the resumed run; no local placeholder was created |
| Tobias Knopp, Validation of the Turbulence Models in the DLR TAU Code for Transonic Flows - A Best Practice Guide, DLR-FB 2006-01, 2006, <https://elib.dlr.de/45853/> | TAU transonic validation practice and caution about claiming accuracy | DLR eLib record is not open access |
| Stefan Langer, Axel Schwöppe, and Norbert Kroll, The DLR Flow Solver TAU - Status and Recent Algorithmic Developments, AIAA 2014-0080, 2014, DOI <https://doi.org/10.2514/6.2014-0080>, <https://elib.dlr.de/90979/> | Solver architecture, efficiency, and algorithmic context | DLR eLib record is not open access |
| Charles W. Boppe, Aircraft Drag Analysis Methods, AGARD special course material, 1991/1992, <https://www.kimerius.com/app/download/5784131944/Engineering%2Bmethods%2Bin%2Baerodynamic%2Banalysis%2Band%2Bdesign%2Bof%2Baircraft.pdf> | Component, interference, trim, and propulsion drag taxonomy | Original rights and redistribution terms were not clear enough to mirror |
| M. H. Rizk, Propeller slipstream/wing interaction in the transonic regime, AIAA 80-0125, 1980, <https://ntrs.nasa.gov/citations/19800038563> | Axial slipstream velocity and swirl effects on transonic wing flow | NTRS marks copyright as Other; no local PDF was needed for the core deliverable |
| NASA Turbulence Modeling Resource, <https://www.nasa.gov/nasa-turbulence-modeling-resource/> | Public turbulence/transition verification and validation cases | Web resource and datasets are cited, not mirrored as a PDF |
| NASA Common Research Model, <https://commonresearchmodel.larc.nasa.gov/> | Geometry and experimental-data source for CRM validation | Web geometry/data portal, not a single PDF |
| OpenVSP VSPAERO basics and V&V examples, <https://www.nasa.gov/reference/openvsp-vspaero-basics/> and <https://github.com/OpenVSP/OpenVSP/tree/main/examples/scripts/python_scripts> | Solver scope, verification cases, and refinement examples | Online documentation/examples are cited; the repository already contains the local OpenVSP installation |
| SU2 documentation, <https://su2code.github.io/docs_v7/Physical-Definition/> | Open-source RANS, transition, and adjoint capability context | The peer-reviewed paper is archived locally; current documentation is cited online |

## 9. Local PDF source manifest

All paths below are relative to this document. SHA-256 values are uppercase hexadecimal. The rights note is conservative: a downloaded PDF being reachable does not by itself grant redistribution rights.

| Local PDF | Title; author(s) | Year; DOI/report | Source URL | Rights note | SHA-256 |
| --- | --- | --- | --- | --- | --- |
| [arxiv_neuralfoil_2503.16323.pdf](../../bib/aerodynamics/arxiv_neuralfoil_2503.16323.pdf) | NeuralFoil: An Airfoil Aerodynamics Analysis Tool Using Physics-Informed Machine Learning; Peter Sharpe, R. John Hansman | 2025; arXiv:2503.16323 | <https://arxiv.org/abs/2503.16323> | Author-posted arXiv preprint; no public-domain assumption; local research copy | 4FCBD9020DABE129126FA4D4813ACFC1EA16CC3B42C3A08E10FBF3E0528A50FE |
| [dlr_first_high_lift_prediction_workshop.pdf](../../bib/aerodynamics/dlr_first_high_lift_prediction_workshop.pdf) | DLR Contribution to the First High Lift Prediction Workshop; Simone Crippa, Stefan Melber-Wilkending, Ralf Rudnik | 2011; AIAA 2011-938 | <https://elib.dlr.de/68309/1/AIAA-2011-938-963.pdf> | DLR eLib marks full text public/open access; no explicit licence field; retain attribution | 81E36AB76D767B31F5097483DB525D348EF0DFCD7426D76B64681C5022370F8B |
| [dlr_powered_lift_turboprop_2025.pdf](../../bib/aerodynamics/dlr_powered_lift_turboprop_2025.pdf) | Influence of Powered Lift Systems on the Aerodynamics of a Turboprop Aircraft; Dennis Keller | 2025; DOI: 10.57676/cmfw-nm22 | <https://elib.dlr.de/217547/1/InfluenceOfPoweredLiftSystemsOnTheAerodynamicsOfATurbopropAircraft-Keller.pdf> | DLR eLib metadata marks CC BY | 52BFD2B73AD380D0470AD905773D9EAF5540E34E04BF717D2412D5175962AA9D |
| [dlr_tau_transition_high_lift_airfoil.pdf](../../bib/aerodynamics/dlr_tau_transition_high_lift_airfoil.pdf) | Navier-Stokes High-Lift Airfoil Computations with Automatic Transition Prediction using the DLR TAU Code; Andreas Krumbein, Normann Krimmelbein | 2007; DLR eLib record, no DOI | <https://elib.dlr.de/46002/2/ManuscriptSTAB2006AndreasKrumbein_v3.and.final.pdf> | DLR eLib marks full text public/open access; Springer is listed as publisher; no explicit redistribution licence | 52B989410575F6F19D65132F3ACD54713EF27861B8F2EFB4978ABEC0C94CEED8 |
| [dlr_unsteady_high_lift_deployment.pdf](../../bib/aerodynamics/dlr_unsteady_high_lift_deployment.pdf) | Unsteady Simulation of The Flow During the Deployment of High-Lift Systems; Nicolas Renard, Jochen Wild | 2012; ECCOMAS 2012 | <https://elib.dlr.de/80214/1/ECCOMAS_2012_TO52_Renard_Wild.pdf> | DLR eLib metadata marks CC BY-ND | 9DB958429D02C5FEB024ABE6A3569C4A7678475ABC14B22B513646EE9A19A240 |
| [mit_avl_user_primer.pdf](../../bib/aerodynamics/mit_avl_user_primer.pdf) | AVL User Primer; Mark Drela, Harold Youngren; MIT AVL documentation | 2010 documentation version; https://web.mit.edu/drela/Public/web/avl/ | <https://web.mit.edu/drela/Public/web/avl/AVL_User_Primer.pdf> | MIT-hosted AVL documentation; AVL software is GPL, but the document has no separate licence statement in the source page; local research copy | 63A5566C19282991559107AB3FA77E8EB1645B1ED03A4724A8A4D68563B72988 |
| [mit_mses_user_guide_3_05.pdf](../../bib/aerodynamics/mit_mses_user_guide_3_05.pdf) | A User's Guide to MSES 3.05; Mark Drela | 2007; MIT manual | <https://web.mit.edu/drela/Public/web/mses/mses.pdf> | MIT-hosted manual; no separate redistribution licence stated; local research copy | 8C61F00B3B19A6C972C2485A60B0C07127AEEBC6564A845AA8E924E67137CFF3 |
| [nasa_cfd_vision_2030.pdf](../../bib/aerodynamics/nasa_cfd_vision_2030.pdf) | CFD Vision 2030 Study: A Path to Revolutionary Computational Aerosciences; Jeffrey P. Slotnick, Abdollah Khodadoust, Juan Alonso, David Darmofal, William Gropp, Elizabeth Lurie, Dimitri J. Mavriplis | 2014; NASA/CR-2014-218178 | <https://ntrs.nasa.gov/api/citations/20140003093/downloads/20140003093.pdf> | NTRS says public distribution but may include copyright material; local research copy | B4DA4DDC8C4B1B494A0188C03D3F1F62210937FEE3F165537F5E25EE446D6966 |
| [nasa_cr_137928_transonic_drag_buildup.pdf](../../bib/aerodynamics/nasa_cr_137928_transonic_drag_buildup.pdf) | Advanced airfoil design empirically based transonic aircraft drag buildup technique; W. D. Morrison Jr. | 1976; NASA-CR-137928, LR-27524 | <https://ntrs.nasa.gov/api/citations/19770013073/downloads/19770013073.pdf> | NTRS determination: government public use permitted | 1C29F0E2AA7550A15DFCF1226E92BADBEA687FF1E7B5FF6129CC6A2F3C5E23F7 |
| [nasa_cr_152138_propeller_slipstream_supercritical_wing.pdf](../../bib/aerodynamics/nasa_cr_152138_propeller_slipstream_supercritical_wing.pdf) | Simulated propeller slipstream effects on a supercritical wing; H. R. Welge, J. P. Crowder | 1978; NASA-CR-152138 | <https://ntrs.nasa.gov/api/citations/19790016853/downloads/19790016853.pdf> | NTRS determination: government public use permitted | BB4B78AC18F81AF1E2BB63ABD0199D3200F9C51DAEDD34B26CCC0B15BB93A684 |
| [nasa_cr_1632_wing_slipstream_interaction.pdf](../../bib/aerodynamics/nasa_cr_1632_wing_slipstream_interaction.pdf) | Analysis of wing slipstream flow interaction; A. Jameson, Grumman Aerospace Corp. | 1970; NASA-CR-1632 | <https://ntrs.nasa.gov/api/citations/19700027535/downloads/19700027535.pdf> | NTRS distribution is public but copyright determination is Other; local research copy, do not assume redistribution rights | 012F75BA31E7627699BD28426353B9B76B0DFC72AE4FDD6C0A0D7324739E2D14 |
| [nasa_cr_2865_generalized_vlm.pdf](../../bib/aerodynamics/nasa_cr_2865_generalized_vlm.pdf) | A generalized vortex lattice method for subsonic and supersonic flow applications; L. R. Miranda, R. D. Elliot, W. M. Baker | 1977; NASA-CR-2865, LR-28112 | <https://ntrs.nasa.gov/api/citations/19780008059/downloads/19780008059.pdf> | NTRS determination: government public use permitted | EF0B928EE7809ED951854A20601309F06DB02FCAA55BECB861CBEE731D271A7B |
| [nasa_cr_4023_vsaero_theory.pdf](../../bib/aerodynamics/nasa_cr_4023_vsaero_theory.pdf) | Program VSAERO theory document: A computer program for calculating nonlinear aerodynamic characteristics of arbitrary configurations; Brian Maskew | 1987; NASA-CR-4023, AMI-8416 | <https://ntrs.nasa.gov/api/citations/19900004884/downloads/19900004884.pdf> | NTRS determination: government public use permitted | 71BE94E298A8AA4B6731CFB9577667A9FC24DB765254F6BDC53DA4D3722EBD23 |
| [nasa_crm_development_2008.pdf](../../bib/aerodynamics/nasa_crm_development_2008.pdf) | Development of a Common Research Model for Applied CFD Validation Studies; John C. Vassberg, Mark A. DeHaan, S. Melissa Rivers, Richard A. Wahls | 2008; AIAA 2008-6919 | <https://ntrs.nasa.gov/api/citations/20080034653/downloads/20080034653.pdf> | NTRS determination: public use permitted | E07F30826E123F24E36881E53856D953B21C98C709DF037CDC4188FFE243E90D |
| [nasa_crm_dpw_evaluation_2019.pdf](../../bib/aerodynamics/nasa_crm_dpw_evaluation_2019.pdf) | An Evaluation and Recommendations for Further CFD Research Based on the NASA Common Research Model (CRM) Analysis from the AIAA Drag Prediction Workshop (DPW) Series; Edward N. Tinoco | 2019; NASA/CR-2019-220284, NF1676L-28402 | <https://ntrs.nasa.gov/api/citations/20190027400/downloads/20190027400.pdf> | NTRS determination: public use permitted | 9FA59A8327E53EC63FBA0EAA07AC9C59A1F7208B76845DCE11B34A2C1D79E96E |
| [nasa_crm_history_2019.pdf](../../bib/aerodynamics/nasa_crm_history_2019.pdf) | NASA Common Research Model: A History and Future Plans; Melissa B. Rivers | 2019; AIAA 2019-2188, DOI: 10.2514/6.2019-2188 | <https://ntrs.nasa.gov/api/citations/20200002395/downloads/20200002395.pdf> | NTRS determination: government public use permitted | F3621128C354805996B04A74020365FC7470E1FF991FC0C0A0AC3D78CC52192B |
| [nasa_high_lift_crm_validation.pdf](../../bib/aerodynamics/nasa_high_lift_crm_validation.pdf) | Requirements and Challenges for CFD Validation within the High-Lift Common Research Model Ecosystem; Adam M. Clark, Jeffrey P. Slotnick, Nigel Taylor, Christopher L. Rumsey | 2020; NASA/NF1676L-35027 | <https://ntrs.nasa.gov/api/citations/20200011458/downloads/20200011458.pdf> | NTRS says public distribution and may include copyright material; local research copy | 725FC4F547847AC327D93F68034A9F18F18DE95616567FE7291972F9BF7EFE22 |
| [nasa_high_lift_prediction_workshops.pdf](../../bib/aerodynamics/nasa_high_lift_prediction_workshops.pdf) | High-Lift Prediction Workshops: Retrospective, Lessons Learned, and Future Prospects; Christopher L. Rumsey | 2024; ICAS 2024 paper | <https://ntrs.nasa.gov/api/citations/20240006238/downloads/ICAS-rumsey-hilift_4.pdf> | NTRS says public distribution but may include copyright material; workshop data is identified as unrestricted; local research copy | 01186704D87C1970E64EAB4563CAA7E72B637429752DC55BB00369A8ABFDAAC8 |
| [nasa_memo_1_16_59_propeller_wing_flap.pdf](../../bib/aerodynamics/nasa_memo_1_16_59_propeller_wing_flap.pdf) | Semiempirical Procedure for Estimating Lift and Drag Characteristics of Propeller-Wing-Flap Configurations for Vertical-and Short-Take-Off-and-Landing Airplanes; Richard E. Kuhn | 1959; NASA-MEMO-1-16-59L, L-144 | <https://ntrs.nasa.gov/api/citations/19980232082/downloads/19980232082.pdf> | NTRS determination: US Government work, public use permitted | 68C76570FE87903D58A438CF97572552DDB149DF16E971B0E628BE2742FFA0EF |
| [nasa_minimum_trim_drag_vlm.pdf](../../bib/aerodynamics/nasa_minimum_trim_drag_vlm.pdf) | Minimum trim drag design for interfering lifting surfaces using vortex-lattice methodology; John E. Lamar | 1976; NASA record 19760021081 | <https://ntrs.nasa.gov/api/citations/19760021081/downloads/19760021081.pdf> | NTRS determination: government public use permitted | 7A9F38965E0CE3504E3E098B308619E7B0F4748B40A9F1D81B009B0B69879F9C |
| [nasa_model_form_uncertainty_kb.pdf](../../bib/aerodynamics/nasa_model_form_uncertainty_kb.pdf) | Development of a Prototype Model-Form Uncertainty Knowledge Base; Lawrence L. Green | 2016; AIAA 2016-1196 | <https://ntrs.nasa.gov/api/citations/20160007678/downloads/20160007678.pdf> | NTRS determination: government public use permitted | AA5B21CE6B5B0BF14B1890B48FEA026F368A105765F0BBE9F72DB565D179F0A8 |
| [nasa_mvl15_modified_vlm.pdf](../../bib/aerodynamics/nasa_mvl15_modified_vlm.pdf) | Description, Usage, and Validation of the MVL-15 Modified Vortex Lattice Analysis Capability; Thomas A. Ozoroski | 2015; NASA/CR-2015-218969, NF1676L-22616 | <https://ntrs.nasa.gov/api/citations/20160000765/downloads/20160000765.pdf> | NTRS determination: public use permitted | B110C22DA8E63E45AF2381C710699DCBC5AF1E8F3474478E7EEBBE7BAC3432EB |
| [nasa_sp_367_intro_aerodynamics_flight.pdf](../../bib/aerodynamics/nasa_sp_367_intro_aerodynamics_flight.pdf) | Introduction to the aerodynamics of flight; Theodore A. Talay | 1975; NASA-SP-367 | <https://ntrs.nasa.gov/api/citations/19760003955/downloads/19760003955.pdf> | NTRS determination: government public use permitted | 9A4E378AA1378CA4D3B99CB8C3F13A5CC5AE7C91504C2F612F42EDB50C1BBA9A |
| [nasa_sp_405_vortex_lattice_utilization.pdf](../../bib/aerodynamics/nasa_sp_405_vortex_lattice_utilization.pdf) | Vortex-Lattice Utilization; NASA proceedings and contributing authors, single-author field not supplied by NTRS | 1976; NASA-SP-405, L-10948 | <https://ntrs.nasa.gov/api/citations/19760021075/downloads/19760021075.pdf> | NTRS determination: government public use permitted | F90D56574166C9244A9BFB987152D18178D12516A4D803D3E498BCCB2A151E40 |
| [nasa_tm_4541_propeller_high_lift_wing.pdf](../../bib/aerodynamics/nasa_tm_4541_propeller_high_lift_wing.pdf) | Aerodynamic characteristics of a propeller-powered high-lift semispan wing; Garl L. Gentry Jr., M. A. Takallu, Zachary T. Applin | 1994; NASA-TM-4541 | <https://ntrs.nasa.gov/api/citations/19940025432/downloads/19940025432.pdf> | NTRS determination: government public use permitted | 8D1D10186159252B16D30C8B6A4037F028965CFF6C6F3F1C5D517621FA833687 |
| [nasa_turbulence_modeling_vv.pdf](../../bib/aerodynamics/nasa_turbulence_modeling_vv.pdf) | Turbulence Modeling Verification and Validation; Christopher L. Rumsey | 2014; AIAA 2014-0201 | <https://ntrs.nasa.gov/api/citations/20140003976/downloads/20140003976.pdf> | NTRS determination: government public use permitted | D8EBB1190C66790FEA35572FD3252B658CE3ABA7FBDAC09D7A1B82885A130954 |
| [nasa_x57_aerodynamic_database.pdf](../../bib/aerodynamics/nasa_x57_aerodynamic_database.pdf) | Development of the X-57 Aerodynamic Database; Michael A. Frederick, Mark S. Smith, Seung Y. Yoo, Ryan Wallace, Jared C. Duensing, Jeffrey A. Housman, Karen A. Deere, Jeffrey K. Viken | 2025; NASA/TM-20250001715 | <https://ntrs.nasa.gov/api/citations/20250001715/downloads/20250001715%20FINAL.pdf> | NTRS determination: government public use permitted | ACEDA5A65F4AF946F8ACB38EE39EECC9E40ACDED15F7EB15874BB66223BF78F0 |
| [su2_aiaa_j053813.pdf](../../bib/aerodynamics/su2_aiaa_j053813.pdf) | SU2: An Open-Source Suite for Multiphysics Simulation and Design; Thomas D. Economon, Francisco Palacios, Sean R. Copeland, Trent W. Lukaczyk, Juan J. Alonso | 2016 journal issue; DOI: 10.2514/1.J053813 | <https://web.mit.edu/su2_v6.0/1.Ej053813.pdf> | Author/MIT-hosted AIAA paper; AIAA copyright notice and permission statement; local research copy | 5FC55CB0C0FEB3C60F444443287957C5738A0D399B3056EB1C85ED92C44B13F7 |

## 10. Recommended implementation order

This research does not require source changes, but the implementation order that follows from it is:

1. Define a shared aero case/result/provenance contract above the existing solver modules.
2. Add validity and diagnostic propagation to the existing AeroAnalysis, drag-buildup, VLM, section, and external-adapter outputs.
3. Make clean, high-lift, powered, ground-effect, and engine-out states explicit configuration dimensions.
4. Make all requirement residuals consume the declared aero status and validity, with not_evaluated distinct from zero or pass.
5. Build deterministic cache keys and store raw/derived results separately.
6. Establish section and 3D benchmark fixtures before calibrating empirical factors.
7. Add finalist MSES/external-3D/CFD evidence and compare only matching frames, references, and scopes.
8. Train or fit reduced aero databases only from retained, valid cases and expose their uncertainty.

This preserves the requirements-first flow in docs/REQUIREMENTS_FIRST_AIRCRAFT_DESIGN.md: materialize a coherent candidate, screen it cheaply, evaluate aero/trim/stability at an explicit fidelity, propagate residuals, and run finalists at higher fidelity. It also preserves the existing ALAS solver boundaries and prevents a finite low-order coefficient from becoming false precision.
