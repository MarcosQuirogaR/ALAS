# Fuel, mass, and mission-model roadmap

## Decision

ALAS should replace its current maximum-available-fuel mission with a coupled
operational fuel-planning solve. Before that solve is trusted, the product
turbofan must be calibrated from the selected engine's sea-level-static rating
instead of treating that rating as a cruise design target.

The target architecture separates four concepts:

1. Aircraft mass capability: OEW/DOM, payload, MZFM, MTOM, MLM, tank capacity.
2. Operational load case: passengers, cargo, distribution, taxi/ramp mass.
3. Mission physics: route, weather, speed/altitude guidance, phase fuel and mass.
4. Fuel policy: taxi, trip, contingency, alternate, final reserve, additional,
   extra, and discretionary fuel under an explicitly selected jurisdiction.

A route completing without crossing zero usable fuel is not reserve-compliant.

## Current-state findings

- Mass buildup assigns fuel as `MTOW - OEW - payload`.
- Fuel loading caps that residual by published or estimated usable capacity.
- Mission analysis burns that maximum available load once; it does not solve
  for the fuel required at the takeoff mass created by that fuel.
- The product path passes installed sea-level-static thrust to a turbofan input
  documented as cruise design thrust. Fuel flow and cruise thrust therefore
  require correction and calibration before operational claims.
- Mission fuel is airborne trip burn only. Taxi/APU, contingency, alternate,
  final reserve, additional, extra, and discretionary fuel are absent.
- Wind, legwise course, en-route weather, CAS/Mach crossover, achievable climb,
  optimum altitude, tank sequencing, fuel-CG motion, and trim drag are absent.
- The payload-range figure uses an independent Breguet model and can disagree
  with the native mission solver.
- Non-converged segments are propagated; aerodynamic surrogate-domain and
  CLmax violations do not govern mission validity; exhaustion is detected only
  at fixed control points.

## Delivery plan

### P0 — Make the existing result physically honest

Scope:

- Calibrate turbofan core flow to reproduce each preset's installed
  sea-level-static rating, then evaluate altitude/Mach lapse. Do not size the
  engine to deliver static rating at cruise.
- Validate every schedule input: finite positive speed/rate, ordered altitudes,
  `|vertical rate| < TAS`, valid cruise fractions, atmosphere domain, and
  route topology.
- Stop at the first non-converged or non-physical segment. Return typed partial
  telemetry and the causal status; never propagate an unconverged state.
- Enforce throttle, angle-of-attack, CLmax/buffet, and aerodynamic-surrogate
  domains by phase.
- Interpolate the exact fuel-exhaustion event within a segment.
- Rename current `block_time_s` to airborne/mission time until ground phases
  exist.
- Separate frozen SUAVE compatibility evidence from product-semantic tests.

Acceptance:

- Every preset reproduces its configured static thrust within a declared
  calibration tolerance and has plausible cruise thrust lapse/TSFC evidence.
- Invalid schedules fail before numerical solve.
- No telemetry after the first invalid segment is labeled flown.
- Mission outputs carry explicit model-validity and convergence status.

### P1 — Introduce typed mass and fuel contracts

Scope:

- Add typed `MassState`, `FuelCapacity`, `FuelLoad`, `FuelPlan`, `FuelPolicy`,
  and constraint/result enums; remove string-keyed fuel lookup at mission
  boundaries.
- Track DOM/OEW, traffic load, ZFM, ramp mass, taxi fuel, TOM, landing mass,
  usable fuel, unusable fuel, and abnormal trapped fuel without double counting.
- Keep kg canonical. Convert volume to mass only at the tank/uplift boundary
  with explicit fuel grade, density, temperature, and provenance.
- Reject negative, NaN, or inconsistent residual fuel before mission setup.
- Consolidate duplicated mass closure and wing-fuel-volume implementations.

Acceptance:

