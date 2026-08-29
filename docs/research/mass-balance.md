# Mass properties and balance for conceptual aircraft design

Research note for ALAS, prepared 2026-08-26 from the verified local corpus in
bib/mass-balance. This note is an engineering synthesis, not a certification
basis. It deliberately keeps conceptual estimates, measured data, and
certification/operational evidence distinct.

## Executive conclusions

1. A candidate mass model should be a component ledger, not a single empty
   weight fraction. The minimum ledger is structure, landing gear, propulsion,
   systems, furnishings, operating items, payload, usable fuel, unusable fuel,
   and retained fluids. Each item needs mass, reference point, uncertainty,
   provenance, and a state/condition tag.

2. Class-I regressions are useful for exploring a design space; they are not a
   closure of the aircraft physics. Move to component or geometry/material
   methods as soon as the arrangement exists, and calibrate both total mass and
   longitudinal/vertical/lateral distribution to a known-aircraft or
   manufacturer-quality reference. NASA's FLOPS work and its comparison with
   Raymer- and Roskam-style workflows support this staged approach.

3. Fuel is a state variable and a volume/geometry problem. Separate maximum
   carried fuel, usable fuel, unusable or trapped fuel, reserve, mission fuel,
   and the fuel remaining at each event. A mass balance that obtains fuel by
   MTOW minus empty mass minus payload must be labeled as a closure remainder;
   it is not evidence that the tanks can contain the remainder or that the
   mission is feasible.

4. CG must be computed from the same item ledger used for mass closure. Inertia
   requires the full tensor, component centroidal tensors, and parallel-axis
   transformations. A point CG without a tensor is insufficient for dynamic
   response, flight-test correlation, or loads/stability work.

5. Uncertainty must be propagated through the mission, performance, stability,
   and structures calculations. Correlated empty-mass, fuel-flow, tank-volume,
   payload, and CG errors matter. Retain a candidate only when the required
   nominal case and the defined uncertainty envelope satisfy hard constraints;
   otherwise report a typed residual instead of hiding it in a rounded margin.

6. The current ALAS design direction already has useful separation of payload,
   mission fuel status, and capacity evidence. The missing closure is an
   authoritative component state ledger connected to geometry, tank topology,
   CG/inertia, loading sequences, and uncertainty-aware feasibility residuals.

## 1. Scope and evidence rules

This note covers conceptual and preliminary design methods for:

- component-level empty-weight and operating-item estimation;
- systems, furnishings, payload, landing gear, engine, and propulsion mass;
- CG, products of inertia, and mass-moment tensors;
- loading cases, fuel burn, transfer, tank volume, and unusable fuel;
- calibration to known aircraft and measurement evidence;
- uncertainty propagation into performance, stability, structures, and
  feasibility screening.

The evidence hierarchy used here is:

1. measured or configuration-specific data with a stated test or accounting
   basis;
2. official certification and operational guidance;
3. NASA, DLR, AGARD, or other primary technical reports;
4. openly licensed peer-reviewed or author-archived research;
5. a transparent conceptual correlation with a stated applicability range.

Raymer, Roskam, and Torenbeek are included as bibliographic and methodological
cross-references only. Their books are copyrighted and no book PDF is included
in this repository. Exact proprietary equations are not reproduced here.

Use consistent units and one aircraft coordinate convention. The
recommendations below assume x positive aft, y positive starboard/right, and z
positive up; if ALAS uses another convention, record the transform and tensor
sign convention in the data contract rather than silently changing signs.

## 2. Required mass states and closure equations

The model should expose named states rather than infer them from one scalar:

| State | Meaning | Required accounting |
|---|---|---|
| DWM | dry/installed component mass before operating items | component ledger and provenance |
| BEW/OEW | basic or operating empty weight, with the selected convention | DWM plus installed systems, furnishings, fluids, and declared operating items |
| ZFW | zero-fuel weight | BEW/OEW plus the selected payload and any payload-specific equipment |
| ramp/taxi mass | mass before taxi fuel burn or dispatch allowance | ZFW plus all carried fuel and other ramp items |
| take-off mass | mass at brake release or the project definition | ramp/taxi mass minus taxi burn and declared disposables |
| landing mass | mass at the landing event | take-off mass minus mission burn and any event-specific jettison |
| reserve state | final required fuel state | reserve fuel plus any trapped/unusable/retained fluid definition |

The basic identities are:

    ZFW = empty-state mass + payload-state mass
    ramp mass = ZFW + total carried fuel + ramp-only items
    take-off mass = ramp mass - taxi/ground burn
    landing mass = take-off mass - mission fuel burned

Do not use the word “fuel” for several different quantities. At minimum store:

- tank geometric capacity;
- maximum dispatchable/carryable fuel;
- usable fuel;
- unusable or trapped fuel;
- reserve fuel;
- fuel loaded for the mission;
- fuel consumed by each segment;
- fuel remaining after each segment and at landing.

For a design state s, the closure residuals should be explicit:

    r_mass(s) = M_ledger(s) - M_state(s)
    r_cg(s)   = CG_ledger(s) - CG_state(s)
    r_vol(t)  = V_fuel(t) - V_tank,available(t)
    r_fuel    = fuel_required + reserve - usable_fuel_loaded

The acceptable unit and tolerance for every residual must be attached to the
requirement or analysis case. A displayed zero after rounding is not a
validated zero.

## 3. Component-level estimation method

### 3.1 Staged fidelity

Use three deliberately different stages:

**Class I / statistical exploration.** Use calibrated regressions or mass
fractions to initialize a broad design space. Keep the source family,
reference fleet, configuration class, and applicability range with each
estimate. NASA's FLOPS documentation is a useful example of a complete
conceptual synthesis with structural, propulsion, systems, operating-item,
payload, and fuel groupings.

**Class II / component ledger.** Once planform, fuselage, cabin, gear,
propulsion installation, and tank arrangement exist, estimate each component
from geometry, material, loads, installation, or a component correlation.
Calculate the centroid and uncertainty at the same time. The ledger is the
authoritative source for CG and inertia; group regressions are only priors or
cross-checks.

**Class III / configuration or test calibration.** Replace high-sensitivity
items with supplier data, detailed structural sizing, weighed equipment,
measured tank data, or flight/ground-test evidence. Preserve the earlier
estimate as a prior and record the calibration delta rather than overwriting
history.

