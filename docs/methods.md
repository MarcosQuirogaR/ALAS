# Aerodynamic exposed-area correction

The hybrid parasite buildup uses gross projected wing area for coefficient
normalization and exposed wing area for skin friction. With
`drag_model.exclude_buried_main_wing_area = true` (the product default), the
main-wing center section inside the body is subtracted before applying the
configured wing wetted-area factor. Setting it to `false` retains the gross
wing-area convention; `AeroAnalysis::new_reference_compatibility` also retains
the frozen convention independently of this flag.

The basis is the exposed-area relation in [NASA NDARC Theory, v1.6, wing drag
model](https://rotorcraft.arc.nasa.gov/Publications/files/NDARCTheory_v1_6_938.pdf),
`S_wet = 2(S - c*w_fus)`. ALAS integrates the piecewise-linear chord over the
buried span, interpolates the local fuselage cross-section at the root quarter
chord, and evaluates its superellipse width at wing-root height. Thus a detached
high wing is not shielded merely because it projects onto the body. The
aircraft reference area is unchanged.

This is a local extruded-body approximation, not a mesh intersection: it does
not resolve fairings, thickness, changing body contour over the chord, twist
in the buried-area metric, or dihedral crossing the body surface. It is limited
to symmetric main wings and centered fuselages. Tail wetted-area conventions
are unchanged. OpenVSP's [Comp Geom](https://www.nasa.gov/reference/openvsp-comp-geom/)
offers an intersected-surface calculation for higher-fidelity checks.

Reproduce the numerical change with
`cargo run -p alas-aero --example parasite_area_correction`. Analytic tests cover
a rectangular center section, a tapered center section, vertical and horizontal
detachment, and an oversized body; the analysis test verifies flag selection,
legacy replay, and unchanged induced and wave terms. These establish numerical
verification, not aircraft drag calibration or physical validation.

For the current preset geometry, the isolated flag comparison at each preset's
cruise Mach and altitude gives the following dimensionless parasite coefficients
(these are not fitted total-polar intercepts):

| Preset | Gross convention | Exposed convention | Change |
| --- | ---: | ---: | ---: |
| AVE | 0.01764529 | 0.01666136 | -5.576% |
| A340-300 | 0.01851220 | 0.01750726 | -5.429% |
| A380-800 | 0.01468718 | 0.01368473 | -6.825% |
| B787-9 | 0.01899801 | 0.01780423 | -6.284% |
| A320-200 | 0.02196036 | 0.02052914 | -6.517% |
| A220-300 | 0.02281666 | 0.02157961 | -5.422% |
| ATR72-600 | 0.02460001 | 0.02460001 | 0.000% |
| DC-10 | 0.01844081 | 0.01715870 | -6.953% |

The ATR main-wing root is above the modeled fuselage; its projected overlap is
therefore not subtracted. These modest changes do not remove the reported
25-63% differences against published drag estimates. Further agreement requires
equivalent observables and flight conditions, geometry checks, and independent
calibration evidence; no multiplier is fitted to those reported differences.

## Interpreting drag and lift comparisons

Sun, Hoekstra and Ellerbroek's [2020 drag-polar
paper](https://pure.tudelft.nl/ws/portalfiles/portal/71038050/published_OpenAP_drag_polar.pdf)
estimates polar coefficients from flight surveillance with a stochastic total
energy model. Its coefficients are model-based estimates, not directly
measured component drag. A fitted total-polar intercept is not necessarily the
parasite buildup alone, and effective Oswald efficiency from a total polar is
not the same observable as pointwise inviscid span efficiency from a solver.

`PolarSweep` exposes both the geometric solve angle and the Prandtl-Glauert
relabelled reporting angle. A slope on the latter axis is an analytical
compressibility approximation, not a compressible VLM solution. Low-speed
wing-alone estimates must not be scored against a whole-aircraft cruise slope.

There is no universal swept-wing efficiency ceiling below 0.968. The planar
elliptic-loading bound is conditional; nonplanar lifting systems can exceed
unity for a fixed projected reference span. See [NASA's nonplanar lifting-line
discussion](https://ntrs.nasa.gov/api/citations/19920016018/downloads/19920016018.pdf).

# Mass statement, tanks, fuel policy and dispatch closure

The item ledger (`alas-mass::ledger`) is the representation every mass
property is computed from. Each item carries a mass, a role (fixed,
operating item, unusable fuel, payload, usable fuel), a reference point in
the geometry frame (x aft, y starboard, z up) and a centroidal inertia
tensor; the aircraft tensor about any state's centre of gravity follows from
the parallel-axis theorem, with products of inertia stored as the positive
integrals and negated on the matrix off-diagonal (JSBSim structural frame
convention). Component tensors are closed-form solids: thin plates for
lifting surfaces, a thin-walled cylinder for the fuselage, solid cylinders
for engines, prisms for tanks and payload items. Component stations follow
Raymer (*Aircraft Design*, 6th ed., ch. 15-16): the integrated wingbox
centroid, tails at 42 percent of their mean chord, fuselage at 45-50 percent
of its length by engine placement, gear at the configured nose and main
stations, engines at nacelle mid-length. The radius-of-gyration cross-check
uses Raymer's definition against half-span, half-length and half their mean
(`I_xx = m (R_x b/2)^2`), reproducing the measured B747-100 tensor of NASA
CR-2144 to one percent; the frozen `alas-stab` estimate keeps its own
full-length convention for parity.

Tanks (`alas-mass::tanks`) are spar-box volumes integrated on the built
wing between the configured front and rear spars over declared semispan
intervals, a carry-through centre tank between the body sides, a stabiliser
trim tank and a declared-volume auxiliary tank. Manufacturer-published
per-tank volumes are authoritative where a preset declares them; the
geometric estimate supplies the centroid and a calibration factor that keeps
a redesigned wing's capacity consistent: on a design other than the preset's
own, each published cell becomes a per-cell factor between its published
volume and the geometric estimate on the preset's geometry, applied to the
candidate's own spar box (`FuelTankLayout::resolve_scaled`), so the optimizer
sees fuel volume grow and shrink with the wing. Fuel is loaded in the reverse of the
burn order and burned centre first, outer wing last. A trim tank is a CG
control tank: it is filled after every other tank and emptied first,
standing for the forward transfer that empties it before landing on the
A330, A340 and A380 (`alas-mass::tanks::order`; secondary source, the
in-flight transfer path is not modelled); unusable fuel (CS
25.959) is part of the empty mass and expansion space (CS 25.969, at least
two percent) is excluded from the usable volume.

The fuel policy (`alas-config::fuel_policy`, `alas-mass::fuel_policy`)
decomposes the load into taxi, trip, contingency, destination alternate,
final reserve, additional and extra fuel. Under the EASA basic scheme
(CAT.OP.MPA.181(c) and AMC1) contingency is the larger of five percent of the
trip fuel and five minutes holding at 1,500 ft above the destination at the
estimated landing mass, the final reserve is thirty minutes holding at 1,500
ft at the estimated mass on arrival at the alternate, and a missing
destination alternate is replaced by a fifteen-minute hold. Under 14 CFR
121.639 the reserve is forty-five minutes at normal cruise consumption after
the alternate with no contingency; under 121.645(b) it is ten percent of the
flight time to the destination priced at cruise consumption plus a thirty-
minute hold, and two hours of cruise replace a missing alternate
(121.645(c)). Holding fuel flow is the cruise TSFC applied at the minimum-
drag lift coefficient of the parabolic polar, which for a high-bypass
narrowbody lands within about ten percent of the cruise flow at the same
mass; this is an analytic estimate, not an engine-deck value.

The dispatch closure (`alas-mass::dispatch`, `alas-pipeline::mission_stage::
dispatch`) solves the fixed point `takeoff mass = zero-fuel mass + required
takeoff fuel(takeoff mass)`, first with the analytic Breguet model built from
the report's drag polar and engine binding, then by re-flying the native
segment mission at the estimate and re-pricing the policy on the flown trip
until the mass changes by less than the larger of five kilograms and one
part in ten thousand of the takeoff mass (the fixed point contracts by about
a quarter per flight, so the residual error is under a third of the last
change). The mass is bounded by
the takeoff-mass limit and by the usable tanks less the taxi fuel; the
shortfall beyond either bound is a reported finding, and the mission is
flown at the admissible mass.

The search minimises a mission quantity through `optimizer.objective` and ranks
candidates feasibility first: a candidate that violates a hard requirement family
costs more than any feasible one and infeasible candidates order by their
normalised violation; soft families rank behind feasibility and ahead of the
objective; diagnostic families are reported only. Tail-volume windows are
plausibility bands (tier S of the research note) and rank soft even under a
hard geometry family, and a violation below ten parts per million of its
limit is numerical noise (the builder's own rounding of the reference area)
and is not counted. The objective is a
mission quantity closed by an inner takeoff-mass fixed point, which the
2026-09-05 MDO research note argues is the right architecture for a
derivative-free search (an equality closure constraint leaves a
population-based method a measure-zero feasible set).

# FLOPS transport mass method

The NASA Flight Optimization System weight equations (Wells, Horvath and
McCullers, *The Flight Optimization System Weights Estimation Method*,
NASA/TM-2017-219627 Vol. I, 2017) are implemented as three separately
selectable groups in `alas-mass::flops_transport`, each replacing the
corresponding frozen group of the Torenbeek/fraction buildup when selected
in `mass_model`:

- `systems_mass_method = flops_transport_v1`: surface controls, APU,
  instruments, hydraulics, electrical, avionics, furnishings, air
  conditioning and anti-icing (equations 97-115) and the operating items
  (crew and baggage, unusable fuel, engine oil, passenger service, cargo
  containers; equations 119-126).
- `structural_mass_method = flops_transport_v1`: the wing (equations 10-17
  and 33-45, with the simplified bending factor of equation 10 or the
  detailed load-path integration of equations 18-32 and the engine
  inertia-relief factor of equations 27-30 and 39-41), horizontal tail (46),
  vertical tail (50), fuselage (56-57), main and nose landing gear (63-67),
  paint (68) and nacelles (69, 74).
- `propulsion_mass_method = flops_transport_v1`: the scaled engine
  (75-76, 80), the distributed-propulsion scaling of counts, thrust and
  nacelle diameter beyond four engines (81-85), thrust reversers (86),
  engine controls and starters (87, 89, 91) and the fuel system (92).

Every equation is evaluated in its published US customary units with the
exact conversions in `alas-units` and returned in kilograms. Geometry
inputs are read from the built airplane (reference area and span projected
on the aircraft plane, tip-to-root taper, quarter-chord sweep, the
area-weighted lofted thickness ratio, tail areas and taper, fuselage length
and maximum width and depth, nacelle diameter and length, engine stations);
architecture inputs that a geometry cannot express (engine mounting,
maximum Mach, maximum fuel capacity, crew and cabin classes, hydraulic
pressure) are the declared `mass_model.flops_transport` fields, and the
technology factors (`FCOMP`, `FAERT`, `FSTRT`, `PCTL`, `CARGF`, `WPAINT`,
`WENGB`, `THRSO`, `EEXP`, `WPMISC`, `WMARG`) are `mass_model.flops_structure`
with the FLOPS defaults. A missing datum makes the selected method
unverified and the buildup fails with the list of blockers; no fraction is
substituted.

Where FLOPS estimates an input the user leaves blank, the same estimate is
used and named in the evaluation record: the design landing mass defaults
to the mass model's landing-mass fraction of the design gross mass rather
than FLOPS' range-based equation 65, and the gear oleo lengths follow
equations 66-67. The fin, canard and hybrid-wing-body branches, alternate
engines and energy storage, and the general-aviation and fighter equations
are not transport equations and are not translated. Nacelles belong to the
structural group in the FLOPS statement but are carried in the propulsion
group of the ten-group breakdown so the mass stations place them on the
engines.

Verification uses the two FLOPS-run validation cases NASA's Aviary
distributes (`LargeSingleAisle1FLOPS`, detailed wing, and
`LargeSingleAisle2FLOPS`, simple wing; inputs and FLOPS outputs recorded in
an internal validation-data note, test
`crates/alas-mass/tests/flops_validation_cases.rs`). Every structural,
propulsion, systems and operating-item output is reproduced within the
data file's quoted precision (one part in a thousand; the simple bending
factor 8.8294 and the detailed factor 11.5918 to four figures, the pod
inertia-relief factor 0.967333 to 1e-4). Two conventions the memorandum
typesets ambiguously were settled by that cross-check: the sweep-bucket
bracket of equation 31 divides the unadjusted factor, and the
distance-weighted sweep of equation 18 is a plain weighted sum that FLOPS
does not renormalise when the stations stop short of the tip. This is
implementation verification against the published equations as FLOPS
evaluates them, not physical validation against weighed aircraft.

## The optimizer

The one method is `optimizer.solver.method = differential_evolution`. A saved
file that names a retired method (`scipy_legacy` included) loads as this one,
with a load note.

Set `optimizer.solver.method = differential_evolution` to select the current
mission-sized profile. Its evaluation chain and optimizer are described
below: mission sizing, explicit requirement policies, a screening stage, a
diverse elite, the refinement kernel under epsilon constraints, and bounded
feasibility restoration.

# Multidisciplinary sizing loop and the L-SHADE epsilon-constrained driver

`alas-opt::mdo::mda` closes each candidate as a converged multidisciplinary
analysis rather than a single pass: mass and centre of gravity at the
current takeoff mass, trim and drag polar at that mass and centre of
gravity, mission fuel at that polar, and the takeoff mass those imply,
repeated until the takeoff mass changes by less than
`optimizer.objective.sizing_tolerance_kg`. The fixed point is accelerated
with Aitken's delta-squared extrapolation every third pass; the
vortex-lattice trim is re-run only when the centre of gravity has moved by
more than `retrim_cg_tolerance_pct_mac` of the mean aerodynamic chord since
the last trim, so a converged candidate is trimmed at its own weight and
its cruise lift coefficient follows the sized mass (a zero tolerance keeps
the single trim at the takeoff-mass ceiling). The number of passes and
re-trims and the residual centre-of-gravity inconsistency are reported with
the sized candidate.

This is the coupling chain every candidate goes through, one discipline
feeding the next (`alas-opt::mdo`, one module per stage): `build` evaluates
geometry (`alas-geom`), a two-pass mass breakdown with structural feedback
(`alas-mass`), and the trimmed aerodynamic operating point (`alas-aero`,
`alas-stab`) that do not depend on the takeoff mass; `sizing` closes the
takeoff-mass fixed point analytically against that trimmed drag polar, flying
the design mission under the configured fuel policy (`alas-mission`);
`residuals` turns the sized candidate into a typed table, one entry per
requirement family (mass and fuel, balance, airworthiness performance,
geometry), instead of folding every requirement into a single weighted
penalty; and `cost` assembles that table into the scalar the search
minimises, ranking feasibility ahead of the objective value. Every candidate
the search ranks, including the reported winner, went through this whole
chain; nothing downstream of `build` is skipped or approximated for a
"cheap" evaluation inside the search. The screening stage evaluates through
a declared `ScreeningFidelity`, which today is the full model; a cheaper
descriptor must first rank candidates like the full model in the
`screening_rank_correlation` experiment, and a screening score only ever
chooses where the refinement starts.

Under the mission-sized `differential_evolution` profile, every candidate is
a converged aircraft from the chain above: **current-to-pbest/1/bin
differential evolution under the epsilon-constrained method**
(`alas-opt::search_methods::lshade_de`), with static `F = 0.5`, `CR = 0.9`
(R. Tanabe and A. S. Fukunaga, "Reviewing and Benchmarking Parameter Control
Methods in Differential Evolution," IEEE Trans. Cybern. 50(3), 2020, DOI
10.1109/TCYB.2019.2892735: static beats adaptation within 800 D evaluations)
and L-SHADE success-history adaptation as a switch.

- **L-SHADE**: R. Tanabe and A. S. Fukunaga, "Improving the Search
  Performance of SHADE Using Linear Population Size Reduction," IEEE
  Congress on Evolutionary Computation (CEC) 2014, DOI
  10.1109/CEC.2014.6900380. Success-history parameter adaptation for the
  mutation factor `F` and crossover rate `CR` (weighted Lehmer means into a
  circular memory; off by default), `current-to-pbest/1` mutation with an
  external archive (J. Zhang and A. C. Sanderson, "JADE: Adaptive
  Differential Evolution With Optional External Archive," IEEE Trans. Evol.
  Comput. 13(5), 2009, DOI 10.1109/TEVC.2009.2014613), and linear population
  size reduction in evaluations from `clamp(B / 10, 24, 6 D)` for refinement
  budget `B` down to eight; with adaptation on, both memories take the
  weighted Lehmer mean and the crossover rate has its terminal value.
- **Epsilon-constrained method**: T. Takahama and S. Sakai, "Constrained
  Optimization by the epsilon Constrained Differential Evolution with
  Gradient-Based Mutation and Feasible Elites," CEC 2006, DOI
  10.1109/CEC.2006.1688283, and "...with an Archive and Gradient-Based
  Mutation," CEC 2010, DOI 10.1109/CEC.2010.5586484. Two closed-infeasible
  candidates within a shrinking `epsilon` are ranked by objective alone,
  otherwise the less-violating one wins. Two engineering deviations from the
  paper: a strictly feasible candidate always ranks ahead of an
  epsilon-feasible one, and `epsilon(0)` is the 0.2 quantile of the
  closed-infeasible initial violations only (not-closed and pre-gated
  candidates carry barrier values, not physical misses). With adaptation on,
  the memory weights are each parent's relative improvement, a tier change
  counting as one, where L-SHADE uses the absolute objective change.
  `epsilon` decays to exactly zero at a fifth of
  the evaluation budget, after which the comparison is exactly Deb's
  feasibility rule (K. Deb, CMAME 186(2-4), 2000). This lets the search
  explore past a locally-blocking hard limit early on without ever reporting
  a candidate that violates one: `Outcome::winner` is tracked as the
  strict feasibility minimum over every candidate the run ever evaluated,
  independent of the epsilon-relaxed dynamics that decide which candidates
  survive inside the live population.
- **Bound handling**: midpoint-to-parent repair - a mutant component that
  leaves its bound is placed halfway between the bound it crossed and the
  parent's own value there, rather than reflected or clamped to the bound.
- **Termination**: with a feasible best whose relative improvement stays
  below 1e-4 for `convergence_stagnation_generations` generations, the run
  stops as `converged` when the normalised spread is below `tolerance` and
  as `stagnated` otherwise; else it stops on `evaluation_budget`,
  `time_budget` (checked at generation boundaries, the first right after
  the initial population) or `cancelled`.
- **Determinism**: one generation's trial vectors are built in fixed index
  order from the seeded stream, then evaluated as a single batch whose
  scores return in index order, so a seeded run that stops on its
  evaluation budgets replays bit-identically at any
  `optimizer.solver.workers`. Time-limited: the stopping point depends on
  machine speed and worker count; replay with the recorded evaluation counts
  (`replay_evaluations` per stage) for a bit-identical result at any worker
  count, or set `stop_on_evaluations_only`.

A screening stage evaluates seeded Latin-hypercube batches of the design box
plus the baseline with the full in-loop model; a diverse elite of it, its
best point and the baseline seed the refinement's initial population
(`alas-opt::search::screening` and `::elite`). It never selects the winner
directly: every screened point the refinement keeps is scored again under
the refinement's rule, as an exact cache hit when the models agree.

# Passenger cabin: exits, door stations and monuments

Implemented in `alas-payload::cabin`. Frame: x in metres aft of the nose tip.

- **Exit ceiling.** A registered aircraft declares its exit pairs by CS 25.807
  type letter only (`alas_config::CertifiedExitLayout`). The seat ceiling is
  the sum of the CS 25.807(g) pair ratings of those letters (Type A 110,
  B 75, C 55, I 45, II 40, III 35, IV 9); a source maximum (`certified_max_seats`)
  and, in a fixed-aircraft basis, the planning seats bound it further. A body
  without a declared arrangement keeps the generic diameter and pair-spacing
  proxy.
- **Door stations.** Where the source prints door stations with the body
  length they are measured on, the main deck runs from one monument bay ahead
  of the first door's cross-aisle to one behind the last, and no seat row may
  overlap a door cross-aisle (the door's CS 25.807(a) opening width). On a body
  of another length, the first door keeps its nose distance, the last its tail
  distance, and the doors between are spaced in proportion; an inconsistent
  mapping falls back to the generic frame. Sources: Boeing 787 ACAP
  D6-58333 Rev Q section 2.7.1 (787-9), Boeing 777X ACAP D6-86073 Rev G
  Table 2-3 (777-9, flown on AVE).
- **Monuments.** On a declared cabin, galleys and lavatories (counts from the
  provisioning ratios, one lavatory per 45 and one galley per 100 passengers
  plus one, unless configured) stand side by side across bays that leave the
  aisles open. The bays at both ends and at each class boundary come with the
  cabin; further bays are charged at the intermediate doors. A bay is
  0.813 m long, the 32 in lavatory of D6-86073 Rev G Figure 2-4, which also
  covers a galley stowing a 0.81 m full-size ATLAS trolley (secondary source).
  No service length is reserved beyond these bays; the calibrated
  `service_reserve_len` remains only on a cabin whose doors are unknown.
- **Main-deck baggage.** A fuselage whose lower deck has less than 0.9 m of
  clear height mid-cabin stows its baggage on the main deck, so its seats keep
  their declared pitch and the floor they leave becomes the forward and aft
  compartments, instead of the pitch being stretched over it.

These are geometric screens, not an evacuation demonstration (CS 25.803) or an
approved LOPA.

# Propulsion-specific field performance

`performance.legacy_field_correlations = false` selects the corrected field
method. Jet takeoff retains Raymer's FAR-25 TOP correlation (5th edition,
section 5.4, Fig. 5.4): 37.7 TOP in ft with wing loading in psf. It already
estimates field length; its BFL proxy receives no additional 1.15 multiplier.
[CS/FAR 25.113](https://www.govinfo.gov/content/pkg/CFR-2025-title14-vol1/pdf/CFR-2025-title14-vol1-sec25-113.pdf)
applies 1.15 to an all-engine takeoff-distance candidate, compared with the
engine-out distance. It does not multiply a balanced field length by 1.15.
The historical low-level APIs and the explicit replay selection preserve the
translated correlation, speed schedule and BFL multiplier for parity.

Propeller distances use Torenbeek, *Synthesis of Subsonic Airplane Design*
(1982), sections 5.4.5-5.4.6, pp. 167-170, equations 5-73/74, 5-89 and
5-93/94, with Appendix K defining the separate takeoff phases. The ground
balance is integrated with installed thrust evaluated at each true airspeed
and the actual ambient density. The active engine model supplies normal AEO
power and the declared reserve rating after one engine fails. Clean field drag
comes from the shared candidate model, with the existing high-lift increment
added; gear drag is omitted. A positive linear lift term is bounded over the
field lift range. Unconfigured failed-engine and asymmetric drag are explicit
zero assumptions, rather than aircraft-specific evidence.

The BFL approximation uses the energy-equivalent acceleration over 0..V2 in
place of the source's mean over 0..V1, plus its 200/sqrt(sigma) inertia
allowance and 0.37 g stopping deceleration. The OEI gradient at V2 substitutes
for the source's equivalent gradient over phases 1..2. The required takeoff
field is the
larger of this approximation and 1.15 times the AEO screen-distance estimate.
Rotation is instantaneous in that AEO estimate. This extension retains
propeller thrust lapse but does not solve critical V1 or independently certify
accelerate-stop performance. Single-engine aircraft and nonpositive OEI climb
gradients are outside this BFL method's domain and report an error.

Propeller actual landing distance is the air-phase energy distance from 50 ft
plus the touchdown kinetic-energy braking distance. The sourced conceptual
inputs are mean excess drag/weight 0.10 and mean deceleration 0.40 g (within
Torenbeek's 0.35-0.45 g turboprop range without reverse). Mean deceleration
already includes inertia; no second braking delay is added. The field schedule
uses Vref = 1.23 VS1g under [25.125](https://www.govinfo.gov/content/pkg/CFR-2025-title14-vol1/pdf/CFR-2025-title14-vol1-sec25-125.pdf),
taking the certified reference stall speed equal to the modeled one-g stall
speed. Published preset speeds recover effective gross-area CLmax inputs,
with mass variants, IAS/CAS assumptions and chart-read uncertainties recorded
in `alas-config::presets::PublishedLandingReference`. Reconstructing those
speeds verifies the inputs; it is not independent speed validation.

The ATR factsheet additionally supplies V2 min = 116 KCAS at MTOW. Its
registered takeoff coefficient is model-equivalent under the retained
V2/VS1g = 1.20 convention, with the 23,000 kg option selected explicitly;
the family-wide speed line does not identify which MTOW option applies.
Neither this inversion nor the existing preliminary V2/VMC schedule establishes
measured takeoff CLmax or certification compliance. The source speed is an
input independent of the published distance, rather than a distance fit.

Actual landing distance and airport dispatch field length are separate.
[CAT.POL.A.230(a)](https://www.easa.europa.eu/en/document-library/easy-access-rules/online-publications/easy-access-rules-air-operations?erules-id=ERULES-1963177438-18821)
uses 60% of LDA for turbojets and 70% for turboprops. The latter is configurable
through `propeller_dry_landing_distance_share`; select 0.60 for the general
[FAR 121.195(b)](https://www.ecfr.gov/current/title-14/chapter-I/subchapter-G/part-121/subpart-I/section-121.195)
turbine-aircraft convention. Corrected feasibility and runway figures compare
the factored dispatch distance with LDA. The legacy feasibility boundary uses
actual distance for reproducibility.

The full CADO comparison is reproducible with `tools/field_fleet_before.ps1`,
`cargo run -p alas-pipeline --profile test --example field_fleet_check`, and
`tools/field_fleet_compare.ps1`. All 288 rows are inspected, with supported
metric cohorts and exclusions retained. CADO field lengths and speeds are
comparison targets, never coefficient-fitting inputs. Aircraft lacking an
installed model use a declared ideal actuator-disk thrust envelope based on
CADO shaft rating and rotor diameter, with 0.85 effective-power sensitivity.
The published preset audit uses the actual installed propulsion model. Neither
comparison establishes certificated performance.