- Exact mass ledger closes at ramp, takeoff, every phase boundary, and landing.
- MTOM, MZFM, MLM, tank-capacity and CG constraints report typed controlling
  limits.
- Unusable fuel belongs to empty mass and cannot be consumed or subtracted twice.
- Property tests cover finite/nonnegative state and conservation invariants.

### P2 — Solve required fuel and takeoff mass together

Scope:

- Implement a bounded outer closure around the mission kernel:
  `fuel_on_board = taxi + trip(TOM) + policy reserves`.
- Start with an EASA basic-policy implementation:
  taxi + trip + contingency + destination-alternate/no-alternate + final reserve
  + additional + extra + discretionary fuel.
- Calculate turbine final reserve at estimated arrival mass: at least 30 minutes
  holding at 450 m/1,500 ft above the applicable aerodrome under standard
  conditions. Keep contingency and final reserve distinct.
- Add alternate/missed-approach/diversion/hold mission branches and taxi/APU
  phases. Protect final reserve as a landing requirement, not nominal trip fuel.
- Add selectable FAA Part 121 policies instead of mixing their rules with EASA.

Acceptance:

- Closure converges monotonically or returns typed infeasibility with the
  controlling mass/tank/policy constraint.
- Re-running the mission at solved TOM reproduces planned trip fuel within
  tolerance.
- Regulatory worked examples and independent OFP cases reproduce each ledger
  component, landing fuel, TOM, and landing mass.

### P3 — Make route and trajectory operationally representative

Scope:

- Replace scalar route distance with legwise geometry, course, route-source
  quality, SID/STAR/airway distances, and a declared fallback distance margin.
- Add wind vectors and weather varying with position, altitude, and time;
  integrate ground speed and reject non-positive along-track ground speed.
- Replace imposed TAS/rate profiles with CAS/Mach crossover schedules,
  acceleration/deceleration, 250 kt restrictions where applicable, thrust-
  limited climb/descent, ceiling and buffet margins, optimum altitude, and
  achievable step climbs.
- Apply departure, en-route, and arrival atmosphere independently.
- Generate payload-range results with the same mission/fuel-policy kernel;
  retain Breguet only as a labeled sanity check.

Acceptance:

- Headwind, payload, temperature, route-factor and alternate-distance
  sensitivities have correct signs and smooth response.
- Mission time and distance close in the ground frame.
- Payload-range and point-mission results cannot use conflicting physics.

### P4 — Couple tanks, CG, trim, and performance

Scope:

- Model tanks, usable/unusable/trapped quantities, moments, feed/transfer order,
  imbalance limits, and fuel-temperature/density effects.
- Update mass, CG, and inertia as fuel burns; check taxi, takeoff, critical tank
  states, and landing envelopes.
- Solve longitudinal trim with tail/elevator and propulsion moments so CG and
  tank strategy affect trim drag and fuel burn.
- Use phase-specific flap/gear aerodynamics and CLmax for takeoff, approach,
  missed approach, and landing.

Acceptance:

- Tank mass and moments conserve exactly.
- Published planning-envelope cases are reproduced only within their documented
  applicability; AFM/WBM-only limits remain explicitly unavailable.
- Trim and fuel-burn changes under forward/aft CG have verified physical signs.

### P5 — Calibration, uncertainty, and advanced operations

Scope:

- Calibrate engine/airframe models against manufacturer or approved engine-deck,
  payload-range, climb, and operational-flight-plan evidence. Report uncertainty
  bands rather than false precision.
- Add discretization studies and adaptive control-point refinement; then improve
  performance using warm starts, cached aero/engine models, and analytic/AD
  Jacobians where profiling supports it.
- Add EDTO/ETOPS critical fuel, isolated-aerodrome PNR, and RCF/redispatch only
  after alternates, route coverage, OEI/depressurization, icing/APU, and MEL/CDL
  penalties exist.
- Add scenario/robust optimization over payload, wind, temperature, routing and
  reserve policy; keep design mission and operational dispatch mission distinct.

