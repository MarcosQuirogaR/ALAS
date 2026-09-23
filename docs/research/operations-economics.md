# Operations, airline economics, and maintainability research

Repository: ALAS
Captured: 2026-08-26
Scope: operationally credible conceptual aircraft design, including airline missions and schedules, turnaround and boarding, fleet economics and direct operating cost (DOC), dispatch reliability, maintainability and accessibility, airport compatibility, crew workload, cargo and baggage, and lifecycle cost.

This is a research and integration note, not a certification basis and not a claim that scenario values in the literature are universal airline requirements. The downloaded corpus is preserved under bib/operations-economics/. Every downloaded PDF is listed with source metadata, a rights note, and SHA-256 below.

## Bottom line

1. A conceptual aircraft should be evaluated as an asset in a fleet rotation and route network, not only against one design range, one payload, or one cruise point. NASA’s integrated aircraft/airline study found that design, route assignment, capacity, and revenue-management decisions are coupled; a design that is attractive in an isolated aircraft objective can be poor when assigned across a network.
2. Turnaround is an architecture variable. Boarding, deboarding, cabin servicing, baggage/cargo, refuelling, doors, aisle geometry, galley/lavatory placement, ground equipment, and stand constraints form a partially concurrent critical path. The target should be a distribution of turnaround times and delay propagation, not a single optimistic time.
3. DOC, cash operating cost, acquisition cost, and lifecycle cost are different objectives. Fuel or maximum take-off weight is not a sufficient economic proxy. Crew, maintenance, spares, delays, utilization, financing, depreciation, fleet size, and technology maturity can change the preferred architecture.
4. Reliability and maintainability are operational design properties. Dispatch reliability depends on failure probability, troubleshooting, repair/replace time, spares, and the time available at through-stops versus overnight maintenance. Access geometry, modular line-replaceable units, diagnostics, and support concept must be considered before detailed design.
5. Airport and cargo compatibility must be first-class data. ALAS already models aircraft span, field lengths, airport elevation and ISA deviation, payload layouts, ULDs, cargo doors, loading strategy, and CG. It does not yet model gates, stands, GSE, ACN/PCN, baggage-flow resources, or airport service times; these should be added as an operations adapter rather than hidden inside aerodynamics.
6. Requirements, business preferences, objectives, and diagnostics should remain separate. Physical and certification constraints can reject a candidate. Airline preferences can rank feasible candidates. Unavailable or low-confidence operations data should produce an explicit diagnostic or evaluation-failure residual, never a silent pass.

## 1. Evidence method and interpretation

The evidence set combines NASA technical reports, DLR research and dissertations, an author-posted open preprint, EASA and FAA public guidance, an ICAO public regional working paper and public historical manual copy, and a public EUROCONTROL schedule report. The selection emphasizes documents that can be legally downloaded and preserved. Paid or store-only ICAO and IATA manuals are identified but were not copied.

The literature is strongest on the direction of effects and on the structure of the models:

- NASA FLOPS and its lifecycle-cost extension show how weights, aerodynamics, propulsion, mission performance, and cost can be coupled early.
- NASA’s airline-operations work shows why fleet/network assignment and revenue assumptions belong in an outer evaluation loop.
- DLR work provides detailed cabin, boarding, turnaround, lifecycle, and PHM/maintenance examples.
- EASA CS-25 provides certification-oriented workload and interface baselines; it is not a substitute for a certification program.
- ICAO/IATA guidance describes the multi-organization ground-operation process and ULD/turnaround vocabulary; the detailed manuals are not all openly available.
- EUROCONTROL shows the schedule trade between commercial attractiveness, aircraft utilization, buffers, and robustness.

Reported numeric values are scenario inputs, historical samples, calibration values, or model outputs. They are evidence for model structure, not ALAS defaults. In particular, historical load factors, carry-on ratios, boarding rates, salaries, maintenance rates, fuel prices, and turnaround times must be exposed as assumptions with year, market, aircraft class, and uncertainty.

## 2. State of the art and design implications

### 2.1 Airline mission profiles and schedule use

Traditional conceptual design often defines one design mission and optimizes an aircraft-level metric. Roy et al. (NASA, 2018) instead connect aircraft design to route assignment, capacity, fleet decisions, and revenue management. Their study uses an 11-route network and a simultaneous mixed-integer/nonlinear formulation. The important ALAS lesson is architectural: the physics solver can continue to evaluate a mission, but an operations layer should evaluate a weighted portfolio of scenarios.

An operational scenario should distinguish at least:

- scheduled stage length and great-circle distance;
- block time, including taxi and expected runway/airspace effects;
- payload case: passengers, bags, belly cargo, and optional freight;
- offered seats, expected load factor, cabin classes, and cargo demand;
- cruise and reserve policy;
- turnaround distribution and schedule buffer;
- daily rotations, overnight/base maintenance opportunities, and fleet size;
- route frequency and scenario weight;
- airport pair, elevation, temperature/ISA deviation, runway availability, stand/gate category, and local operating limits.

Hölzel, Schilling, and Langhans (DLR, 2010) model lifecycle economics from acquisition through decommissioning. Their model includes route-network choices, maintenance concepts, fuel-price assumptions, revenue by class and seat load factor, time-dependent costs, and net present value. Their cycle-time formulation includes flight, taxi, runway and turnaround components; ground time makes an aircraft unavailable for the next operation. That is the right abstraction for ALAS: mission physics supplies flight performance, while schedule economics consumes block time, turnaround, maintenance events, and availability.

EUROCONTROL’s 2024 schedule analysis describes the practical trade: schedules balance commercial attractiveness and asset use against delays, and buffers can improve robustness while excessive buffers reduce network efficiency. A design requirement such as “complete the turnaround within 35 minutes at the design station” is different from a business preference such as “maximize rotations per day.” The former can be a hard or soft operational constraint; the latter is an objective evaluated over a schedule.

Architecture consequences:

- Design the mission interface to accept a portfolio, not only a single route.
- Keep physical reserve and performance requirements separate from commercial demand and revenue assumptions.
- Treat block time, turnaround, maintenance windows, and schedule buffers as additive or probabilistic terms with provenance.
- Evaluate both a nominal schedule and adverse scenarios: high passenger load, high carry-on, weather/temperature, late inbound aircraft, technical delay, and reduced GSE availability.
- Preserve scenario-wise results so a weighted average cannot conceal a catastrophic tail case.