A useful implementation is an ensemble of methods: a physics/geometry
estimate, an applicable statistical estimate, and a calibrated estimate. A
large disagreement is an epistemic uncertainty or applicability residual, not
permission to average the values without explanation.

### 3.2 Minimum ledger

The first ledger should contain at least:

| Group | Examples | Distribution needed |
|---|---|---|
| primary structure | wing, center section, fuselage, empennage, control surfaces, skins, joints | span/chord stations, longitudinal and vertical CG |
| landing gear | struts, wheels, brakes, actuators, doors, steering, attachment structure | gear positions and retracted/extended state |
| propulsion | engine/motor, gearbox, propeller/fan, nacelle, mount, exhaust, reverser | installation stations and rotating mass |
| fuel system | tanks/bladders, cells, pumps, valves, lines, filters, vents, unusable fuel | tank volumes, fluid centroid, transfer topology |
| systems | electrical, avionics, hydraulics, flight controls, environmental control, anti-ice, fire protection, APU | rack/line/equipment locations |
| furnishings/interior | seats, galleys, lavatories, insulation, panels, bins, cabin equipment | cabin station and deck |
| operating items | crew, oil, trapped fluids, service equipment, containers, standard equipment | loadable positions and fixed/variable flag |
| payload | passengers, baggage, cargo, mission equipment | loading stations, allowed distributions, deck and tie-down |
| fluids not in payload | hydraulic fluid, oil, coolant, fire suppressant | tank/line volume and operating state |

Every row should carry:

- stable item identifier and parent group;
- mass value and unit, with lower/nominal/upper or a distribution;
- x, y, z reference point and the frame;
- geometry or source reference;
- state-dependent inclusion rule;
- whether the mass is fixed, loadable, consumable, transferable, or
  jettisonable;
- correlation group for uncertainty;
- evidence quality and calibration status.

The ten-group MassBreakdown currently exposed by alas-mass is a good top-level
view, but it must not be the only representation. In particular, operating
items, unusable fuel, retained fluids, tank hardware, and payload-specific
equipment should not disappear inside a generic systems or fuel fraction.

### 3.3 Structure

For early sizing, use a calibrated structural fraction or component
correlation. As soon as geometry and load paths are available, split the
structure into wing, center section, fuselage, tails, control surfaces, and
attachments. Include secondary structure and local reinforcements explicitly
when payload, landing gear, propulsion, or tank placement creates a load path.

The composite-airplane NASA report is a useful open example of why the
material system and structural concept need to be inputs: fixed fractions,
statistical equations, and point-stress/material-aware approaches have
different sensitivities. Do not apply a metal-airframe fraction to a composite
concept without a documented calibration.

### 3.4 Landing gear

Estimate gear mass from the selected gear architecture and the design landing
loads, not solely as a fixed MTOW fraction. Carry the main and nose/tail gear
as separate items; include wheels, tires, brakes, steering, actuation, doors,
fairings, attachment fittings, and local structure. State whether the mass is
retracted, extended, or both, because configuration and CG/loads cases can
depend on it. Gear location is a balance and structural input as well as a
mass input.

### 3.5 Engine and propulsion installation

Keep bare engine or motor, gearbox, propulsor, mount, nacelle, intake,
exhaust/reverser, controls, cooling, lubrication, and installation hardware
separate. Supplier dry mass is not necessarily installed mass. For electric
propulsion, distinguish motor, inverter, cabling, cooling, battery modules,
containment, and thermal-management hardware; the battery is both a mass item
and a consumable/mission-state driver when state of charge changes.

Use thrust-to-weight or power-to-weight relations only as an initial prior.
Record the installation factor and its applicability. The propulsion mass must
be linked to the same engine selection and operating point used by the
performance model.

### 3.6 Systems, furnishings, payload, and operating items

Systems and furnishings should be broken out by function and driven by
requirements: avionics, electrical generation/storage, flight controls,
hydraulics, environmental control, anti-ice, fire protection, seats, cabin
equipment, and mission equipment. A systems fraction of MTOW is acceptable
only as a Class-I prior and should be replaced or calibrated as architecture
is known.

Payload is not just a scalar. Its station distribution, deck, height, tie-down
state, loading restrictions, and possible movement need to be modeled. Keep
design payload, maximum payload, and mission payload distinct. Crew, baggage,
containers, catering/service items, oil, and other operating items need an
explicit project convention; otherwise an apparently good ZFW can be
inconsistent with the operational mass state.

### 3.7 Raymer, Roskam, and Torenbeek method families

The three book families are useful as method patterns and cross-checks, but
their equations and tables are not copied into this repository:

| Method family | Useful conceptual pattern | ALAS treatment |
|---|---|---|
| Raymer, Aircraft Design: A Conceptual Approach, 6th ed., DOI 10.2514/4.104909 | staged gross-weight/fuel iteration followed by component, subsystem, and mass-property estimates | use the staging and bookkeeping pattern; implement equations from permitted sources or original derivations and calibrate to the ALAS reference fleet |
| Roskam, Airplane Design Part V, 1985 | Class-I/II component and equipment correlations with explicit mass-property and loading considerations | use as a citation-only cross-check for group selection, component estimates, CG, and moments; preserve applicability metadata |
| Torenbeek, Synthesis of Subsonic Airplane Design and Advanced Aircraft Design, DOI 10.1002/9781118568101 | configuration-sensitive group correlations, installation factors, and aircraft synthesis coupling | the current reference path is labeled Torenbeek-style; keep it as a prior, expose its assumptions, and replace/calibrate it as geometry and supplier data mature |
| NASA FLOPS and NASA method comparison | open report evidence for a complete component grouping, calibration, and mission-coupled conceptual workflow | preferred reproducible baseline in the local corpus; compare rather than silently merge estimates |

The legally safe implementation rule is to cite the books, preserve only
short factual method descriptions, and write independent ALAS equations from
the project data model. Do not add scanned or copied book pages to
bib/mass-balance without an explicit license that permits redistribution.

## 4. CG, products of inertia, and mass moments

### 4.1 Common ledger calculation