Acceptance:

- Validation matrix spans all presets, short/long routes, hot/high fields,
  payload extremes, winds, alternates, and abnormal critical scenarios.
- Numerical refinement changes fuel/time/end mass less than declared engineering
  tolerances.
- Every output states model version, policy, provenance, validity domain, and
  uncertainty.

## Delivery status, 2026-09-06

The typed mass and fuel contracts (P1), the coupled required-fuel closure
(P2) and the tank-local part of P4 are in the normal product pipeline.
Everything below is implementation and numerical verification; no preset
has been validated against flight or operational-flight-plan data.

- **P1, delivered.** `alas-mass::ledger` is the item-level mass statement
  (mass, role, station, centroidal inertia tensor, method) every state is
  computed from; `alas-mass::stations` places every group from the built
  geometry; `alas-mass::statement` produces the operating-empty, zero-fuel,
  takeoff and landing states with full inertia tensors; `alas-mass::tanks`
  resolves the configured tank arrangement on the wing box, distributes fuel
  in burn order and carries unusable fuel in the empty mass. Unusable fuel
  cannot be consumed; negative or non-finite masses are refused rather than
  clamped.
- **P2, delivered for the interactive route.** `alas-config::fuel_policy`
  selects the EASA basic scheme, the FAA domestic and flag/supplemental
  rules or a named study convention; `alas-mass::fuel_policy` prices every
  quantity with the rule that produced it; `alas-mass::dispatch` closes the
  takeoff mass against the required fuel under the takeoff-mass limit and
  the tank capacity. The pipeline's mission stage flies the route at that
  mass (native trip re-flown to convergence, analytic reserves), and the
  feasibility report carries the plan and a `ReserveFuelShortfall` finding
  when the policy fuel does not fit. The frozen maximum-available-fuel case
  remains selectable (`fuel_policy.fly_policy_load_case = false`).
- **P4, tank part delivered.** Fuel sits in its tanks in burn order for
  every state; the centre-of-gravity travel with fuel is reported as a
  curve; the dynamic-mode figure reads the ledger tensor. Trim drag from
  the tank strategy and phase-specific high-lift aerodynamics are not
  implemented.
- **Mission-sized optimization.** `optimizer.objective` selects block fuel,
  takeoff mass, empty mass or fuel per seat-kilometre over a design range
  under the fuel policy, with the takeoff mass closed by an inner fixed
  point and every requirement family declared hard, soft, diagnostic or
  off; a candidate's tank capacity follows its own spar box through the
  preset's per-cell calibration. The legacy lift-to-drag objective remains
  the default.
- **Not done:** P3 (winds, CAS/Mach schedules, legwise routing), the trim
  coupling of P4, and all of P5.

## Technical-debt priorities

1. Correct propulsion scaling and invalid-state propagation before optimization.
2. Replace string/NaN/default-zero domain signaling with typed validated results.
3. Consolidate duplicated fuel-volume and residual-fuel logic.
4. Split compatibility and product semantics behind explicit strategies.
5. Cache trained surrogates across outer fuel iterations.
6. Profile before changing the fixed-grid solver; physics errors dominate today.
7. Use unit newtypes at public boundaries, especially where metres and feet mix.

## Source baseline

- EASA Easy Access Rules for Air Operations, Revision 24, CAT.OP.MPA.180–185
  and associated AMC/GM, March 2026.
- ICAO Annex 6, Part I, 4.3.6 fuel planning; ICAO Doc 9976 as implementation
  guidance where available.
- 14 CFR 121.639, 121.645 and 121.647 for FAA policy variants.
- EASA CS-25.29, CS-25.959 and AFM weight/CG/fuel guidance.
- EASA AMC 20-6 and FAA AC 120-42B for later EDTO/ETOPS work.

The current research report is local at
`.agent/reports/2026-08-29-fuel-mass-mission-investigation.html`.