### 2.2 Turnaround, boarding, deboarding, and schedule robustness

The public ICAO turnround guidance frames aircraft turnaround as a hazardous, complex, multi-organization process involving pre-arrival planning, stand arrival, passenger disembarkation, catering, baggage offload/onload, servicing, passenger boarding, load control, and departure. It emphasizes common procedures, SMS responsibility, contractor coordination, and environmental/lighting conditions. The guidance is operational process guidance, not a universal aircraft design requirement.

DLR’s Fuchte dissertation and the Fuchte/Nagel/Gollnick short-range study model the cabin and fuselage because boarding/deboarding can dominate the gate critical path. The DLR work identifies interactions between passenger walking, seat selection, hand luggage storage, aisle blocking, doors, cabin layout, and load factor. A representative calibration scenario assumes 45% bulky carry-on; that is a study assumption, not a general airline default. The study also reports that a twin aisle can reduce boarding time substantially at some capacities, while its fuselage, structure, systems, and fuel penalties can reverse the economic result at other capacities and stage lengths.

The DLR 2024 coupled simulation links terminal passenger flow to cabin boarding. It is calibrated against a real European medium airport and treats passenger behavior and process restrictions as coupled system, discrete-event, and agent behavior. Its future work explicitly includes baggage handling and resource management, reinforcing that a passenger-only boarding model is incomplete.

The open 2021 early-design turnaround paper similarly treats turnaround as a dependent-activity critical path. Passenger state-machine inputs include seats, luggage, walking, doors, and interference. Its central design implication is that turnaround depends on the business model and station process, not just on seat count.

For ALAS, a turnaround model should include at least these task families:

| Task family | Typical aircraft/design drivers | Operational coupling |
|---|---|---|
| Passenger deboarding/boarding | seat pitch, aisle count and width, doors, cabin zones, carry-on stowage, passenger mix | gate access, cleaning, cabin crew, late passengers |
| Baggage | hold volume, ULD/bulk choice, door dimensions and locations, belt/loader access | passenger boarding, load control, GSE, transfer bags |
| Cargo/ULD | container positions, floor loading, restraint, door proximity, loading strategy, CG trim | baggage, ramp crew, GSE, load sheet |
| Cabin service | galley/lavatory count and placement, water/waste, catering doors | boarding and cleaning critical path |
| Aircraft service | refuelling, power, air, water, waste, de-icing | safety zones, stand equipment, weather |
| Load control and closeout | payload distribution, CG, final passenger/bag/cargo count | departure readiness and reserve margin |
| Delay recovery | late inbound, technical inspection, spare GSE, crew connection | schedule buffer and cancellation risk |

The first useful ALAS implementation is a deterministic precedence graph with task distributions and a critical-path result. A later implementation can add discrete-event or Monte Carlo boarding behavior. A single turnaround_seconds scalar should be retained only as a derived summary, never as the only evidence.

### 2.3 Fleet economics, DOC, and lifecycle cost

NASA’s 1989 lifecycle-cost work extends conceptual design with research, development, test and evaluation (RDT&E), production, direct operating cost, indirect operating cost, and lifecycle-cost objectives. The study shows that minimum take-off weight, fuel, acquisition cost, DOC, and LCC can select different planforms and engine counts. It also reports that maintenance cost can be more sensitive to engine count than engine size in the historical model. The conclusion is still relevant, with a caveat: the parameters are historical and must be recalibrated for a current fleet.

NASA FLOPS provides a rapid conceptual framework for weights, aerodynamics, engine cycle, propulsion scaling, mission performance, takeoff/landing, noise, and cost. It is useful as a physics/economics bridge, but an airline economics layer must add operator-specific assumptions that a generic synthesis code cannot know.

DLR’s lifecycle model gives a useful cost taxonomy:

- fuel and energy;
- flight and cabin crew;
- maintenance labor and material;
- landing, navigation, airport, and handling charges;
- spares and inventory;
- depreciation, financing/interest, and insurance;
- delay, cancellation, passenger compensation, and disruption costs;
- acquisition, modification, and end-of-life costs;
- revenue, load factor, fare/cargo yield, and time value of money.

Terminology varies by airline and source. ALAS should therefore carry an explicit cost_basis rather than assume that DOC, cash operating cost, and total operating cost are interchangeable. A transparent first-order formulation is:

~~~text
cash_operating_cost_per_flight
  = fuel + crew + maintenance + airport/navigation/handling + other cash costs

ownership_cost_per_flight
  = depreciation + financing/interest + insurance + lease or capital charge

lifecycle_cashflow
  = revenue - cash_operating_cost - ownership_cost
    - disruption_cost - major_maintenance - acquisition/end_of_life_cashflows
~~~

For a scenario set, lifecycle NPV can be represented as:

~~~text
NPV = - acquisition_cashflow
      + sum_over_periods(
          (revenue - cash_cost - ownership_cost
           - delay_cost - major_maintenance + residual_value)
          / (1 + discount_rate)^period
        )
~~~

This is deliberately a bookkeeping boundary, not a claim that one industry definition is universal. Every number needs currency, price year, escalation basis, utilization basis, and whether it is per flight, block hour, seat-kilometre, available seat-kilometre, passenger, or aircraft-year.

Design implications:

- Optimize physics feasibility first, then evaluate economic objectives among feasible candidates.
- Include fleet commonality, engine count, maintenance concept, turnaround, and schedule robustness as economic drivers.
- Report a cost breakdown and sensitivity, not only a single DOC score.
- Expose economic uncertainty: fuel price, labor cost, maintenance rate, acquisition price, interest, utilization, load factor, and disruption cost.
- Keep revenue assumptions optional. A design can be operationally credible without asserting a particular fare or airline business model.

### 2.4 Dispatch reliability and operational availability

The NASA dispatch-reliability report defines dispatch reliability as the probability that an aircraft can be dispatched without a delay greater than 15 minutes. It distinguishes a component-failure probability method from an airline-service-experience method. The component method cannot fully represent troubleshooting, repair and replacement time, or the different time available at a through-stop, a turnaround, and an overnight maintenance base. The report concludes that airline experience is needed for accurate prediction.