For component i with mass mᵢ, centroid rᵢ, and centroidal inertia tensor Iᵢ,c,
the aircraft mass and CG are:

    M = Σ mᵢ
    rCG = (Σ mᵢ rᵢ) / M

Translate each component tensor to the aircraft CG using the parallel-axis
theorem:

    Iᵢ,CG = Iᵢ,c + mᵢ[(dᵢ·dᵢ)E - dᵢ dᵢᵀ]
    Iaircraft,CG = Σ Iᵢ,CG

where dᵢ = rᵢ - rCG and E is the identity matrix. Store the complete
symmetric tensor (Ixx, Iyy, Izz, Ixy, Ixz, Iyz), the convention for products of
inertia, and the reference axes. If a downstream module only consumes the
diagonal, that is a documented reduction, not a reason to discard products
earlier.

The result must be recomputed for every state that changes a mass or location:
payload loading, gear retraction, fuel transfer/burn, battery state, stores,
and jettison. The tensor is not constant merely because the airframe geometry
is constant.

### 4.2 Required outputs

At minimum expose:

- total mass and CG in the aircraft frame;
- CG in normalized longitudinal coordinates where required by stability
  rules, with the reference chord and datum documented;
- component and total inertia tensors about CG and, when useful, about a
  fixed datum;
- principal moments and axes when a dynamic model needs them;
- uncertainty or interval for each output;
- state identifier and loading/fuel sequence that produced the result.

NASA's rigid-structure mass-property program demonstrates a geometry/material
route from basic shapes to mass, CG, moments, and products of inertia. NASA's
multibody aircraft study calls for CG envelopes consistent with the general
arrangement, fuel sequence, stability/control, and inertia envelopes. These
are stronger requirements than placing every group at one representative
point.

### 4.3 Physical and numerical checks

Reject or diagnose a state when:

- M is non-positive or a component has an invalid unit/sign;
- the CG lies outside the convex hull of included positive-mass item
  locations without a declared negative-mass bookkeeping convention;
- the inertia tensor is not symmetric within tolerance;
- a principal moment is negative beyond numerical tolerance;
- a fuel or payload item is included in mass but has no valid position;
- a tensor uses a different frame or product-of-inertia sign convention;
- a state changes mass without changing the appropriate first and second
  moments.

Do not silently clamp invalid or negative component masses. A robust solver may
return a diagnostic state for exploration, but the feasibility layer should
classify it as invalid input or an unresolved mass closure.

## 5. Fuel, tank, volume, and loading closure

### 5.1 Tank geometry and capacity

Tank capacity is computed from geometry and fill rules, not from the MTOW
remainder. For each tank or cell, define:

- geometric volume and usable geometric volume;
- liquid density range and temperature basis;
- fill/vent limits, ullage, and expansion allowance;
- pickup, outlet, sump, baffles, and trapped regions;
- pump, valve, crossfeed, and transfer topology;
- attitude, acceleration, and operational restrictions;
- unusable volume/mass and its location;
- fuel hardware mass separately from fuel mass.

The closure should be checked in both mass and volume:

    m_fuel = ρ(T, composition) V_fuel
    0 ≤ V_fuel(t) ≤ V_available(t)
    usable fuel = total fuel - unusable/trapped fuel

For a multi-tank aircraft, a single total volume is not enough. The model must
know which tank can feed which engine or motor, what can transfer, the
priority/sequence, and whether a pump or valve failure changes the usable
amount. A design with enough total volume can still fail a feed, balance, or
reserve requirement.

FAA transport-category flight-test guidance treats unusable fuel as a
drainable plus undrainable quantity and points to tank geometry, pickups,
pitch/roll, sideslip, go-around pitch-up, acceleration, turbulence, pump
failure, and attitude limits. That operational logic is appropriate as a
conceptual checklist even before certification testing.

### 5.2 Fuel burn and transfer

Fuel is a time/state trajectory:

    m(t + Δt) = m(t) - ∫ ṁfuel dt + transfers_in - transfers_out

Each segment should record fuel flow, burn, transfer events, tank levels, tank
CG, aircraft CG, and reserve status. A simple mission solver may integrate
positive fuel flow into total mass, but the mass/balance layer must also
evaluate tank-local state and transfer order.

For each segment and event, retain:

- initial and final total mass;
- initial and final usable fuel by tank;
- fuel burned and the source tank(s);
- transfer quantities and their timing;
- fuel remaining at landing and at reserve checkpoints;
- CG and tensor before/after burn/transfer;
- whether the segment converged and whether fuel was exhausted.

Fuel CG movement can set the limiting forward or aft case. The NASA AVION
preliminary design report explicitly discusses fuel CG travel and options such
as forward fuel, fuselage fuel, or fuel-management rules. Use the fuel
sequence, not only full and empty endpoints, to construct the CG envelope.

### 5.3 Loading sequences

Evaluate at least:

1. empty aircraft;
2. maximum forward payload;
3. maximum aft payload;
4. nominal dispatch payload;
5. full fuel with each allowed payload;
6. minimum fuel/reserve with each allowed payload;
7. sequential passenger/cargo loading and unloading;
8. fuel loading and transfer order;
9. gear/propulsion configuration changes;
10. mission segment states, including worst fuel/CG transitions.

The FAA weight-and-balance guidance expects loading schedules/envelopes and
accounts for passenger distribution, fuel density/movement/use, fluids,
passenger/crew movement, gear/flap movement, and baggage/freight. For ALAS,
these should be generated cases with traceable item assignments, not a
hand-written forward/aft margin.

### 5.4 Feasible payload/fuel region

For each design state, solve the coupled region:

    payload + fuel + empty-state mass ≤ structural/operational limit
    usable fuel loaded ≤ tank/feed/volume limit
    CG(state) ∈ allowable envelope
    reserve and mission fuel constraints are met

The useful output is a boundary or set of feasible cases, not a maximum
payload and maximum fuel number that cannot coexist. Keep “design payload”
and “maximum payload at reduced fuel” as separate results.

## 6. Calibration and measurement

### 6.1 Calibration to known aircraft

Select a reference aircraft or test article by configuration similarity:
materials, propulsion architecture, gear arrangement, cabin/mission system,
and technology level. Calibrate at several levels:

- total empty/operating mass;
- group masses;
- longitudinal, vertical, and lateral CG;
- fuel-system hardware and unusable fuel;
- inertias if measured or credibly reported.

