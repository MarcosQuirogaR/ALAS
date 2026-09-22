# Performance and airport compatibility for conceptual transport aircraft

Research date: 2026-08-26
Repository: ALAS
Scope: takeoff and landing field performance, accelerate-stop/go and balanced
field length, climb and ceiling requirements, low-speed/high-lift and speed
limits, atmospheric and runway conditions, pavement compatibility, and fuel
reserves.
Requested artifact: a research and integration memo; no Rust source, fixture,
configuration, or repository-ledger files were modified.

## Executive conclusion

ALAS already has useful conceptual screening primitives: airport elevation and
ISA deviation, TODA/LDA, takeoff and landing lift coefficients, a Raymer-style
field-length correlation, V-speed factors, a cruise thrust-to-weight constraint,
and an algebraic OEI climb constraint. Those are appropriate as a first sizing
screen when they are labeled as proxies.

They are not yet a transport-category performance method. Certification
requirements do not reduce to a single takeoff-distance multiplier, a balanced
field factor, or one OEI thrust-to-weight inequality. A transport performance
record must bind mass, centre of gravity, configuration, engine state, runway
declared distances, obstacle height, atmospheric state, wind, runway gradient,
surface/contamination state, braking/reverse-thrust policy, and the applicable
speed and procedure schedule. It must also identify whether the result is a
conceptual prediction, a validated correlation, an approved-data translator, or
actual compliance evidence.

The recommended ALAS architecture is:

    requirements-first brief
        -> typed performance/airport load cases
        -> atmosphere, aircraft, propulsion, runway and pavement translators
        -> point-mass/energy evaluators with uncertainty
        -> named residuals and evidence status
        -> mission/pipeline feasibility and report traceability

The first implementation priority is not to add more default factors. It is to
replace ambiguous scalar fields with explicit load cases and declared
definitions. In particular:

1. model accelerate-go and accelerate-stop independently;
2. find balanced field length by a V1 search, rather than setting
   `BFL = TODR * factor`;
3. separate takeoff field distance, rejected-takeoff distance, landing
   distance, and runway declared distances;
4. integrate climb and time-to-climb over a defined speed, configuration, mass,
   and atmospheric schedule;
5. make OEI ceiling and cruise ceiling named criteria with a residual climb
   rate or gradient;
6. represent runway condition and pavement strength as typed data;
7. make fuel reserve policy part of the mission contract, not an unmodeled
   afterthought.