This distinction matters for early ALAS work:

- a component reliability estimate is a model input or diagnostic;
- dispatch reliability is a fleet/operation result that also consumes maintenance and schedule data;
- the same failure rate can have different schedule consequences depending on access, spares, repair time, and station support;
- “no technical delay” is not a defensible default when a subsystem is not modeled.

Candidate diagnostics should include dispatch probability, technical-delay probability, mean technical delay minutes, cancellation or aircraft-substitution probability, mean time between unscheduled removals (MTBUR), mean time to repair (MTTR), no-fault-found rate, and recovery-station coverage. Results should identify whether they are physics-based, empirical, or placeholder.

### 2.5 Maintainability, accessibility, and support concept

NASA’s maintainability work defines supportability as the combination of reliability, maintainability, logistics, operations, and safety. It emphasizes that a system can fail operationally even when failure rates are low if faults cannot be diagnosed quickly, spares are unavailable, repair lead times are long, or components are difficult to access, install, test, and close up. The paper recommends defining a maintenance concept early, including mission profile, availability, diagnostics, repair-versus-replace decisions, tools, test equipment, and training.

NASA decomposes corrective maintenance time into:

~~~text
T_corrective = DI + DL + GA + RR + SR + CK + CU
DI diagnosis; DL local delivery; GA gain access; RR remove/replace;
SR restore; CK checkout; CU close-up.
~~~

These terms provide a useful typed decomposition for future design trades. A concept with excellent component reliability can still have poor availability if gain access, remove/replace, checkout, close-up, or spares delivery dominates.

Relevant architecture variables include:

- LRU/ORU modularity and connectivity;
- line-of-sight and physical access panels;
- standard fasteners and connectors;
- separation of hot, wet, dirty, high-energy, and sensitive equipment;
- safe access at the gate without major disassembly;
- built-in test and fault isolation;
- inspection intervals and inspection access;
- commonality across fleet variants;
- ground support equipment, tooling, technician skill, and spares strategy;
- ability to complete corrective work during scheduled ground time.

DLR’s 2025 review of condition-based maintenance (CBM) and prognostics and health management (PHM) identifies a current state-of-practice gap: aviation has a strong safety and certification framework, but approval pathways for certifiable CBM solutions remain incomplete and adoption is slower than the technical promise. ALAS should model CBM as a scenario or technology option with regulatory maturity and evidence status, not assume that a predicted failure can automatically defer an inspection.

### 2.6 Airport, stand, and gate compatibility

ICAO Annex 14, the ICAO aerodrome design manuals, FAA AC 150/5300-13B, and airport-specific standards cover geometric and operational compatibility. The current FAA advisory circular organizes airport design around aircraft design groups using dimensions such as wingspan and tail height; this is useful for an early geometry screen, but the FAA groups are jurisdiction-specific and do not replace airport/operator approval.

An early airport compatibility screen should distinguish:

- runway takeoff and landing distances, including TODA/LDA and elevation/temperature;
- approach speed and aircraft category;
- wingspan, tail height, wheelbase, gear track, and turning radius;
- runway/taxiway/apron separation and stand clearance;
- bridge, stairs, belt-loader, catering, refuelling, power, air, water, and waste interfaces;
- gate/stand type: contact, remote, pier, bus, or constrained regional stand;
- pavement compatibility, including ACN/PCN or the applicable local method;
- rescue/fire category and airport operational limitations;
- noise, emissions, local air-quality, curfew, and slot constraints;
- towbar/tug, GPU, de-icing, and other GSE compatibility.

The current ALAS airport model contains name, ICAO code, elevation, TODA, LDA, ISA deviation, notes, and coordinates. That is enough for a first field-performance and route context, but not for a gate/stand or ramp-operability claim. The future model should make missing airport-operational data visible.

Airport compatibility is usually a design requirement when a market or airport set is named. It is a business preference when an airline chooses to limit the fleet to a particular airport code or gate family for commonality. A generic concept should report both the physical margin and the data-coverage status.

### 2.7 Flight-deck and cabin-crew workload

EASA CS-25 provides an appropriate early boundary:

- CS 25.771 requires the pilot compartment and its arrangement to permit the minimum flight crew to perform duties without unreasonable concentration or fatigue.
- CS 25.1302 requires controls, systems, and information to be accessible and usable in relation to urgency, frequency, and duration, with predictable behavior and error management.
- CS 25.1523 requires workload assessment for the minimum crew, with evidence from analysis, simulation, or aircraft evaluation as appropriate.

At conceptual level ALAS should not claim compliance. It can, however, report proxies such as number of simultaneous abnormal tasks, control reach/accessibility flags, alert burden, required manual actions during turnaround, cabin-crew station placement, door/galley/lavatory workload, and ground-crew task concurrency. A later human-factors study can replace the proxies.

### 2.8 Cargo, baggage, ULDs, and loading operations

Public IATA descriptions identify IGOM as covering passenger handling, baggage, aircraft servicing, turnaround, load control, and airside safety. IATA’s ULD Regulations describe container and pallet/net identification, aircraft acceptance, handling, continuing airworthiness, repair, and lifecycle controls. The detailed manuals are paid, so this note uses their public descriptions and does not treat them as reproduced requirements.

Operational cargo/baggage modeling should distinguish:

- passenger carry-on and checked baggage;
- transfer and local baggage;
- bulk lower hold versus containerized/ULD loading;
- ULD type, tare, fill, floor position, restraint, and aircraft acceptance;
- cargo door dimensions, sill height, door location, and loader approach;
- loading order and unloading order;
- CG target, trim moves, and load-control closeout;
- baggage sortation, belt-loader, container loader, and ramp-crew availability.

ALAS already has strong seams for this work. PayloadLayout contains passenger/cargo summaries, belly cargo, hold capacity, ULDs, doors, CG, aisle/deck data, and utilization. CargoDeckConfig already has main/lower-deck ULD types, loading strategy, door positions, target CG, and trim controls. The operations layer should consume these facts and estimate task time, compatibility, and disruption exposure rather than duplicate the payload solver.

### 2.9 Lifecycle, PHM, and maintenance economics