Use a calibration table with reference value, ALAS estimate, delta, reason,
and applicability range. Do not hide a structural shortfall by changing a
systems fraction; preserve the causal delta. If only total mass is known,
calibrate total mass but keep distribution uncertainty wide.

NASA's FLOPS method was calibrated against transport and fighter/attack
aircraft sets, while the NASA method-comparison report shows why multiple
conceptual methods should be compared. The composite-airplane report also
warns that small or old reference databases require updating before being
treated as universal.

### 6.2 Measurement pathway

When a physical article exists, use a measurement campaign to replace the
largest uncertainties:

- calibrated multi-point weighing for total mass and CG;
- controlled fuel or ballast states;
- torsion-pendulum or equivalent test for inertia;
- equipment inventory and installation survey;
- tank calibration and drain/unusable-fuel tests.

The open 2022 Sensors paper reports an integrated large-aircraft setup using
four-point weighing and a torsion pendulum, with very small demonstrated
measurement errors under its stated setup. Treat those values as evidence of a
possible measurement capability, not as universal ALAS tolerances.

The FAA guidance gives an operational change-control pattern: establish
empty mass/CG by weighing, maintain a continuous change record, and reweigh
when accumulated changes exceed its stated thresholds. Those thresholds are
certification/operational guidance for the applicable aircraft context, not
automatic conceptual-design limits.

## 7. Uncertainty propagation

### 7.1 Sources

Separate:

- aleatory variation: passenger/payload distributions, fuel density,
  operational loading, turbulence/attitude, and mission variability;
- epistemic uncertainty: sparse reference data, early geometry, unknown
  installation factors, simplified tank/transfer models, and model-form error;
- numerical uncertainty: solver tolerances, discretization, interpolation,
  and sampling error.

The DLR uncertainty framework recommends carrying uncertainty through the
coupled aircraft analysis, recognizing correlations, and checking convergence
of the sample size. Treating each group as independent can substantially
understate total mass, CG, or mission-fuel risk.

### 7.2 Practical model

Define a random or interval input vector:

    q = [mstructure, mgear, mpropulsion, msystems, mfurnishings,
         mpayload, ρfuel, Vtank, fuel_flow, component_positions, ...]

with correlation groups for quantities sharing a source or design assumption.
For each draw:

1. build the component ledger;
2. close mass, geometry, and tank volume;
3. compute CG and inertia for every loading/fuel state;
4. run performance and mission segments;
5. run stability/control and structural load checks;
6. classify all residuals and retain the full trace.

Use Monte Carlo, Latin hypercube, or a validated surrogate for a broad
exploration. For local sensitivity, finite differences or derivative-based
propagation can rank drivers, but do not use a local linear approximation near
hard boundaries, changing load paths, tank-feed switches, or infeasible
states. Run a sample-size convergence check on means, quantiles, and failure
probabilities.

Report at least nominal, lower/upper or 5/50/95 percentiles, worst sampled
case, and sensitivity ranking. If epistemic alternatives are material, report
separate model families or a conservative outer envelope instead of blending
them into a misleading narrow distribution.

### 7.3 Propagation into downstream physics

**Performance.** Mass changes wing loading, lift/drag required, climb and
acceleration, stall speed, takeoff/landing distances, energy/fuel burn, and
range/endurance. Propagate the entire mass trajectory, not just MTOW. A
fuel-flow model calibrated at one mass can be biased when the mass/CG state
changes.

**Stability and control.** CG changes alter static margin, trim, elevator/
control authority, rotation/flare, and dynamic derivatives. Inertia and
products of inertia alter short-period, phugoid, lateral-directional, and
coupled responses. Evaluate forward/aft/vertical/lateral CG cases and
fuel-transfer transitions.

**Structures.** Mass and CG change inertia relief, landing/ground loads,
wing-root bending, tail loads, engine/gear attachment loads, payload
tie-down loads, and tank slosh/pressure loads. Use the uncertainty samples
that drive each structural case; the heaviest case is not always the maximum
root bending or tail load.

**Mission and operations.** Empty mass, payload, density, tank volume, and
transfer rules jointly determine whether reserve is reachable. The DLR work
shows that mission fuel can correlate strongly with operating-empty and
take-off mass, and that improving a mean value can worsen an upper-tail
outcome. Rank candidates on the required quantile or robust margin when that
is the project policy.

**Optimization.** Make mass/CG/tank residuals constraints, not soft penalties
that an optimizer can trade away. A candidate can be Pareto-attractive while
physically infeasible if the feasibility evaluator is fed a closure remainder
instead of a tank- and state-valid fuel value.

## 8. Residual taxonomy and candidate decision

### 8.1 Typed residuals

Use stable residual codes with value, unit, tolerance, case/state, evidence,
and severity. Recommended classes:

| Code family | Meaning | Typical disposition |
|---|---|---|
| MASS-CLOSURE | component sum does not equal declared state | hard reject until explained |
| MASS-NEGATIVE/UNIT | invalid mass, sign, or unit conversion | invalid input/hard reject |
| CG-OUT | CG outside declared envelope | hard reject for that case |
| INERTIA-INVALID | non-physical tensor, missing frame, or missing required tensor | hard reject when downstream needs it |
| TANK-VOLUME | fuel exceeds available geometry/ullage/feedable volume | hard reject |
| FUEL-RESERVE | mission plus reserve not met | hard reject unless the requirement is explicitly relaxed |
| FUEL-FEED/TRANSFER | engine cannot be supplied for a required state or failure case | hard reject for that operational case |
| PAYLOAD-LOAD | requested payload cannot be placed or violates deck/tie-down rules | hard reject for that case |
| STRUCT-MASS/LOAD | mass-driven structural requirement fails | hard reject |
| STAB-CONTROL | stability, trim, or control margin fails | hard reject |
| PERFORMANCE | takeoff, climb, stall, landing, range, or endurance fails | hard reject when required |
| UNCERTAINTY-TAIL | required percentile/robust margin fails | hard reject under robust policy |
| EVIDENCE-MISSING | value is a placeholder or unsupported by the selected stage | retain only as provisional |
| MODEL-DISAGREEMENT | applicable methods disagree beyond tolerance | investigate/calibrate |
| SOLVER-NONCONVERGED | state/mission did not converge | not a passing result |
| NUMERICAL | sampling or numerical convergence not demonstrated | provisional until resolved |