The compliance boundary should remain explicit. FAA AC 25-7D describes
acceptable flight-test methods for showing compliance with 14 CFR Part 25; it
does not turn a conceptual model into an approved certification method. The
[FAA flight-test guide](https://www.faa.gov/documentLibrary/media/Advisory_Circular/AC_25-7D.pdf)
and the [current EASA CS-25 Easy Access Rules page](https://www.easa.europa.eu/en/document-library/easy-access-rules/online-publications/easy-access-rules-large-aeroplanes-cs-25)
are the correct anchors for the regulatory interpretation, while the local
NASA, DLR, OpenAP, and flight-test papers provide conceptual and validation
evidence.

## 1. Evidence and fidelity boundary

### 1.1 What this memo can support

The local corpus supports four distinct uses:

- **Conceptual sizing:** fast correlations and point-mass models for matching
  charts, architecture trades, airport screening, and optimizer residuals.
- **Physics-based conceptual analysis:** time-marching takeoff/landing and
  climb models with explicit propulsion, lift, drag, braking, atmosphere, and
  runway inputs.
- **Validation evidence:** comparisons against published flight-test or
  operational data, used to estimate bias and uncertainty for a stated
  aircraft class and operating domain.
- **Certification-method traceability:** a map from a requirement to the
  relevant clause, advisory material, procedure, and evidence that a future
  certification program would need.

The corpus does not provide an approved ALAS certification method, approved
aircraft flight manual data, conformity inspection, test instrumentation,
flight-test results for an ALAS configuration, or a regulator's finding of
compliance.

### 1.2 Required status vocabulary

Every evaluated result should carry both a numerical residual and an evidence
status. Suggested values are:

| Status | Meaning | Permitted conclusion |
|---|---|---|
| `Proxy` | Empirical or simplified conceptual equation | Screening result only |
| `PhysicsBased` | Explicit point-mass/energy or 6-DOF calculation | Conceptual prediction within declared assumptions |
| `Validated` | Model compared against independent data with a stated domain and uncertainty | Evidence-backed estimate, not certification |
| `RegulatoryTranslator` | A requirement-specific implementation of an accepted method with the required inputs | Candidate compliance analysis; authority and program approval still required |
| `FlightTest` | Test evidence under an approved plan and configuration control | Potential compliance evidence, subject to certification process |
| `Unavailable` | Required translator, input, or evidence is missing | `Inconclusive`, never an automatic pass |

`Pass` should mean only that the evaluated candidate is inside the numerical
criterion at the selected fidelity. It should not be rendered as `certified`,
`approved`, or `compliant` unless the separate certification evidence graph
supports that statement.

### 1.3 Conceptual proxies versus certification-compliant methods

| Topic | Conceptual proxy acceptable for early sizing | Certification-level distinction |
|---|---|---|
| Takeoff | Point-mass integration, calibrated correlation, or a declared Raymer-style field-length equation | Required distance/path is determined for prescribed weights, altitude, temperature, configuration, engine state, speeds, runway condition, gradients, delays, and obstacle definitions; data and method must be acceptable to the authority |
| Balanced field | Search for the V1 at which accelerate-stop and accelerate-go distances are equal | Must use the defined rejected-takeoff and takeoff-path procedures, realistic crew response, engine failure point, stopway/clearway and declared-distance rules |
| Landing | Stall-speed and wing-loading correlation | Regulated landing distance is from the specified reference height to complete stop under the applicable operating/certification conditions and speed/configuration rules |
| Climb | Excess-thrust or excess-power equation at a point | Must use the applicable net flight path, engine-out segment, configuration, obstacle/turning and weight/ambient conditions |
| Time-to-climb | Integrate `dt = dh / ROC` using a conceptual speed schedule | Must define the aircraft operating procedure, atmosphere, thrust ratings, mass change, acceptable data, and any operational time requirement |
| Ceiling | Altitude where a chosen ROC threshold is reached | “Ceiling” is not one universal Part 25 scalar; the criterion, engine state, net/gross convention, temperature, weight, speed/Mach and systems limits must be named |
| V-speeds | Factors applied to stall and VMC estimates | VSR/VREF, VMC, V1, VR, V2, VAPP, VTD and operating limits have separate definitions, margins, demonstrations and procedures |
| Contamination | Sensitivity multiplier or friction assumption | Dry, wet, slippery-wet, snow, slush, ice and standing water require declared depth/coverage/condition reporting and approved aircraft/operator performance data |
| Pavement | ACN/PCN-style or generic gear-load screen | Current FAA strength reporting uses ACR/PCR; aircraft gear, tire pressure, flexible/rigid pavement, subgrade and traffic assumptions must be explicit |
| Reserves | Fixed trip fraction or fixed reserve mass | Fuel rules are authority-, operation-, aircraft- and route-specific; contingency, alternate, final reserve, critical failures, delay and discretionary fuel are separate terms |

## 2. Regulatory and guidance baseline

### 2.1 FAA and EASA transport performance

The basic FAA transport-category performance clauses are 14 CFR Part 25
Subpart B. The linked
[eCFR Part 25 Subpart B](https://www.law.cornell.edu/cfr/text/14/part-25)
should be used as the current web source; a local annual CFR snapshot is not
part of this corpus. The corresponding EASA structure is CS-25 in the
[Easy Access Rules for Large Aeroplanes](https://www.easa.europa.eu/en/document-library/easy-access-rules/online-publications/easy-access-rules-large-aeroplanes-cs-25).

The transport rules establish the shape of the problem:

- **14 CFR/CS 25.101, General:** performance must be shown for the applicable
  weights and operating conditions; procedures, configurations, atmospheric
  assumptions, and limitations matter.
- **25.105, Takeoff:** takeoff speeds and distances are evaluated over the
  relevant weights, altitude, ambient temperature and runway-gradient
  conditions; wind corrections and effective runway gradients are not optional
  metadata.
- **25.109, Accelerate-stop:** dry and wet rejected-takeoff cases include
  vehicle response, braking, runway condition, tire pressure/anti-skid and
  stopway treatment. The two-second interval after V1 is part of the dry
  rejected-takeoff treatment; this is not represented by a generic BFL factor.
- **25.111 and 25.113, Takeoff path and distance:** the takeoff path and
  takeoff distance use engine-out/all-engine branches and specified screen
  heights. The dry takeoff-distance rule compares the OEI and all-engine
  branches; wet-runway treatment has its own branch and reference height.
- **25.117, Climb:** the required climb demonstrations are evaluated at the
  applicable weights, altitude, temperature, configurations, and critical
  centre-of-gravity conditions.
- **25.119, Landing climb:** the all-engine landing climb gradient is 3.2
  percent in the referenced transport rule.
- **25.121, One-engine-inoperative climb:** for the common transport
  segments, the minimum gradients are:

  | Segment | Twin | Three-engine | Four-engine |
  |---|---:|---:|---:|
  | First segment | Positive | 0.3% | 0.5% |
  | Second segment | 2.4% | 2.7% | 3.0% |
  | Final segment | 1.2% | 1.5% | 1.7% |
  | Approach, OEI | 2.1% | 2.4% | 2.7% |

  These are requirements on a defined flight condition and path, not a
  universal thrust-to-weight design point. A conceptual model may translate
  the second-segment target into a thrust-to-weight inequality, but must retain
  the source segment, speed, configuration, engine state and fidelity label.
- **25.125, Landing:** the regulated landing distance is measured from the
  specified reference height to complete stop under the applicable conditions.
  The rule also sets a reference-speed relationship, including the familiar
  `VREF >= 1.23 VSR0` condition, subject to the full clause.
- **25.1505 and 25.335, speed limits:** VMO/MMO and the design airspeeds are
  different concepts. VMO/MMO is an operating limitation; it is not a cruise
  target and must sit below the relevant design/test speeds with the required
  margins.

FAA [AC 25-7D](https://www.faa.gov/documentLibrary/media/Advisory_Circular/AC_25-7D.pdf)
is especially useful for ALAS traceability because it describes acceptable
flight-test practices for the Part 25 performance clauses. Its status is
guidance: it identifies acceptable means, not the only possible means, and it
does not substitute for the approved certification basis or a compliance
finding.

### 2.2 Airport design guidance is not aircraft operational performance

FAA [AC 150/5325-4B, Runway Length Requirements for Airport Design](https://www.faa.gov/documentLibrary/media/Advisory_Circular/AC_150_5325-4B.pdf)
is an airport-design document. It is not an aircraft flight-operations
manual. For large aircraft and regional jets it directs airport designers to
the manufacturer's approved performance material or an approved aircraft
performance model, with aircraft weight, elevation, temperature, flap and
engine information. It distinguishes airport planning runway length from
operational field performance.

FAA [AC 150/5300-13B Change 1 with errata](https://www.faa.gov/documentLibrary/media/Advisory_Circular/AC-150-5300-13B-Airport-Design-Chg1-w-errata.pdf)
is the current airport-design geometry reference in the local corpus. It is
relevant to declared distances, runway/taxiway/stand geometry and obstacle
surfaces, but it does not remove the need for aircraft-specific performance
data. ICAO Annex 14 and Doc 9157 provide the international airport-design
counterpart; the current official publications are controlled references (see
Section 10).

### 2.3 Runway contamination and condition reporting

FAA [AC 150/5200-30D Change 2](https://www.faa.gov/documentLibrary/media/Advisory_Circular/150-5200-30D-chg-2-consolidated.pdf)
describes winter operations, runway-condition assessment and reporting. Its
important modeling consequence is that snow, slush, ice, standing water,
frost, compacted snow and wet/slippery-wet conditions are not one continuous
“runway friction multiplier.” A useful condition record includes:

- surface class and contaminant descriptor;
- depth and coverage;
- temperature and precipitation context;
- runway-condition code/report or an equivalent authority-defined record;
- braking/friction model and tire/anti-skid assumptions;
- reverse-thrust policy;
- the source of the aircraft performance correction.

ICAO Circular 355, *Assessment, Measurement and Reporting of Runway Surface
Conditions*, is the principal citation for the global GRF/RWYCC/RCR/RCAM
vocabulary. It is listed by ICAO as a controlled publication; no local copy is
claimed in this repository. The FAA winter-operations document is therefore
the local open guidance source, while Circular 355 remains a citation-only
international terminology source.

### 2.4 Pavement strength: do not silently equate ACN-PCN and ACR-PCR

FAA [AC 150/5335-5D, Standardized Method of Reporting Airport Pavement
Strength](https://www.faa.gov/documentLibrary/media/Advisory_Circular/150-5335-5D-Pavement-Strength-202501.pdf)
supersedes the prior FAA PCN reporting guidance and establishes the ACR-PCR
method for covered U.S. reporting. The aircraft number depends on weight,
landing-gear configuration, tire pressure, pavement type and subgrade; the
pavement number is the published capacity under the specified reporting
method. The AC explicitly warns that the ACR-PCR method has no mathematical
correlation to the former ACN-PCN method. FAA's
[design software page](https://www.faa.gov/airports/engineering/design_software)
identifies FAARFIELD support for ACR/PCR.

Consequently, the existing ALAS brief field named `acn` should be treated as a
legacy or conceptual airport-compatibility requirement. It must not be
converted to PCR by a hidden scale factor. The future model should either:

1. retain `ACN <= PCN` as an explicitly legacy authority case; or
2. use `ACR <= PCR` with a named ACR method, gear, tire pressure, pavement
   type, subgrade category, traffic and published PCR.

If only the legacy ACN value is available while the evaluator is ACR/PCR, the
result is `Unavailable/Inconclusive`, not `Pass`.

### 2.5 Fuel and reserve policy

Fuel reserve is an operational policy, not a single aircraft sizing constant.
Relevant open references are:

- [14 CFR 121.639, domestic operations](https://www.law.cornell.edu/cfr/text/14/121.639):
  destination, the most distant alternate where required, and 45 minutes at
  normal cruising fuel consumption for the domestic case.
- [14 CFR 121.645, flag and supplemental operations](https://www.law.cornell.edu/cfr/text/14/121.645):
  turbine cases include a trip-time percentage, alternate fuel and a holding
  reserve; no-alternate cases have a separate requirement and the operating
  specifications may be more conservative.
- [14 CFR 121.647, factors for computing fuel](https://www.law.cornell.edu/cfr/text/14/121.647):
  forecast wind/weather, traffic delays, approach/missed-approach and other
  foreseeable delays affect the dispatch calculation.
- EASA [CAT.OP.MPA.181 in the current Air Operations rules](https://www.easa.europa.eu/en/document-library/easy-access-rules/online-publications/easy-access-rules-air-operations?erules-id=ERULES-1963177438-12803):
  current aircraft-specific data is preferred; the structure includes
  contingency, destination/alternate, final reserve, critical-failure,
  additional and discretionary fuel.
- The public ICAO Annex 6 copy in the local corpus is an older, public ICAO
  copy, not the current controlled edition. Its fuel-planning section is useful
  for terminology and historical traceability, but current Annex 6 and ICAO
  Doc 9976 must be checked from the official controlled publication.

For conceptual ALAS work, the reserve contract should expose trip fuel,
contingency, destination, alternate/missed approach, final reserve,
critical-failure/additional fuel and discretionary fuel. A default 5 percent
trip fraction is a useful sensitivity case, not a globally valid reserve
policy.

## 3. Modeling findings by requirement

### 3.1 Declared runway distances and field-length definitions

Declared distances belong to the runway/airport record:

| Quantity | Meaning for the model |
|---|---|
| TORA | Takeoff run available |
| TODA | TORA plus any declared clearway |
| ASDA | TORA plus any declared stopway |
| LDA | Landing distance available |
| Clearway | Declared area available for the applicable takeoff path, with authority-defined geometry/obstacle conditions |
| Stopway | Declared area available for the accelerate-stop case, with authority-defined strength and geometry |

The aircraft result must be named separately:

- **Takeoff run/ground roll:** motion on the runway before lift-off.
- **Takeoff distance required / takeoff field length:** distance to the
  specified screen height, with the aircraft and procedure definition stated.
  A common dry Part 25 screen is 35 ft; a wet-runway branch can use a different
  prescribed reference.
- **Accelerate-stop distance (ASD):** accelerate to the selected V1, account
  for the engine-out or all-engine rejected-takeoff branch and crew response,
  then stop using the declared braking/reverse-thrust/stopway policy.
- **Accelerate-go distance:** accelerate through the failure event, continue
  with the critical engine inoperative, and reach the specified obstacle/path
  criterion.
- **Balanced field length (BFL):** the runway length at the V1 for which the
  accelerate-stop and accelerate-go limiting distances are equal, under the
  same mass, atmosphere, configuration, runway and procedure assumptions.
- **Landing distance:** from the prescribed reference height to a complete
  stop, with the landing speed, flare, touchdown, braking, runway condition
  and reverse-thrust definitions stated.

Balanced field is therefore a root-finding problem:

    find V1 such that ASD(V1) - AGO(V1) = 0

with the limiting takeoff distance and all declared-distance rules applied.
If the curves do not cross in the available V1 interval, the design is
unbalanced at that condition and the controlling branch must be reported.
Setting `BFL = TODR * 1.15` and `ASD = BFL` is a screening correlation, not a
balanced-field calculation.

The implementation should preserve both the raw curves and the selected
operating point. That permits a report to answer: which branch controls, what
V1 was selected, whether a stopway or clearway was used, and how much runway
margin remains.

### 3.2 Conceptual takeoff and landing models

An adequate early physics model can integrate the longitudinal equations in
small steps. A representative ground-roll equation is:

    m dV/dt = T(V, h, T_a, state) - D(V) - mu(V, surface) (W - L(V))

with runway-gradient and wind terms included in the ground-relative/air-relative
transformation. Rotation, lift-off, acceleration through V2, engine failure,
gear/flap schedules, transition and obstacle clearance then need separate
segments. The integration should retain uncertainty in thrust, lift, drag,
rolling/braking coefficient, crew response, wind and runway condition.

The present Raymer-style ALAS correlation,

    TODR proportional to (W/S) / (sigma CLmax T/W)

is useful for matching-chart screening. Its `bfl_factor`, speed factors and
landing constant should be treated as tunable correlations with source and
uncertainty, not as Part 25 translators. A new evaluator can use the
correlation as a fallback while returning `Proxy`.

Landing should similarly separate approach, flare, touchdown and braking.
The current `LDR = (W/S) K / (sigma CLmax)` is a landing-wing-loading screen.
It does not represent the regulated reference-height distance, VREF/VAPP
procedure, flare, braking, brake wear, runway slope, reverse thrust or
contaminant state. It should remain useful as a fast bound, but a transport
landing evaluator should report its assumptions explicitly.

### 3.3 Climb gradients and time-to-climb

The transport climb requirements are path requirements. A useful conceptual
energy relation is:

    ROC = (T - D) V / W

or, for a climb angle:

    gamma approximately (T - D) / W

with the small-angle approximation stated. The evaluator should include
aircraft mass change, density, thrust lapse, drag polar, compressibility,
configuration, speed schedule, engine-out thrust and any net-flight-path
correction. A single `T/W` point can screen a condition but cannot demonstrate
the full path.

Time-to-climb is not one universal Part 25 scalar. For an ALAS requirement such
as “1500 ft to initial cruise altitude in 25 min,” define:

- start and target pressure altitude;
- initial mass/fuel and a mass-loss model;
- atmosphere and ISA deviation;
- engine state and thrust rating;
- climb speed/Mach schedule and configuration schedule;
- climb-rate floor and any level-off/acceleration segments;
- required time and uncertainty allowance.

Then integrate:

    t = integral from h0 to h1 of dh / ROC(h, W, T, configuration, speed)

If `ROC` reaches zero, the time is undefined and the case fails or becomes
inconclusive according to policy. The report should also expose the minimum
ROC, peak ROC, fuel used and the altitude where the floor is first crossed.

### 3.4 OEI ceiling and cruise ceiling

“OEI ceiling” should be a named design metric, not an unqualified number. A
recommended definition is the highest pressure altitude at which a specified
one-engine-inoperative aircraft state achieves at least a specified residual
rate of climb or gradient. The record must name:

- critical engine and engine count;
- weight/fuel state and centre of gravity;
- configuration and bleed/anti-ice/system state;
- ISA deviation or actual atmospheric profile;
- speed or Mach schedule;
- gross versus net path convention;
- residual threshold, such as `ROC >= 100 ft/min`, if that is the design
  policy; this threshold is a project choice, not a universal Part 25 value.

This metric must be distinguished from an en-route net flight path, drift-down
or obstacle-clearance demonstration. A candidate with a high algebraic OEI
ceiling may still lack an acceptable operational drift-down path.

“Maximum operating cruise altitude” likewise needs a criterion. Candidate
definitions include service ceiling (`ROC >= 100 ft/min`), absolute ceiling
(`ROC = 0`), buffet-margin limit, pressurization limit, or an operational
maximum altitude below all of them. The model should report the controlling
limit rather than collapse them into one scalar. Mach/VMO, buffet margin,
thrust available, drag divergence, temperature and pressurization are separate
envelope boundaries.

### 3.5 Approach speed, high-lift and low-speed limits

Approach speed is a procedure and safety quantity, not just a stall-speed
multiplier. The model should separate:

- clean, takeoff and landing maximum lift coefficients;
- the stall reference used by the requirement;
- VSR/VSR0 or a conceptual stall equivalent;
- VREF and its regulatory margin;
- VAPP wind/additive policy;
- VTD/touchdown speed;
- flap/slat/gear configuration, propulsive lift and ground effect;
- mass, centre of gravity, icing/anti-ice and runway condition.

The current ALAS factors (`VAPP = 1.30 VS_L`, `VTD = 1.15 VS_L`) are sensible
conceptual scheduling defaults but do not claim the meaning of VREF or an
operator's VAPP procedure. The NASA sensitivity study in the local corpus
found that propulsion and lift-curve assumptions can dominate takeoff/landing
distance sensitivity; therefore `CLmax` should carry a high-lift evidence
source and uncertainty instead of being treated as a universal constant.

VMO/MMO should be modeled as an upper envelope limit. It is not the initial
cruise Mach requirement, and it must not be used as a substitute for buffet,
flutter, structural design-speed, or engine operating-limit analyses.

### 3.6 ISA deviations and airport elevation

ISA deviation is useful for a first-order density and temperature perturbation,
but it is not a complete airport atmosphere. The condition record should
distinguish:

- geometric elevation and pressure altitude;
- static pressure, temperature, density and humidity;
- ISA deviation and the reference altitude at which it is applied;
- hottest-month or design-day temperature policy;
- runway wind direction and speed;
- aircraft airspeed versus ground speed;
- engine thrust lapse and propulsive installation effects.

For airport design, FAA AC 150/5325-4B explicitly considers airport elevation,
effective runway gradient and temperature. For aircraft operations, the
aircraft-specific performance model must use the applicable actual or forecast
condition and approved data. The ALAS `Airport` record currently contains
elevation, TODA, LDA and ISA deviation, which is a good minimum screen, but it
lacks the condition and declared-distance detail needed for a transport
airport compatibility result.

### 3.7 Slope, wind, runway surface and contamination

Runway slope changes both the acceleration and the stopping work. Wind changes
the relation between airspeed and ground speed and must be applied with a
declared sign convention. The evaluator should not bury either effect inside a
single “effective distance” multiplier.

A runway case should specify at least:

- runway direction and reference frame;
- TORA/TODA/ASDA/LDA, stopway and clearway;
- effective longitudinal gradient and local slope;
- headwind/tailwind/crosswind;
- dry, wet, slippery-wet or contaminated surface class;
- contaminant type, depth and coverage;
- runway condition code/report or equivalent;
- tire pressure, anti-skid, wheel/brake state and reverse-thrust policy;
- obstacle/screen height and obstacle location;
- whether the result is dry, wet, contaminated, dispatch, or landing-distance
  data.

Do not use a dry-runway calculation as a wet or contaminated result merely by
multiplying distance by a generic factor. If aircraft-specific contaminant
data is absent, produce a sensitivity envelope and mark the result
`Unavailable` for any hard certification-style requirement.

### 3.8 Airport geometry and pavement compatibility

Airport compatibility is broader than wingspan. DLR's people-mover study in the
local corpus shows the coupled effects of approach category, runway length,
wing span, gear span and airport circulation geometry. Relevant checks can
include:

- approach category and approach speed;
- wingspan and outer-engine/gear span;
- overall length and turning radius;
- runway/taxiway and taxiway/object clearances;
- stand, gate, de-icing, run-up and hangar compatibility;
- declared runway distances and obstacle surfaces;
- pavement strength and traffic assumptions.

The existing brief's span and ACN fields should remain visible as requirements,
but the evaluator should expose whether each is a geometric screen, a declared
airport-code category, or a pavement-strength analysis.

## 4. State-of-the-art and validation evidence

### 4.1 NASA conceptual methods

The NASA FLOPS report is the closest local precedent for integrating these
requirements in a conceptual system. It combines weights, aerodynamics,
propulsion, mission, noise and field-length modules, and its design constraints
include upper approach speed, upper takeoff/landing field length, missed
approach climb, second-segment climb and fuel volume. Its BFL treatment includes
OEI takeoff, all-engine aborted and OEI aborted branches, rather than a single
unqualified factor. FLOPS is still a conceptual/preliminary design system; it
does not supply aircraft-specific certification evidence for ALAS.

The NASA STOL regional-jet concept study shows how field length and high-lift
choices can be traded at conceptual level. The NASA Rapid CDE flight-dynamics
paper shows the next fidelity step: a 6-DOF/flight-dynamics environment can
evaluate takeoff, landing and thrust-loss events. The NASA sensitivity paper
is a warning against overconfidence in simple correlations: propulsion,
lift-curve, braking coefficient and approach geometry can materially control
the result.

### 4.2 DLR and operationally informed methods

DLR's market-driven field-performance paper derives a takeoff-field-length
requirement from runway/market distributions and corrects physical runway
lengths for elevation and reference temperature. It supports using field
performance as an airport access requirement, but it does not turn a market
percentile into a certification criterion.

DLR's people-mover study links approach category, runway length, span and
airport circulation. It is a useful source for a systems-level airport
compatibility load-case family.

Poll and Schumann's open 2025 full-flight-profile model provides a transparent
conceptual method for climb, cruise, descent, holding, operating Mach/flight
level and approximate takeoff/landing. It compares with complete flight data
for several transport types and is valuable for time-to-climb/fuel regression
tests. It is an operational performance model with stated approximations, not
an approved transport certification method.

### 4.3 Open performance and flight-test validation

OpenAP is an open-source aircraft performance model assembled from public
aircraft/engine properties, kinematic and dynamic models, and surveillance-data
observations. Its published validation is useful for a transparent baseline
and regression tests, while its authors also identify limited flight data as a
validation limitation.

The Southampton takeoff-uncertainty paper compares a Monte Carlo takeoff
simulator with eight instrumented BAe Jetstream 3100 takeoffs. The measured
35-ft results were generally close, with an outlier associated with a likely
wind change. This supports retaining uncertainty distributions and event-level
diagnostics in ALAS rather than returning only a deterministic distance.

The NASA F-104 report and the AGARD/NASA Airbus/Concorde wind-tunnel versus
cruise comparison show the older but still important validation lesson:
performance prediction must be reconciled to flight data, and drag/propulsion
uncertainty can be comparable to the margin being optimized. They are useful
validation-method references, not direct transport design data for ALAS.

## 5. Proposed typed load cases

The following is a design sketch for `alas-config`/`alas-perf`; it is not a
source edit. It deliberately separates aircraft state, airport condition,
procedure policy and evidence. Each case should have a stable identifier and
be serializable into a run manifest.

~~~rust
enum PerformanceLoadCase {
    TakeoffField {
        airport_id: String,
        runway_id: String,
        mass_kg: f64,
        cg_percent_mac: f64,
        flap_config: String,
        thrust_rating: String,
        anti_ice: bool,
        engine_state: EngineState,
        v1_policy: V1Policy,
        obstacle_height_m: f64,
    },
    AccelerateStop {
        airport_id: String,
        runway_id: String,
        mass_kg: f64,
        cg_percent_mac: f64,
        flap_config: String,
        engine_failure_speed_policy: V1Policy,
        runway_surface: RunwaySurface,
        brake_state: BrakeState,
        reverse_thrust_policy: ReverseThrustPolicy,
        crew_response_s: f64,
    },
    AccelerateGo {
        airport_id: String,
        runway_id: String,
        mass_kg: f64,
        cg_percent_mac: f64,
        flap_config: String,
        critical_engine: usize,
        failure_speed_policy: VefPolicy,
        segment_definition: TakeoffPathDefinition,
        obstacle_height_m: f64,
    },
    LandingField {
        airport_id: String,
        runway_id: String,
        landing_mass_kg: f64,
        cg_percent_mac: f64,
        landing_config: String,
        vref_rule: VrefRule,
        vapp_policy: VappPolicy,
        runway_surface: RunwaySurface,
        brake_state: BrakeState,
        reverse_thrust_policy: ReverseThrustPolicy,
        obstacle_height_m: f64,
    },
    ClimbAeo {
        start_altitude_m: f64,
        target_altitude_m: f64,
        mass_kg: f64,
        isa_deviation_c: f64,
        speed_schedule: SpeedSchedule,
        configuration_schedule: ConfigurationSchedule,
    },
    ClimbOei {
        engine_count: u8,
        critical_engine: usize,
        segment: OeiClimbSegment,
        mass_kg: f64,
        altitude_m: f64,
        isa_deviation_c: f64,
        residual_gradient_or_roc: ClimbCriterion,
    },
    TimeToClimb {
        start_altitude_m: f64,
        target_altitude_m: f64,
        mass_kg: f64,
        isa_deviation_c: f64,
        speed_schedule: SpeedSchedule,
        climb_rate_floor_m_s: f64,
        target_time_min: f64,
    },
    Ceiling {
        mode: CeilingMode,
        engine_state: EngineState,
        mass_kg: f64,
        isa_deviation_c: f64,
        residual_roc_m_s: f64,
        speed_policy: SpeedSchedule,
    },
    CruiseEnvelope {
        mass_kg: f64,
        altitude_m: f64,
        mach: f64,
        vmo_m_s: f64,
        mmo: f64,
        buffet_margin: Option<f64>,
    },
    ReserveMission {
        trip: MissionLeg,
        contingency: ContingencyPolicy,
        alternate: Option<MissionLeg>,
        final_reserve: ReserveCriterion,
        additional_fuel: AdditionalFuelPolicy,
        discretionary_fuel_kg: f64,
    },
    Pavement {
        mass_kg: f64,
        gear_configuration: GearConfiguration,
        tire_pressure_kpa: f64,
        pavement_type: PavementType,
        subgrade_category: String,
        traffic_cycles: f64,
        published_pcr: Option<f64>,
        legacy_acn: Option<f64>,
    },
}

struct AirportCondition {
    elevation_m: f64,
    pressure_altitude_m: Option<f64>,
    temperature_c: f64,
    isa_deviation_c: f64,
    humidity: Option<f64>,
    runway_direction_deg: f64,
    tora_m: f64,
    toda_m: f64,
    asda_m: f64,
    lda_m: f64,
    stopway_m: f64,
    clearway_m: f64,
    effective_gradient: f64,
    wind: WindVector,
    surface: RunwaySurface,
    rwycc_or_rcr: Option<RunwayConditionReport>,
    contaminant_depth_m: Option<f64>,
    contaminant_coverage: Option<f64>,
    obstacle_height_m: f64,
    pavement_pcr: Option<f64>,
    pavement_type: Option<PavementType>,
    subgrade_category: Option<String>,
}
~~~

The enum should eventually support a case family, for example all runway
directions and hot-day temperatures for a given airport. A single “airport
performance” scalar hides the controlling case and makes optimizer results
hard to audit.

## 6. Proposed residuals and policy semantics

Use a common violation-residual convention: positive means violation,
negative means margin, and zero is the boundary. For an upper-bound
requirement, use `predicted - allowed`; for a lower-bound requirement, use
`required - predicted`. Each residual should carry units, case ID, fidelity,
source/evaluator ID, uncertainty, and `RequirementPolicy`.

| Requirement/evaluator | Proposed residual | Positive means | Initial owner |
|---|---|---|---|
| Takeoff distance | `todr_m - toda_m` | Required takeoff distance exceeds TODA | `alas-perf`, pipeline |
| Accelerate-stop | `asd_m - asda_m` | Rejected-takeoff distance exceeds ASDA | `alas-perf`, pipeline |
| Accelerate-go | `ago_m - toda_m` | Continued takeoff path exceeds TODA | `alas-perf`, pipeline |
| Balanced field | `abs(asd_m - ago_m) - balance_tolerance_m` plus controlling declared-distance residuals | The selected V1 is not balanced within policy or a branch exceeds declared distance | `alas-perf` |
| Landing distance | `ldr_m - lda_m` | Landing distance exceeds LDA | `alas-perf`, pipeline |
| AEO landing climb | `required_gradient - achieved_gradient` | Achieved gradient is below the criterion | `alas-perf` |
| OEI climb gradient | `required_gradient - achieved_gradient` | Net/gross OEI climb is below the named segment criterion | `alas-perf` |
| Climb rate floor | `required_roc_m_s - achieved_roc_m_s` | Rate of climb is below the case floor | `alas-perf`, mission |
| Time-to-climb | `achieved_time_min - target_time_min` | The climb takes too long | `alas-perf`, mission |
| OEI ceiling | `required_roc_m_s - roc_at_ceiling_m_s` or `target_altitude_m - achievable_altitude_m` | The named OEI criterion is not achieved | `alas-perf`, mission |
| Cruise ceiling | `target_altitude_m - achievable_altitude_m` | Target altitude is above the defined usable ceiling | `alas-perf`, mission |
| Mach/VMO/MMO | `mach - mmo` and `kcas - vmo_kcas` | The operating point exceeds a limit | `alas-perf` |
| Approach speed | `vapp_kcas - limit_kcas` | Approach-speed airport/category limit is exceeded | `alas-perf`, airport |
| Span/gear/length | `actual - airport_limit` | Geometry exceeds an airport constraint | `alas-geom`/`alas-perf` |
| Pavement ACR/PCR | `acr - pcr` | Aircraft rating exceeds published pavement capacity | `alas-perf`/airport |
| Legacy ACN/PCN | no numerical conversion unless the chosen authority method supports it | Method is unavailable or not comparable | diagnostic/inconclusive |
| Reserve fuel | `required_landing_fuel_kg - predicted_landing_fuel_kg` | The mission ends below the policy reserve floor | `alas-mission`, pipeline |

For a hard requirement, a positive residual blocks feasibility. For a soft
requirement, it contributes a visible penalty. For a diagnostic requirement,
the residual is reported but does not affect feasibility. For a required
translator with missing inputs or a non-comparable standard, status is
`Inconclusive`; it must not be coerced to zero.

Balanced-field residuals need two layers. The first checks whether the ASD and
accelerate-go curves are equal at the selected V1. The second checks the
selected takeoff and stop distances against TODA and ASDA. This prevents a
numerically balanced but operationally unusable point from passing.

## 7. Mapping to the current ALAS model

### 7.1 Existing baseline and gaps

| Requirement | Current ALAS baseline | Gap / recommended next translator |
|---|---|---|
| Airport elevation and ISA deviation | `crates/alas-config/src/airports.rs` has `elevation_m` and `isa_deviation_c`; atmosphere derives density ratio | Add pressure/temperature/humidity, wind, direction and effective gradient; preserve the baseline fields as inputs to a richer condition |
| TORA/TODA/LDA | `Airport` stores `toda_m` and `lda_m` | Add TORA, ASDA, stopway, clearway, runway direction and obstacle metadata; do not infer ASDA/TORA from TODA/LDA |
| Takeoff field length | `crates/alas-perf/src/performance/speeds.rs` uses a Raymer-style `37.7` correlation | Retain as `Proxy`; add point-mass or validated correlation evaluator with obstacle height, wind, slope, runway condition and engine state |
| Balanced field length | `PerformanceConfig::bfl_factor` defaults to `1.15` | Replace or supplement with a V1 search over ASD/AGO; retain factor only as a clearly named fallback correlation |
| Accelerate-stop | `FieldPerformance` sets `asd_m = bfl_m` | Implement reaction/braking/anti-skid/reverse-thrust/stopway and wet/contaminated branches; report AEO and OEI rejected-takeoff branches |
| Accelerate-go | No independent result; takeoff feasibility is based on TODR and static T/W | Add engine-failure event, continued takeoff path, segment/obstacle definition and critical-engine state |
| Landing field length | Wing-loading correlation with `k_land` and arrival mass | Add reference-height, VREF/VAPP, flare/touchdown, braking and runway-condition model |
| V-speeds | `crates/alas-perf/src/performance/speeds.rs` computes VMC/V1/VR/V2/VAPP/VTD from factors | Store definitions and evidence; distinguish VSR/VREF from operational VAPP; add V1/VEF policies and configuration dependencies |
| Cruise Mach and limits | `PerformanceConfig` and `DesignBrief` expose cruise Mach, MMO and VMO | Evaluate Mach/VMO/MMO as separate envelope constraints with buffet/structural/engine evidence |
| AEO/OEI climb | `crates/alas-perf/src/performance/constraints.rs` has cruise and algebraic OEI `T/W` constraints; pipeline labels it “engine-out second-segment climb” | Add named segment, net/gross convention, speed, configuration, engine-out, mass, altitude, temperature and achieved-gradient result |
| Time-to-climb and ICA | `DesignBrief` has `ica_m` and `ttc_min`; mission has climb segments | Add a climb-profile evaluator and connect it to the brief residuals; report fuel/mass coupling and the first altitude at which the ROC floor is crossed |
| OEI ceiling | `DesignBrief` has `oei_ceiling_m`; no dedicated ceiling evaluator | Define threshold and engine/configuration state; solve altitude for the residual ROC/gradient criterion |
| Cruise ceiling | `maximum_cruise_altitude_m` exists; no defined service/absolute/buffet/pressurization criterion | Define ceiling mode and controlling limit; connect to atmosphere, aero, propulsion and mission |
| Airport slope and runway condition | Not present in `Airport` or `FieldPerformance` | Add `AirportCondition` and separate dry/wet/contaminated load cases |
| Pavement | `DesignBrief` exposes `acn`; no pavement evaluator | Add ACR/PCR or an explicitly legacy ACN/PCN evaluator; carry gear, tire pressure, pavement and subgrade |
| Reserves | `ReserveFuelPolicy` exists in `design_brief.rs` with trip fraction, fixed mass, diversion, holding and landing floor | Active fuel assessment says trip fuel excludes unmodeled reserves; make reserve policy part of mission evaluation and expose landing-fuel residual |
| Airport geometry | `span_limit_m` exists in the brief; geometry can provide wingspan | Add gear span, length, turning and stand/taxiway/de-icing/run-up cases |

### 7.2 Mapping to the requirements-first brief and owners

`docs/REQUIREMENTS_FIRST_AIRCRAFT_DESIGN.md` already places these requirements
in the intended architecture:

- cruise Mach, MMO and VMO map to atmosphere/aero/envelope;
- ICA and time-to-climb map to climb/performance;
- OEI ceiling maps to propulsion/climb;
- maximum cruise altitude maps to mission/performance;
- TOFL and landing distance map to field performance;
- approach speed maps to high-lift/landing;
- span maps to geometry/airport compatibility;
- ACN maps to pavement;
- range and reserves map to the mission solver;
- policies and hard/soft/diagnostic treatment belong in the canonical
  `DesignBrief` requirement records.

The active adapter in `crates/alas-pipeline/src/design_brief.rs` projects the
brief into legacy configuration, while
`crates/alas-pipeline/src/feasibility.rs` currently runs legacy field checks.
`crates/alas-opt/src/objective_model.rs::design_brief_residuals` currently
adds payload/capacity/span residuals but not the performance/airport residuals
listed above. This is the principal integration gap: the requirements-first
document is richer than the active residual evaluator.

Recommended ownership:

| Layer | Responsibility |
|---|---|
| `alas-config` | Versioned typed load cases, airport conditions, reserve policy, speed/ceiling/criterion definitions |
| `alas-atmo` | Pressure, temperature, density, humidity, ISA deviation and atmosphere provenance |
| `alas-aero`/`alas-prop` | Lift/drag/high-lift and thrust/engine-state translators with validity ranges |
| `alas-perf` | Field-length, V-speed, climb, ceiling, envelope and pavement evaluators; uncertainty and evidence |
| `alas-mission` | Climb time, mass/fuel coupling, reserve segments and landing-fuel accounting |
| `alas-pipeline` | Instantiate case families, run evaluators, aggregate named residuals and expose inconclusive results |
| `alas-opt` | Consume policy-tagged residuals after hard constraints are evaluated; never invent missing performance translations |
| `alas-report`/`alas-viz` | Show controlling cases, raw ASD/AGO curves, margins, uncertainty and evidence status |
| `docs/REQUIREMENTS_FIRST_AIRCRAFT_DESIGN.md` | Requirement intent, policy, source, load-case binding and acceptance semantics |

## 8. Recommended implementation sequence

This sequence is deliberately incremental and can preserve the existing
conceptual behavior:

1. **Schema and semantics.** Add stable IDs and typed records for runway
   declared distances, environmental condition, runway surface, engine state,
   configuration, speed policy, climb criterion, pavement method and reserve
   policy. Keep old scalar fields as compatibility inputs with `Proxy` status.
2. **Field-performance translators.** Implement independent TODR, ASD and AGO
   evaluators. Add the V1 search and retain raw curve samples. First support
   smooth dry runway, then wet, then contamination only when the required
   data is available.
3. **Landing and low-speed model.** Separate VSR/VREF/VAPP/VTD and implement
   approach, flare, touchdown and braking states. Add high-lift evidence and
   uncertainty.
4. **Climb/ceiling profile.** Integrate AEO and OEI climb with mass change,
   speed/configuration schedules and named segment/ceiling criteria. Use the
   same evaluator for time-to-climb and ceiling residuals.
5. **Mission reserve contract.** Compute trip, contingency, alternate/missed
   approach, final reserve, critical-failure/additional and discretionary fuel
   as separate quantities. Feed predicted landing fuel to the reserve residual.
6. **Airport/pavement compatibility.** Add runway slope/wind/condition and
   geometry/pavement cases. Start with an ACR/PCR method or explicitly label an
   ACN/PCN case as legacy.
7. **Pipeline and optimizer integration.** Convert evaluator results to
   evidence-bearing residuals, evaluate hard cases first, and prevent
   `Unavailable` from passing. Add report tables of the controlling load case.
8. **Parity and validation.** Preserve a fixture for the current Raymer
   screen, then add manufactured analytical cases, published-data regression
   cases, and uncertainty/Monte Carlo checks. Only promote an evaluator from
   `Proxy` to `Validated` when its domain and error are demonstrated.

Validation should include at least:

- dimensional and monotonicity tests for density, weight, slope, wind and
  temperature effects;
- analytical ground-roll and constant-ROC cases;
- V1 curve/root tests with an intentionally non-crossing case;
- regression against the published OpenAP/DLR/Poll examples where inputs are
  recoverable;
- a flight-test-style takeoff uncertainty test patterned after the
  Southampton Jetstream study;
- tests that missing RWYCC, pavement method or reserve translator produce
  `Inconclusive`, not a zero residual;
- a campaign fixture proving that the same requirement, load case, model
  version and source metadata reach the pipeline and report.

## 9. Unresolved and citation-only material

The following sources were not copied locally because they are controlled,
edition-sensitive or their public endpoint was not stable:

| Source | Why it remains citation-only | Use in ALAS |
|---|---|---|
| [ICAO Annex 14, Volume I, current edition/amendments](https://store.icao.int/en/annex-14-aerodromes) | Official current publication is sold/secured; the ICAO store identifies the current amendment state | Airport reference code, runway geometry, obstacle and aerodrome design vocabulary |
| [ICAO Doc 9157, Part 1, Runways](https://store.icao.int/en/aerodrome-design-manual-runways-doc-9157-part-1) | Official PDF is paid/secured | Runway length and runway-design methodology |
| [ICAO Doc 10064, Aeroplane Performance Manual](https://www.icao.int/aerodrome-certification) | Official controlled publication; ICAO's aerodrome-certification page lists the manual among the relevant references | Aircraft-performance/airport-compatibility interface |
| [ICAO Circular 355](https://store.icao.int/en/assessment-measurement-and-reporting-of-runway-surface-conditions-cir-355) | Official store copy is controlled; public mirrors/endpoints were unavailable or unstable | GRF/RWYCC/RCR/RCAM terminology and runway-surface assessment |
| [ICAO Doc 9976 / current FPFM guidance](https://store.icao.int/en/flight-planning-and-fuel-management-fpfm-manual-9976) | Official controlled publication | Flight planning and reserve implementation |
| [Current ICAO Annex 6 Part I](https://store.icao.int/en/annexes/annex-6) | Official current edition is controlled; the local public copy is an older edition | Fuel/reserve operational rule cross-check |
| Current eCFR Part 25 and Part 121 | Web regulations are edition-sensitive; no local PDF was needed | Current U.S. legal wording and operational reserve baseline |
| Current EASA Easy Access Rules | Local PDF is a January 2023 consolidated snapshot; current page should be checked for later amendments | Current CS-25 and air-operations wording |
| Manufacturer AFM/FCOM/aircraft performance manuals | Usually proprietary and aircraft-specific | Certification/operational performance data for a real aircraft |

The unresolved items are not reasons to weaken the conceptual model. They are
reasons to keep source version, authority, rights and evidence status explicit.

## 10. Local source ledger

All local PDFs below were downloaded from a public official or institutional
endpoint on 2026-08-26. SHA-256 values are for the exact local bytes, rendered
in uppercase. A local copy is a research artifact; it does not change the
copyright, licence, access terms or amendment status of the original source.

### 10.1 Regulation and airport guidance

| Local PDF and metadata | Official/source URL | Rights note | SHA-256 |
|---|---|---|---|
| [FAA AC 25-7D, Flight Test Guide for Certification of Transport Category Airplanes](../../bib/performance-airport/faa_ac25_7d_flight_test_guide.pdf), 2018-05-04, 481 pp. | [FAA PDF](https://www.faa.gov/documentLibrary/media/Advisory_Circular/AC_25-7D.pdf) | FAA advisory circular; public government guidance. Use as acceptable-means guidance, not as an ALAS approval. | `47DCB52C8EA7B74E1A2A58C5F9EBAE38B5798F863FDFB4C93CA6A7FC74B9DEFE` |
| [EASA Easy Access Rules for Large Aeroplanes (CS-25), Amendment 27](../../bib/performance-airport/easa_cs25_jan2023_easy_access_rules.pdf), consolidated PDF, 1,495 pp., PDF created 2023-01-18 | [EASA current online rules](https://www.easa.europa.eu/en/document-library/easy-access-rules/online-publications/easy-access-rules-large-aeroplanes-cs-25) | EASA consolidated reading aid; check current amendment and legal publication. EASA rights remain with EASA. | `76B28A91EE2A24EF5EEA3A72F26FC6D95C9758D9C9342A8070E27B12FD9E28C6` |
| [FAA AC 150/5325-4B, Runway Length Requirements for Airport Design](../../bib/performance-airport/faa_ac150_5325_4b_runway_length.pdf), 2005-07-01, 42 pp. | [FAA PDF](https://www.faa.gov/documentLibrary/media/Advisory_Circular/AC_150_5325-4B.pdf) | FAA airport-design guidance; public government guidance. It is not an aircraft operational performance approval. | `93C4753A39BF21B4FDEBCEF3B9815318514B0F9F5D3CBEEC9EFCBDDD9948AB1A` |
| [FAA AC 150/5300-13B Change 1 with errata, Airport Design](../../bib/performance-airport/faa_ac150_5300_13b_chg1_errata_airport_design.pdf), consolidated 2022/2024/2025 errata, 413 pp. | [FAA PDF](https://www.faa.gov/documentLibrary/media/Advisory_Circular/AC-150-5300-13B-Airport-Design-Chg1-w-errata.pdf) | FAA airport geometry/design guidance; public government guidance. | `09BB2C0C01FB29022CA48FDFA62BC5190B692E42AB9E2E41D190D8BB9942312C` |
| [FAA AC 150/5320-6G, Airport Pavement Design and Evaluation](../../bib/performance-airport/faa_ac150_5320_6g_pavement_design.pdf), 2021-06-07, 195 pp. | [FAA PDF](https://www.faa.gov/documentLibrary/media/Advisory_Circular/150-5320-6G-Pavement-Design.pdf) | FAA public airport-pavement guidance. Strength reporting is cross-referenced to AC 150/5335-5D. | `95F896E8950D66B94D04BB119A38F5F5E5E49B44F0E189A78597758D3074C203` |
| [FAA AC 150/5335-5D, Standardized Method of Reporting Airport Pavement Strength—PCR](../../bib/performance-airport/faa_ac150_5335_5d_pcr.pdf), 2022-04-29 with 2025-01-06 errata, 102 pp. | [FAA PDF](https://www.faa.gov/documentLibrary/media/Advisory_Circular/150-5335-5D-Pavement-Strength-202501.pdf) | FAA public government guidance; current FAA ACR/PCR reference in this corpus. | `3ABBEC15EC2FD5056C87082A0C2D6EA03CCEAF55D778CF14D71AEF85EFC17E92` |
| [FAA AC 150/5200-30D Change 2, Airport Winter Safety and Operations](../../bib/performance-airport/faa_ac150_5200_30d_chg2_runway_conditions.pdf), 2016 original, Change 2 2020-10-29, 106 pp. | [FAA PDF](https://www.faa.gov/documentLibrary/media/Advisory_Circular/150-5200-30D-chg-2-consolidated.pdf) | FAA public winter-operations guidance; aircraft-specific contaminant data remains required. | `28CC045707FC954D33C440548AF532FD14F81455CE34C7CE8752894D927B750E` |
| [ICAO Annex 6, public copy](../../bib/performance-airport/icao_annex6_public_copy.pdf), Part I, 11th edition with Amendment 44 (2018/2020 copy) | [ICAO-hosted public copy](https://www.icao.int/sites/default/files/safety/CAPSCA/PublishingImages/Pages/ICAO-SARPs-%28Annexes-and-PANS%29/Annex%206.pdf) | Public ICAO-hosted legacy/in-progress copy; copyright remains ICAO. It is not the current controlled Annex 6 edition and must not be presented as such. | `1FF84915DDEA3364AEF5CF56B2096A623291A41819BCD73AE5058859C1FCDDA3` |

### 10.2 NASA, AGARD and DLR conceptual methods

| Local PDF and metadata | Official/source URL | Rights note | SHA-256 |
|---|---|---|---|
| [Geiselhart, “A Technique for Integrating Engine Cycle and Aircraft Configuration Optimization,” NASA CR-191602](../../bib/performance-airport/nasa_flops_conceptual_design_system.pdf), Feb. 1994, 78 pp. | [NASA NTRS PDF](https://ntrs.nasa.gov/api/citations/19940022103/downloads/19940022103.pdf) | NASA contractor report available through NTRS; public access. Contractor/cited third-party material retains its original terms. | `A3C1A510087AE9D563DA6B1811F69A632125BD2844F52951CC7BA58418D7BC29` |
| [Hahn, “A Conceptual Design of a Short Takeoff and Landing Regional Jet Airliner”](../../bib/performance-airport/nasa_stol_regional_jet_concept.pdf), NASA/AIAA 2010-1011, Jan. 2010, 9 pp. | [NASA NTRS PDF](https://ntrs.nasa.gov/api/citations/20100003051/downloads/20100003051.pdf) | NASA-hosted paper; AIAA publication rights may apply. Local copy is for research and citation. | `4EEA85BC6624D96860208D9742011DA825B0380E5896D80ACB70298E0814FE5F` |
| [Abraham and Denham, “Flight Dynamics Modeling for a Rapid Conceptual Development Environment”](../../bib/performance-airport/nasa_rapid_cde_flight_dynamics.pdf), NASA, 2024, 13 pp. | [NASA NTRS PDF](https://ntrs.nasa.gov/api/citations/20240007153/downloads/Flight_Dynamics_Modeling_for_RapCDE_06122024.pdf?attachment=true) | NASA-hosted report/paper; public access with any third-party publication terms preserved. | `D8A86308497FCAF0808A6C26682A0AE4614EC3E3FF11E1BDCCCB56B0192BB382` |
| [Martinez Rodriguez, Blaesser and Borer, “Sensitivity Analysis for Takeoff and Landing Distance Parameters for Regional Air Mobility Aircraft”](../../bib/performance-airport/nasa_sensitivity_takeoff_landing_distance.pdf), NASA, 2024, 13 pp. | [NASA NTRS PDF](https://ntrs.nasa.gov/api/citations/20240007499/downloads/Sensitivity_Analysis_for_TOL_Martinez.pdf?attachment=true) | NASA-hosted report/paper; public access with any third-party publication terms preserved. | `EE02C712FB638C9D5E32A90EB8BB6F030AF851C5F385FB997E4D2CE70470AB4C` |
| [Marshall and Schweikhard, “Modeling of Airplane Performance from Flight-Test Results and Validation with an F-104G Airplane”](../../bib/performance-airport/nasa_f104_performance_model_validation.pdf), NASA TN D-7137, Feb. 1973, 31 pp. | [NASA NTRS PDF](https://ntrs.nasa.gov/api/citations/19730007281/downloads/19730007281.pdf) | NASA public technical report. | `9276DDF80851804023C023539CD40A265B0C3384177E03B5D26AA3E90E88D70A` |
| [Berger, “A Comparison of Predictions Obtained from Wind Tunnel Tests and the Results from Cruising Flight (Airbus and Concorde)”](../../bib/performance-airport/agard_cp242_wind_tunnel_flight_comparison.pdf), NASA TM-75238 / AGARD CP 242, Aug. 1979, 71 pp. | [NASA NTRS PDF](https://ntrs.nasa.gov/api/citations/19790022965/downloads/19790022965.pdf) | NASA-hosted unclassified-unlimited translation/report; AGARD, Aerospatiale and cited material rights remain applicable. | `62003C9F640438B36584396CEF0E4492091AA631D7599077E879B57D808E4783` |
| [Dzikus, Terekhov, Gollnick and Hartmann, “Market-driven Derivation of Field Performance Requirements for Conceptual Aircraft Design”](../../bib/performance-airport/dlr_market_field_performance_requirements.pdf), DLR/AIAA 2018-3499, 12 pp., DOI `10.2514/6.2018-3499` | [DLR repository PDF](https://elib.dlr.de/125948/1/6.2018-3499.pdf) | DLR repository copy; paper states © 2018 DLR and AIAA permission. Do not redistribute beyond repository terms. | `6AFAE5AF5F682CC34DDD094F077DFFA21E05CE8E6D44F9384A9EA6147A996E23` |
| [Wöhler, Walther and Grimme, “Design and Eco-Efficiency Assessment of a People Mover Aircraft in Comparison to State-of-the-Art Narrow Body Aircraft”](../../bib/performance-airport/dlr_people_mover_airport_constraints.pdf), DLR/ICAS 2022, 13 pp. | [DLR repository PDF](https://elib.dlr.de/193083/1/ICAS_2022_0304-Woehler_Sebastian.pdf) | DLR repository/conference copy; conference and author rights remain applicable. | `F51EA0B55ACC92F9E2702E82369018AAE580E8CC172E98DB8F014E319BEAE3D6` |
| [Poll and Schumann, “An estimation method for the fuel burn and other performance characteristics of civil transport aircraft; part 3 full flight profile when the trajectory is specified”](../../bib/performance-airport/dlr_poll_schumann_full_flight_profile.pdf), The Aeronautical Journal, 2025, 37 pp., DOI `10.1017/aer.2024.141` | [DLR repository PDF](https://elib.dlr.de/212199/1/Poll_Schumann_Fuel-burn%20performance%20civil-transport-aircraft-part-3-full-flight-profile.pdf) and [DOI](https://doi.org/10.1017/aer.2024.141) | Repository PDF states Open Access and CC BY 4.0; retain attribution and licence notice. | `EE868646AEBD9279EF2582DF436F6B439031DC289BB61AAC93D99BC314FC2F09` |

### 10.3 Open academic and flight-test validation literature

| Local PDF and metadata | Official/source URL | Rights note | SHA-256 |
|---|---|---|---|
| [Sun, Hoekstra and Ellerbroek, “OpenAP: An Open-Source Aircraft Performance Model for Air Transportation Studies and Simulations”](../../bib/performance-airport/openap_performance_model.pdf), Aerospace 7(8), 104, 2020, 25 pp., DOI `10.3390/aerospace7080104` | [TU Delft repository PDF](https://pure.tudelft.nl/ws/portalfiles/portal/82834623/aerospace_07_00104_v2.pdf), [publisher article](https://www.mdpi.com/2226-4310/7/8/104) | Published article is CC BY according to the repository/publisher metadata; preserve attribution and the paper's licence. | `52ABF206A1A29C23A5603D9A95A70078BFF397F1C4D8ECD874FF816EC23207DC` |
| [Sóbester, “Flight-Test Validation of a Takeoff Performance Uncertainty Model”](../../bib/performance-airport/soton_takeoff_uncertainty_validation.pdf), Journal of Aircraft, 2021, 13 pp., DOI `10.2514/1.C036180` | [University of Southampton repository PDF](https://eprints.soton.ac.uk/452175/2/1.c036180_5.pdf) | Institutional author-manuscript copy; AIAA copyright applies. Use for research/citation; do not treat the repository copy as a redistribution licence. | `924A81E768CFBB95A6602B64A65EDF225819E12CA619F8D6F1AFE73EE71594E9` |

## 11. Local corpus manifest and reproducibility note

The SHA-256 values above are intended to make citations reproducible even when
web-hosted PDFs are replaced. To reproduce the checksums from the repository
root with PowerShell:

~~~powershell
Get-ChildItem .\bib\performance-airport\*.pdf |
    Get-FileHash -Algorithm SHA256 |
    Sort-Object Path
~~~

The memo intentionally does not include copied regulatory text beyond short
definitions and requirement summaries. Users implementing a certification
translator should pin the applicable FAA/EASA/ICAO edition, record the source
URL or controlled-document identifier, and obtain the relevant authority or
design-organization approval. The local PDFs are evidence inputs for
conceptual design and traceability, not a claim that ALAS is certifiable.

## Conclusion

The current ALAS performance model is a useful conceptual seed, but airport
compatibility needs to become a typed, case-based evaluation rather than a
collection of scalar multipliers. The most valuable next step is an
evidence-bearing `PerformanceLoadCase`/`AirportCondition` interface that makes
the runway, aircraft, atmosphere, procedure, pavement and reserve assumptions
visible. Once that interface exists, the existing Raymer-style screen can
remain as a fast `Proxy`, while point-mass, validated and eventual
certification-specific translators can coexist without silently changing the
meaning of a requirement.