The DLR lifecycle model is particularly useful because it connects route and schedule utilization, flight cycles and flight hours, scheduled maintenance intervals, line/base/engine maintenance, unscheduled events using MTBUR and MTTR, ground time and aircraft availability, spares inventory and repair turnaround, passenger/revenue effects, and NPV.

For early ALAS design, a life-cycle model can start with a transparent event calendar and then mature into a discrete-event fleet simulation. The event calendar should include flights, turns, overnight windows, scheduled inspections, major maintenance, unscheduled removals, spare availability, and end-of-life. It should report aircraft utilization and availability alongside cost; otherwise a lower cost may simply reflect an unrealistic amount of flying.

## 3. Requirements, preferences, objectives, and diagnostics

The existing ALAS requirements-first document already distinguishes hard, soft, objective, and diagnostic policies. The operations extension should preserve that separation and add source confidence and coverage.

| Concern | Recommended policy | What it means in ALAS |
|---|---|---|
| Range, payload, takeoff/landing, climb, speed, CG, static margin | Hard or soft physical requirement | Candidate is infeasible or penalized when the stated design brief requires it. |
| Wingspan, gear geometry, runway/stand/pavement compatibility | Hard when airport set is contractual; otherwise soft or diagnostic | Compare against named airport constraints; do not infer gate compatibility from wingspan alone. |
| Minimum seat count, cargo/ULD capability, door or deck arrangement | Hard when required by the brief; otherwise soft | Architecture and payload-layout constraints. |
| Maximum turnaround at a named station | Hard or soft operational requirement | Use a percentile, such as P90, plus task/assumption coverage. |
| Dispatch reliability or technical-delay ceiling | Hard only when an evidence-backed target is supplied | Requires fleet/support assumptions; otherwise report a diagnostic. |
| Access time, MTTR, inspection interval, spares wait | Soft design requirement or diagnostic in early concept | Drives architecture and lifecycle cost; does not become a pass without a maintenance concept. |
| DOC, cash cost, CASM, cost per block hour, lifecycle NPV | Objective | Rank feasible designs; show cost basis and sensitivity. |
| Fleet utilization, rotations/day, schedule robustness | Objective or soft preference | Depends on network, buffers, crew, and maintenance assumptions. |
| Load factor, fare/yield, cargo revenue, market share | Business preference/analysis assumption | Do not turn a commercial forecast into a universal aircraft requirement. |
| Cabin comfort, boarding style, service level, commonality | Business preference or soft requirement | Can be optimized, but should be named as an operator choice. |
| Crew workload, passenger flow, baggage service, GSE availability | Diagnostic until modeled and validated | Preserve evidence and coverage; use evaluation-failure residuals for missing model inputs. |

Useful rule: a candidate can be physically feasible but operationally unproven. The result should say feasible_with_ops_gap or equivalent rather than silently treating missing operations evidence as feasible.

## 4. Proposed typed operations/economics layer

The following is an interface proposal, not a source-code change in this research task. All values should use SI units. Cost values should carry currency code, price year, escalation basis, and cost basis. Probabilities and percentiles should preserve sample count or model source.

~~~rust
pub struct OperationsProfile {
    pub currency_code: String,
    pub currency_year: u16,
    pub cost_basis: CostBasis,
    pub scenarios: Vec<OperationalScenario>,
    pub fleet: FleetAssumptions,
    pub reliability: ReliabilityAssumptions,
    pub maintenance: MaintenanceAssumptions,
    pub airport_data_policy: DataCoveragePolicy,
}

pub struct OperationalScenario {
    pub id: String,
    pub origin_icao: String,
    pub destination_icao: String,
    pub route_distance_m: f64,
    pub payload_case: PayloadCase,
    pub seats_offered: u16,
    pub passenger_load_factor: f64,
    pub belly_cargo_kg: f64,
    pub frequency_weight: f64,
    pub schedule: ScheduleAssumptions,
    pub airport_ops: AirportOperationsInput,
}

pub struct TurnaroundModelInput {
    pub boarding_model: BoardingModel,
    pub passenger_count: u16,
    pub seat_map: SeatMapReference,
    pub doors: Vec<DoorReference>,
    pub carry_on_distribution: Distribution,
    pub baggage_and_cargo: BaggageFlowInput,
    pub service_tasks: Vec<GroundTask>,
    pub gse_and_stand: StandResources,
}

pub struct TurnaroundResult {
    pub block_time_s: f64,
    pub critical_path_s: f64,
    pub turnaround_p50_s: f64,
    pub turnaround_p90_s: f64,
    pub task_times: Vec<TaskTiming>,
    pub delay_sensitivity: Vec<DelaySensitivity>,
    pub evidence_status: EvidenceStatus,
}

pub struct DirectOperatingCost {
    pub fuel_cost: Money,
    pub crew_cost: Money,
    pub maintenance_cost: Money,
    pub airport_navigation_handling_cost: Money,
    pub other_cash_cost: Money,
    pub ownership_cost: Money,
    pub disruption_cost: Money,
    pub basis: CostBasis,
}

pub struct ReliabilityResult {
    pub dispatch_probability: Probability,
    pub technical_delay_probability: Probability,
    pub expected_technical_delay_s: f64,
    pub cancellation_probability: Probability,
    pub mtbur_cycles: Option<f64>,
    pub mttr_s: Option<f64>,
    pub no_fault_found_rate: Option<f64>,
    pub evidence_status: EvidenceStatus,
}

pub struct MaintainabilityResult {
    pub diagnosis_s: f64,
    pub local_delivery_s: f64,
    pub gain_access_s: f64,
    pub remove_replace_s: f64,
    pub restore_s: f64,
    pub checkout_s: f64,
    pub close_up_s: f64,
    pub spares_wait_s: f64,
    pub scheduled_inspection_s: f64,
    pub evidence_status: EvidenceStatus,
}

pub struct AirportCompatibilityResult {
    pub runway_margins: Vec<ConstraintMargin>,
    pub wingspan_margin_m: Option<f64>,
    pub stand_compatibility: CompatibilityStatus,
    pub gse_compatibility: CompatibilityStatus,
    pub pavement_compatibility: CompatibilityStatus,
    pub missing_inputs: Vec<String>,
}