Keep raw residuals and normalized severity:

    severity = max(0, signed_violation / allowed_scale)

For two-sided bounds, evaluate the nearer violated side and report which bound
was active. Do not normalize away a missing or unsupported requirement.

### 8.2 Retain/reject policy

A candidate is **valid and retainable** only if:

- all required mass, CG, inertia, tank, payload, mission, performance,
  stability/control, and structure states are evaluated;
- every hard residual is within tolerance for the nominal required cases;
- the stated uncertainty policy is satisfied (for example, a required
  percentile or conservative bound);
- all solver states converge or have an explicitly approved fallback;
- evidence quality is sufficient for the current design gate;
- no capacity or fuel value is being supplied solely by an unverified MTOW
  remainder.

A candidate is **provisional** when it passes the available nominal checks but
has missing evidence, unresolved model disagreement, or unverified uncertainty
tails. Keep it in an exploration set with visible provisional residuals; do
not rank it as equivalent to a fully evidenced candidate.

A candidate is **rejected for the case** when any hard constraint fails, when a
required state cannot be evaluated, or when closure is nonphysical. It may be
reopened after a design change or better evidence, but the failed case and
reason remain in the record.

A useful ranking order is:

    valid stage → no hard violations → normalized residual severity
    → soft penalties → objective

This agrees with the requirements-first policy in the ALAS repository and
prevents a lower predicted empty mass from compensating for an impossible tank
or CG state.

## 9. ALAS mapping and implementation recommendations

The mapping below is based on the current repository shape and the
requirements-first document already present in the worktree. It is a
recommendation for the existing design; this research pass does not modify
those source files.

### alas-mass

Current useful behavior: MassBreakdown exposes wing, horizontal tail,
vertical tail, fuselage, gear, propulsion, systems, furnishings, payload, and
fuel groups; the reference path uses Torenbeek-style group correlations and a
fuel remainder; coordinate helpers provide coarse representative locations.

Recommended contract:

- retain the ten top-level groups for compatibility, but add an item-level
  ledger behind them;
- make empty-state convention explicit and distinguish DWM, BEW/OEW, ZFW,
  ramp, takeoff, landing, reserve, usable, and unusable fuel;
- attach source, method, calibration delta, uncertainty, state predicate, and
  frame to every item;
- calculate CG and all six independent inertia-tensor terms from the ledger;
- prohibit silent negative-mass clamping in passing evaluations; return typed
  diagnostics for invalid rows;
- provide geometry-derived tank capacity and per-tank feedability separately
  from MTOW closure fuel;
- expose nominal and uncertainty-aware mass property states to downstream
  modules;
- compare Class-I, component, and calibrated methods and emit a
  MODEL-DISAGREEMENT residual when applicable methods diverge.

The current reference fuel calculation, MTOW minus empty mass minus payload,
should remain available only as a clearly labeled closure remainder for early
exploration. It must not be promoted to required mission fuel or tank
capacity.

### alas-payload

Current useful behavior: PayloadLayout stores deck items and computes total
mass and first moments in x/y.

Recommended contract:

- preserve item kind, deck, station, z, dimensions, height, mass, label, and
  metadata;
- add allowed placement/tie-down zones, occupancy rules, loading/unloading
  order, and item uncertainty;
- compute z first moment and the full payload inertia contribution, including
  item dimensions or a declared point-mass approximation;
- emit forward/aft/lateral/vertical payload cases and sequence traces;
- distinguish design payload, maximum payload, mission payload, crew,
  baggage/cargo, and payload-specific equipment;
- report unplaceable mass, deck/cabin capacity, and CG-envelope residuals
  rather than dropping an item or silently moving it to a cabin centroid.

### alas-mission

Current useful behavior: sequential segments carry forward the previous final
mass, integrate positive fuel flow, and retain partial telemetry on
exhaustion/nonconvergence.

Recommended contract:

- carry tank-local fuel state, transfer events, unusable fuel, and reserve
  policy between segments;
- return fuel-burned, fuel-remaining-by-tank, aircraft CG, tensor, and
  closure residual at segment boundaries;
- distinguish converged mission, fuel exhaustion, capacity shortfall, transfer/
  feed failure, and solver nonconvergence;
- recompute mass properties at burn/transfer events and at performance/stability
  checkpoints;
- retain partial traces for diagnosis but never mark an exhausted or
  nonconverged mission as a passing feasible result;
- propagate correlated mass/fuel/CG draws for robust mission screening.

### pipeline, feasibility, and requirements-first policy

The existing requirements-first document defines the correct high-level order:
geometry → payload → mass/CG/fuel → aero/stability/performance → mission →
structures → residuals. Mass/balance should therefore publish a typed,
state-indexed evidence object consumed by the later stages.

The current feasibility direction already separates MTOW-closure fuel, usable
capacity, carried-fuel basis, and mission-fuel status. Preserve that
separation and extend it with:

- mass-state and item-ledger provenance;
- capacity evidence based on tank geometry/topology;
- CG/inertia evidence and active loading case;
- uncertainty quantiles or interval policy;
- residual code, normalized severity, and hard/soft/provisional status.

Suggested gate outputs:

| Gate | Minimum mass/balance evidence |
|---|---|
| geometry | reference frame, tank/gear/propulsion locations, volumes |
| payload | item-placement cases, payload mass/CG/inertia, load sequence |
| mass/CG/fuel | item ledger, state closure, tank capacity/feedability, CG/tensor |
| aero/stability/performance | state-specific mass/CG/inertia and uncertainty envelope |
| mission | segment burn/transfer trace, reserve and landing state |
| structures | load-case masses, CG, inertia, payload/tank/gear attachment loads |
| residuals | typed closure and feasibility status; no hidden fallback |

## 10. Source ledger: verified local PDFs

All files below are present in bib/mass-balance, begin with a valid PDF
signature, and were parsed with pdfinfo. SHA-256 values are recorded to make
the local evidence set reproducible. Government reports and agency guidance
retain the rights status of their issuing record; an official-hosted PDF is
not automatically a CC-licensed work.