pub struct OperationsEconomicsResult {
    pub per_scenario: Vec<ScenarioOperationsResult>,
    pub aggregate: AggregateOperationsResult,
    pub residuals: Vec<ConstraintResidual>,
    pub assumptions: AssumptionManifest,
    pub evidence_status: EvidenceStatus,
}
~~~

The exact Rust names can follow ALAS conventions. The important properties are typed scenario identity, explicit data coverage, distributional results, and immutable provenance from the design candidate and configuration.

### 4.1 Objective candidates

Potential objective identifiers include:

- ops.doc_per_flight;
- ops.doc_per_block_hour;
- ops.doc_per_ask;
- ops.cash_operating_cost;
- ops.lifecycle_npv;
- ops.revenue_margin when a revenue model is intentionally enabled;
- ops.turnaround_p90;
- ops.schedule_robustness;
- ops.aircraft_utilization;
- ops.dispatch_unavailability;
- ops.maintenance_cost;
- ops.fuel_burn_over_scenario_set.

These should be evaluated only after hard physical and stated operational requirements pass. Multi-objective ranking should retain the breakdown so a lower DOC cannot hide a worse delay or maintenance assumption.

### 4.2 Diagnostic and residual candidates

The following names provide stable reporting and optimizer seams:

| Identifier | Actual and direction | Default status |
|---|---|---|
| ops.design_range | achieved range minus target; lower bound | physical requirement |
| ops.block_time | actual minus maximum; upper bound | scenario diagnostic or soft |
| ops.turnaround_mean | actual minus target; upper bound | diagnostic/soft |
| ops.turnaround_p90 | P90 minus target; upper bound | soft or hard for named station |
| ops.schedule_buffer | available buffer minus minimum; lower bound | soft or hard |
| ops.dispatch_reliability | target minus achieved probability; lower bound | hard only with evidence |
| ops.technical_delay_minutes | actual minus maximum; upper bound | soft/diagnostic |
| ops.cancel_probability | actual minus maximum; upper bound | soft/diagnostic |
| ops.maint.mttr | actual minus target; upper bound | soft/diagnostic |
| ops.maint.access_time | actual minus target; upper bound | soft/diagnostic |
| ops.maint.spares_wait | actual minus target; upper bound | diagnostic |
| ops.airport.toda_margin | actual minus required; lower bound | physical/airport |
| ops.airport.lda_margin | actual minus required; lower bound | physical/airport |
| ops.airport.span_margin | allowed minus actual; lower bound | airport |
| ops.airport.stand_compatibility | boolean/graded compatibility | hard or diagnostic |
| ops.airport.pavement_compatibility | ACN/PCN or local result | hard or diagnostic |
| ops.payload.uld_compatibility | accepted ULDs/required ULDs | hard or diagnostic |
| ops.payload.baggage_access | service-time or access margin | soft/diagnostic |
| ops.crew.workload | workload index minus limit; upper bound | diagnostic until validated |
| ops.lifecycle.npv | objective value, lower or higher by study | objective |
| ops.data_coverage | supplied evidence/model coverage | diagnostic |

Each residual should carry actual, target, direction, scale, severity, hard/soft/diagnostic policy, scenario ID, and evidence status. An absent model or missing airport input should be an EvaluationFailure or explicit unknown state, not a zero violation.

### 4.3 Scenario aggregation

Use a weighted scenario mean for expected economics and retain worst-case/tail metrics:

~~~text
expected_metric = sum(weight_i * metric_i)
tail_metric = percentile(metric_i, 90 or 95)
worst_hard_violation = max(normalized_hard_violation_i)
~~~

The weights should sum to one and be traceable to the mission/network definition. For reliability and disruption, a fleet simulation may be required; an arithmetic mean of route-level dispatch percentages is not necessarily a fleet dispatch result.

## 5. ALAS integration points

The existing requirements-first document already provides the policy language and design-stage mapping needed for this extension. The operations layer should be additive and should consume existing discipline results.

| ALAS area | Existing seam | Proposed integration |
|---|---|---|
| alas-config / design brief | mission, accommodation, performance/airport limits and hard/soft/objective/diagnostic policy | Add an optional operations profile: scenario set, station/airport constraints, turnaround target, reliability policy, cost basis, fleet and schedule assumptions. Freeze it with the design brief. |
| alas-mission | MissionRequest carries route, elevations, ISA deviation, distance, and mission profile | Preserve physical mission calculations. Add a wrapper result for block time, reserve policy, route scenario ID, and operational ground-time assumptions. Do not put fare or schedule preferences in the physics request. |
| alas-payload | PayloadLayout, passenger/cargo summaries, ULDs, hold capacity, doors, CG, aisle/deck utilization | Feed seat map, passenger count, carry-on/baggage assumptions, ULD compatibility, door proximity, loading strategy, and CG trim into turnaround and cargo task models. |
| alas-config::airports | ICAO, elevation, TODA, LDA, ISA deviation, coordinates | Use as the physical airport baseline. Add a derived or optional airport-operations profile for stand, gate, GSE, pavement, fire category, curfew, and service-resource data. |
| alas-pipeline | candidate materialization, geometry, payload, mass/CG, aero, mission, feasibility, reports | Evaluate operations/economics after cheap geometry/payload/mission results are available. Add a typed OperationsEconomicsResult to the future pipeline result without making physics stages depend on airline revenue. |
| alas-opt | ObjectiveAssessment, ConstraintResidual, FeasibilityScore, feasibility-first ranking | Convert named operational limits to residuals. Aggregate soft/objective values over scenarios; retain per-scenario evidence and use EvaluationFailure for unsupported coverage. |
| alas-report | required stages and mission/payload/flight figures | Add operations figures and a run manifest containing assumptions, source IDs, model version, scenario weights, and coverage. |
| alas-viz | existing scene/figure infrastructure | Visualize critical-path tasks, schedule utilization, DOC/LCC breakdown, reliability/MTTR, access points, airport margins, and baggage/ULD flow. |
| alas-gui | wizard and preview/report boundary | Expose operational requirements as an optional scenario page; show whether a value is a hard requirement, preference, or diagnostic. |

Recommended pipeline boundary:

~~~text
design brief + operations profile
    -> candidate geometry / payload / mass / aero / mission
    -> airport physical screen
    -> turnaround / baggage / reliability / maintainability proxies
    -> DOC / lifecycle / schedule aggregation
    -> feasibility residuals + objective assessment
    -> report and visualization
~~~

The operations evaluator should accept frozen candidate outputs and return a pure result with an assumption manifest. This preserves reproducibility and keeps the current discipline solvers usable when the optional model is disabled.

### 5.1 Reporting and evidence

Suggested additional report/figure identifiers:

- ops.turnaround_critical_path;
- ops.turnaround_distribution;
- ops.schedule_utilization;
- ops.doc_breakdown;
- ops.lifecycle_cashflow;
- ops.dispatch_reliability;
- ops.maintainability_access;
- ops.airport_compatibility;
- ops.payload_baggage_flow;
- ops.crew_workload;
- ops.assumption_sensitivity.

Every figure should state scenario, percentile or aggregation, units, price year, evidence status, and whether values are modeled, calibrated, or placeholder. A report should be able to answer “why did this design lose?” with a named residual such as ops.turnaround_p90 or ops.maint.spares_wait, not only a composite score.

## 6. Recommended implementation sequence

1. Define a versioned operations schema and assumption/provenance manifest. Include cost basis, scenario weights, data coverage, and policy for unknown values.
2. Add a deterministic ground-task graph. Consume existing payload doors, ULDs, cabin layout, CG, airport identity, and service assumptions. Return critical-path timing and resource conflicts.
3. Add a lightweight boarding/deboarding distribution model. Keep study calibration values explicit and support P50/P90 outputs.
4. Add DOC and lifecycle cashflow. Start with fuel, crew, maintenance, airport/navigation/handling, ownership, disruption, and acquisition/end-of-life categories; add sensitivity analysis before optimization.
5. Add reliability and maintainability proxies. Use MTBUR/MTTR, the NASA access-time decomposition, scheduled ground windows, spares wait, and an evidence status.
6. Add airport stand/GSE/ULD/pavement adapters. Keep physical runway margins separate from service compatibility.
7. Add report and visualization outputs with scenario-wise evidence.
8. Only then connect operational residuals and objectives to optimization. Hard requirements should block; soft requirements should penalize; unsupported analysis should remain visible as an evaluation gap.

## 7. Limitations and non-requirements

- Airline maintenance, labor, delay, spares, and route-demand data are often confidential. Public papers demonstrate methods, not a universal cost table.
- Historical load factors, carry-on rates, boarding rates, and fleet costs must not be copied into a current baseline without a date and sensitivity range.
- IATA IGOM, IATA ULD Regulations, ICAO Annex 14, and related airport manuals are authoritative or widely used but are not all freely downloadable. This note records their public descriptions and does not reproduce paid content.
- The public ICAO Doc 9889 PDF preserved here is visibly a second-edition/historical copy. ICAO currently advertises a third edition; use the downloaded file as background, not as the current regulatory text.
- The downloaded EASA CS-25 document is Amendment 27, a free consolidated/easy-access publication. EASA’s current official certification-specification page has Amendment 28; the downloaded copy is date-labeled for reproducibility and should not be treated as current official text.
- FAA AC 150/5300-13B is U.S. advisory guidance. The dimensions and aircraft design groups are not automatically legal requirements at every international airport.
- Early crew-workload, dispatch, CBM, pavement, gate, and GSE results are diagnostics unless the design brief supplies an applicable standard, operator requirement, and evidence method.
- No source file under crates/ was changed for this note. Implementation suggestions are intentionally kept at the interface level.

## 8. Downloaded source corpus

All files below are in bib/operations-economics/. SHA-256 values were calculated from the final downloaded bytes. The rights notes describe why the PDF was considered legal to preserve; they do not grant additional rights beyond the source terms.