| Local PDF | Title | Authors | Year | DOI or source URL | Rights note | SHA-256 |
|---|---|---|---:|---|---|---|
| nasa-tm-2017-219627-vol1-flops-weight.pdf | The Flight Optimization System Weights Estimation Method | Douglas P. Wells; Bryce L. Horvath; Linwood A. McCullers | 2017 | https://ntrs.nasa.gov/citations/20170005851 | NASA NTRS record marked public use permitted; report metadata states no third-party material | 819a48fc9c8f34f14595d93f3e3d54dc8454298e83e64048c14ac7bda00bb51d |
| nasa-20190000431-weight-method-comparison.pdf | Comparison of Aircraft Conceptual Design Weight Estimation Methods to the Flight Optimization System | Bryce L. Horvath; Douglas P. Wells | 2018 | DOI 10.2514/6.2018-2032; https://ntrs.nasa.gov/citations/20190000431 | NASA NTRS record marked public/government use permitted; report metadata states no third-party material | 3690e6b138c3590a4f52a3c85299d7dedf3a7ae9df2aaaa516139b6b384541ab |
| nasa-cr-178163-composite-weight-estimation.pdf | Weight Estimation Techniques for Composite Airplanes in General Aviation Industry | T. Paramasivam; W. J. Horn; J. Ritter | 1986 | https://ntrs.nasa.gov/citations/19860022059 | NASA CR in public/government-use record; no separate CC license asserted | 34d781fe8de1ad95f733094655121565c3c2a7cf4c2f5f4e7bcff008e1f488d8 |
| nasa-cr-186663-avion-weight-balance.pdf | AVION: A Detailed Report on the Preliminary Design of a 79-Passenger, High-Efficiency, Commercial Transport Aircraft | William Mayfield; Brett Perkins; William Rogan; Randall Schuessler; Joe Stockert | 1990 | https://ntrs.nasa.gov/citations/19900014079 | NASA CR in public/government-use record; no separate CC license asserted | 23dea1f259e2d3923055829396e198faf2d339f51dfa709b79918e2727244848 |
| nasa-cr-165829-vol1-multibody-weight-balance-inertia.pdf | Multibody Aircraft Study, Volume I | J. W. Moore; E. P. Craven; B. T. Farmer; J. F. Honrath; R. E. Stephens; C. E. Bronson Jr.; R. T. Meyer; J. H. Hogue | 1982 | https://ntrs.nasa.gov/citations/19820024468 | NASA CR in public/government-use record; no separate CC license asserted | d2e522d9bdb53e00f06c6179dfd7534c3b79d7b341cd8be77d37efb56a2f0b2a |
| nasa-tm-78681-mass-properties-rigid-structure.pdf | Computer Program for Determining Mass Properties of a Rigid Structure | Reid A. Hull; John L. Gilbert; Phillip J. Klich | 1978 | https://ntrs.nasa.gov/citations/19780012592 | NASA TM in public/government-use record; no separate CC license asserted | c5ae08ad73a139f8edfaf9b7cd139df33de50867d8dd4c7a3a684b3b94dc94b2 |
| nasa-agard-549-pt1-mass-characteristics.pdf | Considerations in the Determination of Stability and Control Derivatives and Dynamic Characteristics from Flight Data, AGARD Report 549 Part I | Chester H. Wolowicz | 1966 | https://ntrs.nasa.gov/citations/19670020806 | Historical AGARD scan hosted by NASA NTRS; public/government record context, no CC license asserted | 357dea445e89cb1396fd29e33f4492f56183c835f938a4a531f7f1c0aa17ea97 |
| faa-ac-120-27f-weight-balance-control.pdf | Aircraft Weight and Balance Control, AC 120-27F | Federal Aviation Administration | 2019 | https://www.faa.gov/documentLibrary/media/Advisory_Circular/AC_120-27F.pdf | Official FAA advisory circular; guidance is generally non-binding and does not replace applicable regulations, AFM, or WBM | 726adba4c7050d366e430f17d117e25baf80308b564ffaaaacd4e7f337131f78 |
| faa-ac-25-7d-chg1-flight-test-guide.pdf | Flight Test Guide for Certification of Transport Category Airplanes, AC 25-7D Change 1 | Federal Aviation Administration | 2025 | https://www.faa.gov/documentLibrary/media/Advisory_Circular/AC_25-7D_Chg_1.pdf | Official FAA advisory circular/change; guidance is not a substitute for certification rules | da5783aec23cccc05345bb358f434e745b183f25cc8c50265c3da0e83325b16f |
| dlr-2024-106-conceptual-fixed-wing-vtol-toolchain.pdf | Development of a Toolchain for the Conceptual Design of Fixed-Wing VTOL UAVs | Ivo Poelma | 2024 | https://elib.dlr.de/206234/ | DLR eLib lists open access; no explicit CC license was shown, so cite and redistribute only under the repository's stated terms | 2302443ebc19b7eff239c7956c93fb1250ebfe0bf972bc46b88ffbd7d94c4b5c |
| dlr-2014-icas-uncertainty-framework.pdf | Aircraft Configuration Analysis Using a Low-Fidelity, Physics Based Aerospace Framework under Uncertainty Considerations | Till Pfeiffer; Erwin Moerland; Daniel Böhnke; Björn Nagel; Volker Gollnick | 2014 | https://elib.dlr.de/95125/ | DLR eLib lists open access; no explicit CC license was shown | 0017af4a41dbde9dba1ef9c4c63cb8f9d0ca280b0e94f644651e834c8b6ba551 |
| mdpi-2022-general-mass-property-measurement.pdf | General Mass Property Measurement Equipment for Large-Sized Aircraft | Xiaolin Zhang; Hang Yu; Wenyan Tang; Jun Wang | 2022 | DOI 10.3390/s22103912; https://www.mdpi.com/1424-8220/22/10/3912 | Article states Creative Commons Attribution 4.0 (CC BY 4.0); local copy came from an open repository mirror of the article | 5cfa9ec60cbc7bdac188f4472ed40df7b06789363e559f57d40d5e1f5ee5f1d7 |
| arxiv-2020-upper-trust-bound-feasibility.pdf | Upper Trust Bound Feasibility Criterion for Mixed Constrained Bayesian Optimization with Application to Aircraft Design | Rémy Priem; Nathalie Bartoli; Youssef Diouane; Alessandro Sgueglia | 2020 | https://arxiv.org/abs/2005.05067 | Public arXiv author manuscript; no assumption is made about third-party material | c39dc9f495736422aaaeddd3873855cee7f3f05988bd6c377e618288c14da819 |