| ID and citation | Evidence used | Downloaded PDF | Landing/source URL | Rights and use note | SHA-256 |
|---|---|---|---|---|---|
| NASA 1989, V. S. Johnson, Optimizing conceptual aircraft designs for minimum life cycle cost, NTRS 19890015840 | LCC objective structure; RDT&E/production/DOC/IOC; planform and engine-count trade; economic sensitivity; pp. 2–6, 8, 11, 16, 22 | nasa_1989_optimizing_conceptual_aircraft_designs_lcc.pdf | [NTRS record](https://ntrs.nasa.gov/citations/19890015840) · [official PDF API](https://ntrs.nasa.gov/api/citations/19890015840/downloads/19890015840.pdf) | NASA NTRS public-use government report; preserve attribution; not a current cost baseline | 1e91f52d51106a52410ea5e2b36bd8d1033e642bb9d7c48dc62a0e3e9b44e870 |
| NASA 1984, Dispatch reliability section, NTRS 19840020733 | Definition as dispatch without delay over 15 minutes; component versus airline-experience method; repair/troubleshooting/through-stop limitations | nasa_1984_dispatch_reliability.pdf | [NTRS record](https://ntrs.nasa.gov/citations/19840020733) · [official PDF API](https://ntrs.nasa.gov/api/citations/19840020733/downloads/19840020733.pdf) | NASA NTRS public-use government report; section extracted from a larger technical report; not a certification target | 553bc2dbc6298e043d4a1e610f478088fd764ce8d71bace4b4354b896e9b1f5d |
| NASA 1997, Lalli and Packard, Designing for Maintainability and System Availability, NASA/TM-107398 | Supportability definition; maintenance concept; modular LRU/ORU; corrective-time decomposition; logistics and availability; pp. 3–12 | nasa_1997_designing_for_maintainability_and_system_availability.pdf | [NTRS record](https://ntrs.nasa.gov/citations/19970013665) · [official PDF API](https://ntrs.nasa.gov/api/citations/19970013665/downloads/19970013665.pdf) | NASA NTRS public-use government report; general system guidance, not aircraft-specific certification material | f8c66ad8d7e48f657677e5fc3aa6e2c9a0bf565c8bc990581cf612fc70897a86 |
| NASA 2017, Wells, Horvath, and McCullers, The Flight Optimization System (FLOPS) User’s Guide, NASA/TM-2017-219627 | Rapid conceptual synthesis: weights, aero, propulsion, mission, field performance, noise, and cost | nasa_2017_flops_weights_estimation_method.pdf | [NTRS record](https://ntrs.nasa.gov/citations/20170005851) · [official PDF API](https://ntrs.nasa.gov/api/citations/20170005851/downloads/20170005851.pdf) | NASA NTRS public-use government report; use with ALAS model validation and attribution | 819a48fc9c8f34f14595d93f3e3d54dc8454298e83e64048c14ac7bda00bb51d |
| NASA 2018, Roy et al., Next Generation Aircraft Design Considering Airline Operations and Economics, AIAA 2018-1647 | Coupled aircraft design, airline operation, route assignment, fleet, and revenue-management loop; 11-route example | nasa_2018_next_generation_aircraft_design_airline_operations_economics.pdf | [NTRS record](https://ntrs.nasa.gov/citations/20180004468) · [official PDF API](https://ntrs.nasa.gov/api/citations/20180004468/downloads/20180004468.pdf) | NASA-hosted conference paper; public NTRS copy; preserve author/AIAA attribution; scenario method, not operator data | bf73d7898d10f5fd5ba72a9bcacb0faa57579fc0c4734770158c0767712c2594 |
| DLR 2010, Hölzel, Schilling, and Langhans, Aircraft Lifecycle Cost-Benefit Analysis of PHM Systems | Route network, schedule cycle time, maintenance intervals, MTBUR/MTTR, spares, availability, delay/revenue, NPV; pp. 2–7, 14 | dlr_2010_aircraft_lifecycle_cost_benefit_phm.pdf | [DLR repository record](https://elib.dlr.de/68866/) · [PDF](https://elib.dlr.de/68866/1/hoelzel_schilling_aircraft_lifecycle_cost_benefit_analysis_of_phm_systems_imapp_2010.pdf) | DLR repository marks the item Open Access; retain DLR/author attribution and repository terms | 748248cb6cfb5c3dbb6c09d88c24b6418630df3d40d35fe60f957f61ba222470 |
| DLR 2012, Fuchte, Nagel, and Gollnick, Design of a Short Range Twin Aisle Aircraft considering Boarding and Turnaround | Boarding/turnaround critical path, cabin layout, luggage interference, carry-on scenario, twin/single aisle trade, task concurrency; pp. 1–8 | dlr_2012_twin_aisle_short_range_economics.pdf | [DLR PDF](https://elib.dlr.de/77238/1/ATIO2012_Fuchte.pdf) | Public DLR repository paper; preserve attribution; model calibration values are historical study assumptions | abdb5b0561fe4dca70c4c0dd5faeba597c80acb59619b3e775ca3656061170e0 |
| DLR 2014, Jörg Clemens Fuchte, Enhancement of Aircraft Cabin Design (DLR-FB-2014-17 dissertation) | Cabin/fuselage layout, boarding simulation, short-range schedule and DOC implications, ground handling; abstract and cabin/turnaround chapters | dlr_2014_fuchte_cabin_turnaround_short_range.pdf | [DLR repository record](https://elib.dlr.de/89599/) · [PDF](https://elib.dlr.de/89599/1/Fuchte%20FB-2014-17%20Version%20Druck.pdf) | DLR repository marks Open Access and peer reviewed; dissertation/repository attribution required | a5170e942c25bc7f4b4b0bff29667bb2acdb4e2721496549e33b2deb9ab8fe3f |
| DLR 2024, Jung, Claßen, and Rudolph, Coupled Passenger and Aircraft Turnaround Simulation | Coupled terminal passenger-flow and cabin boarding model; calibrated European medium-airport case; future baggage/resource coupling | dlr_2024_coupled_passenger_turnaround_simulation.pdf | [DLR repository record](https://elib.dlr.de/207985/) · [PDF](https://elib.dlr.de/207985/1/978-1-964867-34-2_31.pdf) | DLR repository marks Open Access and peer reviewed; preserve author/DLR/DOI attribution | 38487707a5dd9950bf19e7ad5ad59f71b85c6fdc4d32ae7d8f91ae5b47f68277 |
| DLR 2025, Meissner et al., Regulatory Pathways to Certifiable CBM Solutions in Aviation: A Comprehensive Review | Current CBM/PHM state, certification/regulatory barriers, scheduled inspection downtime, adoption caveat | dlr_2025_regulatory_pathways_condition_based_maintenance.pdf | [DLR repository record](https://elib.dlr.de/217696/) · [PDF](https://elib.dlr.de/217696/1/Meissner_2025_RegulatoryPathwaysToCertifiableCBMSolutionsInAviationAComprehensiveReview.pdf) | DLR repository hosts the published version; cite DOI 10.1016/j.paerosci.2025.101143 and repository terms; do not assume a CC license from the landing page alone | d381e607be75a00f6f111753d5e8162e82167d72f1919506e7bb98bf5f706c9d |
| Picchi Scardaoni et al. 2021, Aircraft Turnaround: An Early Design Tool | Turnaround as marketability/value driver; critical path; passenger FSM; seats/luggage/walking/door/interference parameters | arxiv_2108_01015_aircraft_turnaround_early_design.pdf | [arXiv record](https://arxiv.org/abs/2108.01015) · [accepted PDF](https://arxiv.org/pdf/2108.01015) | Author-posted accepted manuscript/preprint; use under arXiv and publisher terms, preserve authors and DOI/publication attribution; not assumed CC | 3d09d9d62b2463c4bfd88886286c1b87fcbca60cb77df010bb9da75c5a9db35d |
| EASA, Easy Access Rules for Large Aeroplanes (CS-25), Amendment 27 | CS 25.771 pilot workload/fatigue; CS 25.1302 controls and information; CS 25.1523 minimum-crew workload assessment | easa_cs25_easy_access_rules_amendment27.pdf | [EASA free publication page](https://www.easa.europa.eu/en/document-library/easy-access-rules/easy-access-rules-large-aeroplanes-cs-25) · [PDF download](https://www.easa.europa.eu/en/downloads/136694/en) | EASA free consolidated/easy-access rules; page states it is not the official publication; current Amendment 28 may differ; cite EASA and amendment | 76b28a91ee2a24ef5eea3a72f26fc6d95c9758d9c9342a8070e27b12fd9e28c6 |
| FAA, AC 150/5300-13B Airport Design, Change 1 with errata | Runway/taxiway/apron geometric design; aircraft design groups; early airport compatibility screen | faa_ac150_5300_13b_airport_design_chg1_errata.pdf | [FAA current AC page](https://www.faa.gov/airports/resources/advisory_circulars/index.cfm/go/document.current/documentnumber/150_530) · [official PDF](https://www.faa.gov/documentLibrary/media/Advisory_Circular/AC-150-5300-13B-Airport-Design-Chg1-w-errata.pdf) | Public U.S. government advisory circular; attribution; jurisdiction-specific advisory guidance, not a universal airport requirement | 09bb2c0c01fb29022ca48fdfa62bc5190b692e42ab9e2e41d190d8bb9942312c |
| ICAO public regional working paper, Model Guidance Chapter 4: Aircraft Turnround | Common turnround process, SMS, contractor coordination, arrival/stand/passenger/baggage/catering/load-control tasks | icao_turnround_model_guidance_chapter4.pdf | [ICAO MID public PDF](https://www.icao.int/sites/default/files/MID/Documents/MIDANPIRG%2019%20%26%20RASG-MID%209/Working%20Papers/WP26-AGA-AOP-Safety-Matters.pdf) | Publicly posted regional working paper; preserve ICAO attribution; it does not replace paid ICAO/IATA manuals or local operator procedures | cdbf647df225312b2225338b49e391aedc28dd96e700765385c7339bbd8d3cda |
| ICAO, Airport Air Quality Manual (Doc 9889), public historical/second-edition copy | Airport emissions/air-quality context and airport-system framing; used as background only | icao_doc9889_airport_air_quality_manual.pdf | [public ICAO PDF](https://www.icao.int/sites/default/files/2025-04/9889_cons_en.pdf) | Public ICAO copy; embedded title identifies second edition while ICAO now advertises a third edition; historical background, not current regulatory text | 8e5e3d1701ded7bf936938d12d090f06f102b8ba3a45eb6d9cdf49c07360502c |
| EUROCONTROL 2024, European Aviation Trends 06: Schedule Planning and Robustness | Commercial attractiveness versus cost, utilization, delay buffers, first rotation, schedule robustness | eurocontrol_2024_aviation_trends_schedules.pdf | [EUROCONTROL report page/PDF](https://www.eurocontrol.int/publication/european-aviation-trends-06) · [PDF](https://www.eurocontrol.int/sites/default/files/2024-12/eurocontrol-aviation-trends-06.pdf) | Public EUROCONTROL report; preserve agency/source attribution; current trends can change and are not aircraft requirements | 7be34574e40c223dd45c1af5aec40348aa3f97b2ccfca33544a35dd84663458b |

## 9. Public guidance consulted but not downloaded

These sources are relevant, but the detailed document was not copied because it is store-only, paid, restricted, or not needed for the open evidence corpus.

| Source | Publicly available information | Status and ALAS treatment |
|---|---|---|
| [ICAO Annex 14, Aerodromes](https://store.icao.int/en/annex-14-aerodromes) | Physical characteristics, obstacle surfaces, airport code system, and minimum specifications for current/future aircraft | Store-only protected PDF. Use as a standards pointer; airport-specific compliance must be supplied by the user/operator. |
| [IATA Ground Operations Manual (IGOM)](https://www.iata.org/en/publications/manuals/iata-ground-operations-manual/) | Public description covers passenger handling, baggage, servicing, turnaround, load control, and airside safety; standardization reduces training/risk/cost | Manual is paid. Use the public taxonomy and cite the manual when an operator supplies access. |
| [IATA ULD Regulations](https://www.iata.org/en/publications/manuals/uld-regulations/) | Public description covers ULD type/identification, aircraft acceptance, handling, continuing airworthiness, repair, and lifecycle | Manual is paid. ALAS should store ULD type and acceptance evidence rather than reproduce IATA tables. |
| [EASA CS-25 Amendment 28](https://www.easa.europa.eu/en/document-library/certification-specifications/cs-25-amendment-28) | Current official certification-specification page | Current version should be checked for certification work. The preserved open PDF is Amendment 27 for reproducibility. |
| [DLR comparison of DOC and LCC methods](https://elib.dlr.de/113625/) | Notes that DOC varies by airline and that salaries, operating conditions, and maintenance data are often confidential | Marked “final paper, only DLR internal”; not downloaded. Treat airline cost parameters as supplied assumptions. |

## 10. ALAS implementation anchors

The following existing files were inspected as integration anchors; they were not modified for this research note.

- [Requirements-first aircraft design](../REQUIREMENTS_FIRST_AIRCRAFT_DESIGN.md): hard/soft/objective/diagnostic policies, DesignBrief, stage mapping, residual evidence, and reporting boundaries.
- [Mission profile request](../../crates/alas-mission/src/profile.rs): route, elevations, ISA deviation, distance, profile configuration, and physical mission boundary.
- [Airport configuration](../../crates/alas-config/src/airports.rs): ICAO, elevation, TODA/LDA, ISA deviation, coordinates, and current airport data coverage.
- [Payload layout](../../crates/alas-payload/src/layout.rs): passenger/cargo summaries, ULDs, doors, hold capacity, CG, aisle/deck data, and utilization.
- [Cargo deck configuration](../../crates/alas-config/src/cabin/cargo.rs): ULD types, door positions, loading strategy, target CG, and trim controls.
- [Feasibility residuals](../../crates/alas-opt/src/feasibility.rs): hard/soft residuals, evaluation failures, normalized violation, and feasibility-first scoring.
- [Objective evaluation](../../crates/alas-opt/src/evaluator.rs): objective assessment and detailed residual integration.
- [Pipeline stages and result](../../crates/alas-pipeline/src/pipeline.rs): candidate flow, mission/report outputs, and the future location for an additive operations/economics result.
- [Report registry](../../crates/alas-report/src/registry.rs): existing required stages and figure-registration boundary.

The recommended next artifact is a versioned operations-profile schema and a small deterministic evaluator. It should be implemented only after the project owner confirms the desired scenario and cost conventions; this note deliberately does not alter production source files.