### What each local source contributes

- FLOPS: a complete conceptual grouping and coupled mission/fuel-capacity
  context; use as a staged baseline and cross-check.
- NASA method comparison: comparison of FLOPS, Raymer-style, and Roskam-style
  processes; use to justify method ensembles and stage transitions.
- Composite weight report: material-aware and point-stress alternatives; use
  for configuration/material sensitivity and calibration caveats.
- AVION: component list, Roskam-style preliminary design practice, moment
  analysis, fuel-CG management, and radii-of-gyration estimates.
- Multibody Aircraft Study: group weight statements, CG/load envelopes,
  payload loading envelopes, and multiple inertia envelopes.
- Rigid-structure mass-properties program: geometry/material decomposition and
  full CG/moment/product calculation.
- AGARD 549: flight-data mass/CG/inertia measurement and fuel-consumption
  effects on dynamic characteristics.
- FAA AC 120-27F: operational weighing, change records, loading schedules,
  passenger/fuel movement, and envelope control.
- FAA AC 25-7D Change 1: unusable-fuel test logic, tank attitude/acceleration,
  pickup, pump, and failure cases.
- DLR 2024: requirements-driven conceptual toolchain, component methods,
  gear/CG, moments, and mass moment of inertia.
- DLR 2014: aleatory/epistemic/numerical uncertainty, correlation,
  Monte-Carlo/surrogate propagation, sensitivity, and sample convergence.
- Sensors 2022: integrated weighing and torsion-pendulum measurement pathway.
- arXiv 2020: uncertainty-aware constrained feasibility screening; apply as a
  screening pattern, while final physical closure remains authoritative.

## 11. Citation-only and unresolved sources

These sources were intentionally not copied into the local corpus. They remain
useful for method cross-checking, but the ALAS note does not reproduce
copyrighted equations or treat an inaccessible copy as verified evidence.

| Source | Year | DOI or source URL | Status and rights note | ALAS use |
|---|---:|---|---|---|
| Daniel P. Raymer, Aircraft Design: A Conceptual Approach, 6th ed. | 2018 | DOI 10.2514/4.104909; https://doi.org/10.2514/4.104909 | Copyrighted AIAA book; citation only, no PDF retained | staged Class-I/II mass-property workflow and component-method cross-check |
| Jan Roskam, Airplane Design: Part V, Component Weight Estimation | 1985 | https://books.google.com/books/about/Airplane_Design_Component_weight_estimat.html?hl=en&id=jIpYAAAAYAAJ | Copyrighted book; citation only, no PDF retained | component weight, systems, equipment, and mass-property workflow cross-check |
| Egbert Torenbeek, Advanced Aircraft Design | 2013 | DOI 10.1002/9781118568101; https://onlinelibrary.wiley.com/doi/book/10.1002/9781118568101 | Copyrighted Wiley book; citation only, no PDF retained | reference path currently exposes Torenbeek-style group correlations |
| Egbert Torenbeek, Synthesis of Subsonic Airplane Design | 1982 | https://research.tudelft.nl/en/publications/synthesis-of-subsonic-airplane-design/ | Copyrighted/personal-download terms; no redistribution | historical conceptual sizing and weight-balance cross-check |
| EASA CS-25 Amendment 28 | 2023, live page updated 2025 | https://www.easa.europa.eu/en/document-library/certification-specifications/cs-25-amendment-28 | Official current certification source; use the live EASA file because the page notes a replacement | certification-facing weight/CG and fuel-tank requirement traceability |
| AGARD-CP-373, Flight Test Techniques | 1984 | https://ntrs.nasa.gov/citations/19840026325 | NTRS record may contain publisher/copyright material; no local PDF retained | flight-test mass, CG, inertia, and dynamic-characteristics context |
| Marco Saporito; Andrea Da Ronch; Nathalie Bartoli; Sébastien Defoort, Robust multidisciplinary analysis and optimization for conceptual design of flexible aircraft under dynamic aeroelastic constraints | 2023 | DOI 10.1016/j.ast.2023.108349; https://www.sciencedirect.com/science/article/pii/S1270963823002468; https://eprints.soton.ac.uk/477582/ | Reported open/CC BY availability, but the institutional downloads returned access errors during this pass; not retained without a verified copy | flexible-aircraft robust MDO and dynamic-aeroelastic uncertainty cross-check |
| Ahmad Ali Pohya; Kai Wicke; Thomas Kilian, Introducing variance-based global sensitivity analysis for uncertainty enabled operational and economic aircraft technology assessment | 2022 | DOI 10.1016/j.ast.2022.107441; https://elib.dlr.de/185440/ | DLR record located but no verified open PDF downloaded | sensitivity-analysis cross-check for operational/economic uncertainty |

The EASA and FAA documents are guidance or certification references, not
permission to claim that a conceptual ALAS result is certified. Applicable
airworthiness rules, approved manuals, and authority findings control any
certification project.

## 12. Implemented mass-state correction

The first executable coupling from this memo is now in the normal pipeline.
The component mass statement still retains its structural `MTOW - ZFM` fuel
budget, but the native mission no longer flies that budget by default. It
iterates an explicit dispatch state against route trip burn and the supported
reserve policy, bounded independently by MTOW and usable tank capacity. Only
a converged selected load replaces the MTOW remainder in mission,
analyzed CG, design-mission departure/landing field checks, feasibility, and
CLI evidence. A failed closure retains its best iterate for diagnosis but does
not promote it to an operational loading state.

The product CG check now forms explicit fuel-remaining cases from that closure:

```text
takeoff fuel = converged loaded fuel
mid-mission fuel = loaded fuel - 0.5 * trip burn
reserve/arrival fuel = predicted landing fuel
```

The half-burn point is a representative mission state, not a claim that half
the block time or distance has elapsed. Gear reaction limits remain sized at
the configured design MTOW, so a lighter dispatch does not silently resize the
landing gear downward. The CG figure distinguishes the hard physical static-
margin floor from the optimizer's preferred margin and plots the analyzed TOW,
mid-mission, and reserve/arrival points.

At this fidelity all fuel is placed at the single preliminary fuel centroid.
Tank sequencing, transfer, unusable-fuel location, and centroid migration are
not resolved; the plot labels that limitation and no uncertainty interval is
invented. Until a tank-local model exists, these cases improve the mass states
but do not establish an operational loading envelope.

The design database and run manifest now retain both roles plus ZFM, trip burn,
reserve, landing fuel, dispatch/landing mass, capacity/MTOW margins, capacity
provenance, residual/tolerance, typed terminal status, and every iterate. The
fuel-capacity figure uses this selected state; if mission evidence is absent it
labels the old quantity only as an MTOW fuel budget. A non-converged dispatch
figure calls retained values the best evaluated iterate, not a dispatch
solution. The remaining work in this memo—tank-local burn sequencing, full
inertia tensors, structure-mass feedback,
and an outer geometry/OEW/mission closure—remains unevaluated rather than being
implied by this first correction.

The capacity provenance is now numerically reproducible rather than a label
alone. An unchanged registered preset retains the published usable volume,
published density when the source supplies it, exact model/weight-variant/
engine/modification/tank identity, and the quantity-specific revision-locked
fuel-capacity source locator. A changed/notional aircraft retains the gross
Torenbeek wing-volume estimate, the applied usable-volume fraction, usable
volume, configured constant Jet-A/Jet-A1 density at 15 deg C, and resulting
mass. The latter remains a conceptual estimate: thermal density change,
detailed tank boundaries, ribs/systems, feedable/unusable fuel, and ullage are
not modeled. Those absent quantities remain stated limitations, not inferred
values.

Mission telemetry now closes through two retained numerical paths. Endpoint
mass loss is compared against the sum of each segment's final pseudospectral
integral of propulsion mass flow. Every segment's integral is also checked
against its own first-to-last mass change, and each adjacent segment boundary
must preserve mass continuity; equal-and-opposite segment errors therefore
cannot cancel at mission level. Missing or dimensionally inconsistent
operators, non-finite flow samples, or a mismatch larger than the declared
mass tolerance reject the dispatch iterate as invalid telemetry. The flow
integral, mission residual, maximum segment residual, and maximum boundary
jump remain in the serialized iteration evidence and dispatch PNG. This does
not create an independent propulsion measurement, but it prevents the mass-
state identity from being used as its own conservation check.

## 13. Recommended ALAS acceptance tests

The following tests should become evidence fixtures when the implementation is
extended. They are written as behavioral requirements and do not require
particular Rust types.

1. **Mass identity.** For every state, the component sum equals the declared
   state mass within the configured tolerance, with a residual containing the
   state and item list on failure.
2. **CG identity.** A two-item analytic case produces the mass-weighted CG in
   all three axes; moving one item changes only the expected first moments.
3. **Tensor translation.** A point mass translated from its centroid to the
   aircraft CG matches the parallel-axis theorem and preserves tensor
   symmetry.
4. **State transitions.** Burning fuel from tank A changes total mass, CG,
   and tensor; transferring equal mass between tanks changes CG/tensor but not
   total mass.
5. **Volume closure.** Density, temperature, volume, ullage, and unusable
   fuel produce a capacity result independent of the MTOW closure remainder.
6. **Feedability.** A tank with sufficient total volume but no valid engine
   feed path fails with FUEL-FEED/TRANSFER.
7. **Loading sequence.** Forward/aft payload cases and an intermediate
   loading step are all retained, with no item silently dropped or relocated.
8. **Mission status.** Fuel exhaustion, segment nonconvergence, and reserve
   shortfall are distinct non-passing statuses with partial traces.
9. **Uncertainty correlation.** Correlated mass draws differ from an
   independent draw and the result reports the correlation policy and sample
   convergence.
10. **Hard-gate ranking.** A candidate with a smaller nominal mass but a tank
    or CG hard violation cannot outrank a valid candidate.
11. **Calibration provenance.** Updating a calibrated group records the old
    estimate, new estimate, source, delta, and applicability range.
12. **Invalid data.** Negative mass, missing position, inconsistent units,
    non-symmetric/indefinite tensor, or missing frame yields a diagnostic and
    cannot become a passing result through clamping.

## 14. Reproducibility and limitations

The local source ledger is limited to the 13 verified PDFs listed above. The
hashes identify the bytes used for this note; they do not establish that every
historical scan is a preferred edition. Several NASA and AGARD documents are
old reports, so their correlations should be treated as historical evidence
and calibrated before use on modern electric, hybrid, composite, distributed,
or VTOL configurations.

The DLR toolchain and uncertainty papers support the workflow but do not
substitute for a configuration-specific structural, propulsion, tank, or
certification analysis. The Sensors measurement accuracy is equipment- and
setup-specific. The arXiv feasibility method is a screening pattern, not a
physical mass-balance solver.

The FAA AC 120-27F and AC 25-7D documents are advisory guidance. They are
valuable for the cases and controls that a conceptual model should anticipate,
but the applicable regulation, approved flight manual, weight-and-balance
manual, operating limitations, and authority-approved data govern an actual
aircraft.

## 15. Short reference list

- Wells, Horvath, and McCullers, NASA/TM-2017-219627/Vol. 1, 2017.
- Horvath and Wells, AIAA-2018-2032, NASA NTRS 20190000431, 2018.
- Paramasivam, Horn, and Ritter, NASA-CR-178163, 1986.
- Mayfield et al., NASA-CR-186663, AVION, 1990.
- Moore et al., NASA-CR-165829, Multibody Aircraft Study, Vol. I, 1982.
- Hull, Gilbert, and Klich, NASA-TM-78681, 1978.
- Wolowicz, AGARD Report 549 Part I, 1966.
- Federal Aviation Administration, AC 120-27F, 2019.
- Federal Aviation Administration, AC 25-7D Change 1, 2025.
- Poelma, DLR-IB-FT-BS-2024-106, 2024.
- Pfeiffer et al., ICAS 2014 paper 0750, 2014.
- Zhang et al., Sensors 22(10), 3912, 2022, DOI 10.3390/s22103912.
- Priem et al., arXiv:2005.05067, 2020.

The primary URLs, rights notes, and byte hashes for these references are in
the source ledger above.
