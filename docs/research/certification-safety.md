# Certification and safety research for the ALAS conceptual aircraft-design wizard

**Research date:** 2026-08-26
**Repository:** `C:\Proyectos\ALAS-rust`
**Scope:** preliminary certification and system-safety modelling for a serious conceptual aircraft-design wizard
**Status:** research input; not a certification basis, approved means of compliance, compliance finding, or claim that any ALAS design is certified

## Executive conclusion

ALAS should treat certification and safety as a requirements-first architecture and traceability problem, not as a late report section. A concept can be rejected early because its architecture cannot plausibly tolerate a catastrophic single failure, because its operating envelope is undefined, because a required emergency-landing or evacuation geometry is impossible, or because a fire/lightning/icing hazard has no credible design response. The same concept must still remain explicitly **not evaluated**, **inconclusive**, or **evidence pending** when the required analysis, test, authority agreement, or design-assurance evidence does not yet exist.

The recommended wizard contract is therefore:

1. The user selects a certification profile: FAA `14 CFR Part 25`, EASA `CS-25`, another basis, or an explicitly non-certification concept study, together with amendment/issue, aircraft category, seating, propulsion, operating rules, novel technologies, and intended operations.
2. The wizard declares the operational and environmental envelope before sizing. It carries normal, abnormal, emergency, dispatch, maintenance, and ground phases as first-class load cases.
3. A preliminary FHA/PSSA-style model filters impossible architectures. It records functions, failure conditions, severity, assumed exposure, redundancy, independence, common-cause threats, crew action, and dispatch state. It does not label these records an approved FHA, PSSA, or SSA.
4. Every safety result is a typed residual. A physical violation is not the same thing as a missing translator, unavailable probability, unperformed test, or unresolved authority interpretation. Unknown is never silently converted to zero residual or “pass.”
5. The output is a compliance-and-evidence matrix with statuses such as `preliminary_screened`, `evidence_pending`, `diagnostic`, `inconclusive`, and `not_applicable_with_rationale`. “Certified” is not an available wizard status.

This approach is consistent with the current FAA Part 25 system-safety rule and AC 25.1309-1B, current EASA CS-25/AMC material, the systems-safety methods in SAE ARP4761 and development-assurance methods in SAE ARP4754, and the safety/requirements traceability practices in the NASA handbooks. The DLR open literature is especially useful for the design-space insight that safety architecture can filter candidate concepts before expensive physics, while also warning that a heuristic screen is not certification evidence.

## 1. What the wizard is and is not modelling

The certification model is a **preliminary design decision aid**. It should answer questions such as:

- Is the selected certification profile applicable to the proposed aircraft and operation?
- What requirements and failure conditions are relevant to this configuration?
- Can the architecture provide a credible first response to each catastrophic, hazardous, and major failure condition?
- Which loads, residuals, operational limits, and evidence objects must be carried into the next design stage?
- Which results are physical screens, which are evidence still to be produced, and which are diagnostics because ALAS has no validated translator yet?

It should not answer:

- “The aircraft is certified.”
- “The concept complies with Part 25/CS-25” when only a conceptual calculation has run.
- “The probability is acceptable” when the probability model, exposure data, independence assumptions, or authority-accepted method has not been established.
- “A requirement does not apply” merely because its translator is absent.

Part 25/CS-25 is an airworthiness basis. Operational approvals, operator procedures, maintenance programmes, MMEL/MEL assumptions, route/airport approvals, and state-of-design/state-of-operation decisions are related but separate. The wizard must keep those layers separate in the data model.

## 2. Authority and guidance map

### 2.1 Regulatory structure

The current FAA Part 25 structure is visible in the [official eCFR Part 25 page](https://www.ecfr.gov/current/title-14/chapter-I/subchapter-C/part-25) and in the readable [Cornell legal mirror](https://www.law.cornell.edu/cfr/text/14/part-25). Its top-level structure is:

| Part 25 area | Main design meaning | ALAS treatment |
|---|---|---|
| Subpart A, §§25.1–25.5 | Applicability, definitions, certification basis | Require an explicit certification profile and amendment/issue; do not infer applicability from aircraft size alone. |
| Subpart B, §§25.21–25.255 | Flight, handling qualities, performance, flight envelope | Build the normal/abnormal/emergency envelope and performance load cases before ranking concepts. |
| Subpart C, §§25.301–25.581 | Loads, strength, structural design, fatigue/damage, lightning and other structural effects | Generate structural load cases from both normal manoeuvres and failure/emergency events; retain assumptions for later substantiation. |
| Subpart D, §§25.601–25.899 | Design/construction, controls, landing gear, cockpit/cabin, exits, fire protection, materials, bonding | Use geometry, zoning, access, separation, egress, materials and fire screens as architecture constraints. |
| Subpart E, §§25.901–25.1207 | Powerplant, fuel, fire, ignition, ice, exhaust, engine failure and related installations | Tie propulsion architecture, fuel/energy storage, thermal protection, and OEI cases to safety functions. |
| Subpart F, §§25.1301–25.1461 | Equipment and systems, system safety, flightcrew information, electrical/electronic equipment | Treat §25.1309 as a system-level constraint; preserve crew alerts, control logic, power, environmental and installation assumptions. |
| Subpart G, §§25.1501–25.1587 | Operating limitations and information | Produce candidate AFM limitations, performance assumptions, abnormal/emergency procedures, and evidence gaps; do not turn them into approved operating limitations. |
| Subpart H, §§25.1701–25.1733 | Electrical Wiring Interconnection Systems (EWIS) | Model EWIS as a system with routing, separation, zones, protection, ageing/service-life and maintenance implications. |
| Subpart I, §25.1801 | Special Federal Aviation Regulation | Check whether the selected certification basis contains special rules affecting the concept. |
| Appendices | ICA, flight tests, icing, ATTCS, emergency evacuation, ETOPS, HIRF, fuel-tank flammability, SLD icing, and other detailed material | Create requirement rows and evidence placeholders rather than treating appendices as optional report prose. |

EASA publishes the parallel [CS-25 group page](https://www.easa.europa.eu/en/document-library/certification-specifications/group/cs-25-large-aeroplanes). The local copy used for this research is [CS-25 Amendment 28](../../bib/certification-safety/easa-cs-25-amendment-28.pdf), made available from EASA's official download endpoint. The wizard should keep FAA and EASA profiles distinct even where section numbers and safety intent are similar; it should record the selected authority, amendment, interpretation, and means-of-compliance reference on every requirement.

### 2.2 System-safety rule and accepted methods

The current [14 CFR §25.1309](https://www.law.cornell.edu/cfr/text/14/25.1309) applies to equipment and systems as installed, including systems on which compliance with other Subparts depends. In outline, it requires that systems perform as intended in the operating and environmental conditions, that other equipment not adversely affect safety, and that:

- each catastrophic failure condition be extremely improbable and not result from a single failure;
- each hazardous failure condition be extremely remote;
- each major failure condition be remote;
- significant latent failures be eliminated as far as practical or their exposure/latency be limited; and
- crew information, controls, indications and annunciations support timely recognition and reduce design-induced crew error.

The rule also contains specific treatment for combined latent/active failures and exceptions that defer to other sections, including flight controls, brakes, evacuation, engine rotor/case failures, propeller debris and related special cases. ALAS should therefore link a failure condition to its governing section instead of assuming that every hazard is evaluated through one generic probability formula.

The current [FAA AC 25.1309-1B](https://www.faa.gov/regulations_policies/advisory_circulars/index.cfm/go/document.information/documentID/1043037) is an acceptable-means-of-compliance document, not a regulation and not the only possible method. It covers FHA, failure-condition classification, safety objectives, common cause, integrated systems, crew and maintenance actions, latent failures, and accepted probability treatments. It also makes clear that systems supporting Subparts B/C/D remain within the system-safety discussion. The local research copy is [faa-ac-25-1309-1b.pdf](../../bib/certification-safety/faa-ac-25-1309-1b.pdf).

EASA's [online Easy Access Rules for CS-25](https://www.easa.europa.eu/en/document-library/easy-access-rules/online-publications/easy-access-rules-large-aeroplanes-cs-25) provides CS-25 and AMC/GM material on §25.1309. It uses the same broad failure-condition vocabulary and expects safety analyses to identify functions, interfaces, failure conditions, classifications, probabilities or mitigations, common cause, crew/ground actions, and completeness/traceability. It also explicitly brings ground phases such as line maintenance, dispatch decisions, embarkation/disembarkation and taxi into the relevant operating context; shop maintenance and storage are treated differently. ALAS should have a phase-of-operation field rather than a binary “flight/ground” flag.

The SAE documents are important industry references, but their text is copyrighted and access-controlled:

- [SAE ARP4761 / ARP4761A](https://saemobilus.sae.org/standards/arp4761-guidelines-methods-conducting-safety-assessment-process-civil-airborne-systems-equipment) covers civil airborne safety-assessment methods, including FHA, FMEA, FTA and related analyses.
- [SAE ARP4754A / ARP4754B](https://saemobilus.sae.org/standards/arp4754a-guidelines-development-civil-aircraft-systems) covers aircraft/system development, requirements, architecture, validation and verification, with development-assurance integration.
- [SAE AIR7209](https://saemobilus.sae.org/standards/air7209-development-assurance-principles-aerospace-vehicles-systems) provides higher-level development-assurance principles.

ALAS may use these as named references and traceability anchors, but it should not reproduce or imply access to their full copyrighted text. The exact process, assurance levels and authority agreement belong to the certification programme.

### 2.3 Software, hardware and environmental qualification references

[RTCA's DO-178 page](https://www.rtca.org/do-178/) identifies DO-178C for airborne software and the related DO-254, DO-297 and environmental documents. These documents are purchased standards and are not downloaded here. At concept stage, ALAS should record a software/hardware development-assurance placeholder when a safety function depends on programmable electronics, complex hardware, integrated modular avionics, or environmental qualification. The placeholder is not a DAL assignment unless the aircraft-level safety assessment and certification programme have allocated one.

The environment and installation model should distinguish:

- aircraft-level failure-condition safety objectives;
- development assurance for software/electronic hardware;
- environmental qualification such as RTCA DO-160/ED-14;
- installation-specific evidence such as separation, bonding, HIRF/lightning, fire, vibration, temperature, fluids and moisture; and
- operational/maintenance procedures.

Collapsing those into a single “system certified” boolean would hide the evidence that later reviewers need.

### 2.4 ICAO operational safety context

[ICAO's Safety Management Manual page](https://www.icao.int/safety-management/SMI/SMM) describes Doc 9859 and its safety-management framework. [ICAO's access page](https://www.icao.int/safety-management/access-icao-annexes-and-guidance) states that many guidance materials are accessible through controlled read-only or purchased channels. Doc 9859, Annex 19 and the Global Aviation Safety Plan are therefore recorded as link-only references here; no local copies were downloaded because the rights and distribution conditions are not sufficiently clear for this task.

ICAO material is useful for the operational safety-management boundary: hazards, safety performance, safety intelligence, risk controls, and continuous improvement. It does not replace a Part 25/CS-25 aircraft airworthiness analysis. ALAS should expose the interface between the aircraft concept's residual risks and the future operator/state safety-management system, without asserting that an aircraft design output is an ICAO safety approval.

### 2.5 NASA and DLR open research

NASA's [System Safety Handbook, NASA/SP-2010-580](https://ntrs.nasa.gov/citations/20120003291) and [Systems Engineering Handbook Rev 2, NASA/SP-2016-6105](https://ntrs.nasa.gov/citations/20170001761) are public NASA technical publications. They provide useful open methods for hazard analysis, risk controls, requirements, architecture, verification, uncertainty and traceability. They are not transport-aircraft certification rules.

DLR's [Systems Architecting: A Practical Example of Design Space Modeling and Safety-Based Filtering](https://elib.dlr.de/188223/) demonstrates a safety-based filter in an aircraft design-space context. Its architecture concepts—generic sources/distributors/consumers/devices, allocation, redundancy, and hard architectural filters—are directly relevant to an ALAS pre-screen. It is research evidence, not an approved means of compliance.

DLR's open [The Bird Strike Challenge](https://elib.dlr.de/134450/) / [MDPI publication](https://www.mdpi.com/2226-4310/7/3/26) is useful for modelling bird-strike exposure, impact areas, operational mitigations and the difference between a physical screen and substantiation testing. The open DLR battery-electric system-design study ([record](https://elib.dlr.de/204588/)) is useful as an emerging-technology architecture example, but its student/research status must not be mistaken for a certification method.

## 3. Operational envelope and load-case model

### 3.1 Required envelope fields

The existing requirements-first design in [`docs/REQUIREMENTS_FIRST_AIRCRAFT_DESIGN.md`](../REQUIREMENTS_FIRST_AIRCRAFT_DESIGN.md) already establishes value, unit, load case, policy, evidence and status as the core contract. Certification/safety adds the following required fields:

| Field | Example values | Why it matters |
|---|---|---|
| Certification profile | `FAA_PART_25`, `EASA_CS_25`, `OTHER`, `CONCEPT_ONLY` | Determines applicable sections, amendments, accepted interpretations and later authority coordination. |
| Basis issue | Amendment/date, special conditions, exemptions, equivalent-safety findings | A section number without its issue and special conditions is not a stable requirement. |
| Operation | passenger transport, cargo, commuter, special mission; Part 121/135 or equivalent later | Part 25 is not the whole operational approval. |
| Configuration | seats/rows/exits, cargo, propulsion, energy storage, automation, avionics | Drives evacuation, fire, power, system safety, bird/icing and structural applicability. |
| Phase | dispatch, taxi, takeoff, climb, cruise, descent, approach, landing, diversion, emergency landing, maintenance | Failure exposure and crew/ground response depend on phase. |
| Environment | altitude, Mach/airspeed, temperature, humidity, icing condition, turbulence, lightning/HIRF, precipitation, runway state, wildlife exposure | A safety function is evaluated in the conditions in which it is expected to operate. |
| Mass/CG | OEW, payload, fuel/energy, ZFW/MZFW, ramp, MTOW, MLW, forward/aft CG | Loads, controllability, evacuation and performance can be worst at different configurations. |
| Dispatch state | all available, MEL-like item unavailable, latent fault present, maintenance test due, degraded power/control channel | Prevents hidden all-systems-available assumptions. |
| Evidence state | model, analysis, test, inspection, flight test, authority agreement, operational data | Separates an early screen from a compliance finding. |

An intended “certification profile” should be a hard input. If the user selects `CONCEPT_ONLY`, the wizard may still run safety screens, but it must label all regulatory mappings as provisional and must not present a certification conclusion.

### 3.2 Minimum phase and load-case catalogue

The following catalogue is a practical preliminary set. A selected certification basis can add, remove or refine cases; removal needs a rationale.

| ID | Phase / condition | Preliminary state variables and residuals |
|---|---|---|
| `LC-DISPATCH` | Pre-flight dispatch and line maintenance | Dispatch configuration, latent failures, maintenance test status, MEL-like assumptions, minimum crew information, ground hazards. |
| `LC-TAXI` | Taxi, pushback, embarkation/disembarkation | Steering/braking, power availability, doors/exits, ground personnel exposure, smoke/fire and evacuation paths. |
| `LC-TO` | Takeoff, including rejected takeoff where applicable | MTOW/MLW as applicable, forward/aft CG, runway length/slope/surface, wind, temperature/altitude, engine/power failure, directional control, braking and emergency response. |
| `LC-CLIMB` | Normal climb and one-engine-inoperative/degraded propulsion climb | OEI path, climb gradient/margin, yaw/roll control, thermal limits, power-channel independence, alerting and crew action. |
| `LC-CRUISE` | Normal cruise and maximum altitude/Mach envelope | MMO/VMO, altitude, temperature, pressurisation, fatigue/environment exposure, lightning/HIRF, fuel/energy and system latent-failure exposure. |
| `LC-TURB-GUST` | Gust, manoeuvre and turbulence | Limit/ultimate loads, gust response, structural margins, control-law/actuator loads, passenger/cabin and equipment retention. |
| `LC-ICE` | Known icing and relevant supercooled-large-droplet envelope | Ice accretion/degradation, stall margin, drag, climb, control, detection/activation, anti-ice power/heat/bleed capacity, exit from icing. |
| `LC-LIGHTNING-HIRF` | Direct/indirect lightning and HIRF environment | Bonding/return paths, fuel ignition protection, shielding, safe state after transients, loss or corruption of safety functions. |
| `LC-APP` | Approach, balked landing and go-around | Vapp/approach configuration, stall margin, control authority, engine/power failure, alerting and runway/obstacle margins. |
| `LC-LAND` | Normal landing, crosswind/tailwind and wet/contaminated runway | MLW, CG, sink rate, gear/brake/steering, loads, stopping distance, evacuation accessibility after landing. |
| `LC-GROUND` | Taxi/landing gear/ground loads | Landing, side, drift, towing, taxi, gear collapse, brake energy, structural and fuel-system consequences. |
| `LC-EMERGENCY-LAND` | Emergency landing/crashworthiness | Inertia directions, restraint/seat/occupant load paths, fuel-tank protection, fire ignition, emergency lighting, exits and post-impact egress. |
| `LC-DITCHING` | Ditching if applicable to intended operation/configuration | Water loads, flotation/survivability assumptions, exits and life-saving equipment; retain as applicability/evidence item even when not modelled. |
| `LC-FIRE-SMOKE` | Engine/APU/cargo/cabin/EWIS/flammable-fluid fire and smoke | Fire zones, detection latency, isolation/suppression, smoke penetration, crew workload, evacuation paths, thermal propagation. |
| `LC-BIRD` | Bird strike at relevant speeds/altitudes and impact zones | Energy/mass/velocity screen, windshield/empennage/controls/pitot/propulsor/engine effects, continued safe flight and landing. |
| `LC-FAILURE-RECOVERY` | Active failure and crew recovery | Transient loads, automatic reconfiguration, control authority, remaining system margins, procedure timing, failure annunciation. |
| `LC-LATENT-ACTIVE` | Latent failure followed by an active failure | Latent exposure window, detection/maintenance, independent channel count, common-cause assumptions, probability placeholder/evidence state. |

The wizard should evaluate each candidate over a declared Cartesian or sampled set of mass, CG, environment, phase, dispatch and failure states. The result should identify the governing case, not merely report a single nominal aircraft point.

### 3.3 Load cases versus compliance evidence

A load case is a design input to a calculation. It is not itself compliance evidence. For example, `LC-BIRD` can require a point-mass impact energy calculation and a residual control/structure margin; a later compliance package may require substantiation analysis, material/attachment evidence, component testing and/or full-scale tests. Similarly, `LC-ICE` can screen iced aerodynamic coefficients and anti-ice capacity, while later evidence may require approved icing envelopes, analysis, tunnel tests, flight tests and AFM limitations.

ALAS should carry the two objects separately:

```text
LoadCase { phase, environment, mass_cg, configuration, dispatch_state, failure_state }
SafetyResidual { requirement_id, load_case_id, actual, target, direction,
                 policy, severity, physical_or_evidence, status, evidence_refs }
```

The existing `ConstraintResidual` shape in `alas-opt` can carry the numeric residual, hard/soft policy, severity and physical/evaluation-failure kind. The richer certification metadata can remain in the requirement/evidence layer until an implementation owner chooses a stable schema.

## 4. Preliminary FHA/PSSA/SSA model

### 4.1 Analysis hierarchy

The following hierarchy is recommended for the wizard. The names are deliberately qualified as **preliminary** until the certification programme adopts the applicable standard, assurance process, authority agreement and review records.

1. **Aircraft functional hazard assessment (preliminary FHA).** List aircraft functions, failure conditions, phase of operation, crew/ground exposure, effects on occupants/non-occupants, severity classification, detection/annunciation, and safety objective. Start with aircraft-level effects, then decompose to systems.
2. **Preliminary system safety assessment (preliminary PSSA).** Allocate safety objectives to functions, systems and architecture. Evaluate redundancy, independence, monitoring, separation, common-cause threats, power/control paths, degraded modes, crew action and dispatch state. Generate architecture filters before expensive optimisation.
3. **System safety assessment (future SSA evidence package).** Combine detailed FHA, FMEA/FMECA, fault trees, common-cause analysis, zonal safety analysis, particular-risk analysis, dependency/propagation analysis, software/hardware assurance evidence, tests, inspections, maintenance actions, and approved operational information as appropriate.

The wizard should never imply that a preliminary graph or heuristic has become an SSA. A report can say “preliminary PSSA screen: no single failure identified in the model” while still saying “SSA evidence pending; model coverage and independence assumptions incomplete.”

### 4.2 Failure-condition classes

Use a stable class vocabulary aligned with Part 25/CS-25 system-safety language:

| Class | Conceptual effect | Early design use |
|---|---|---|
| No safety effect | No effect on safety or operational capability | Record for completeness; do not spend architecture effort unless it affects another function. |
| Minor | Small reduction in safety margin or capability; manageable crew workload | May be a soft/diagnostic item early, but still needs an owner and later evidence if applicable. |
| Major | Significant reduction in safety margins or functional capability; increased crew workload or discomfort | Hard architecture/performance screen where the concept relies on the function; later probability and evidence. |
| Hazardous | Large reduction in safety margins, physical distress or injury potential, or workload that could prevent correct action | Hard preliminary fault-tolerance/crew-action screen; quantitative probability is evidence pending until justified. |
| Catastrophic | Multiple fatalities, usually through loss of aircraft/occupants | Hard “no single failure” and continued-safe-flight/landing architecture screen; certification probability/evidence remains pending. |

The conventional words “remote,” “extremely remote” and “extremely improbable” are regulatory safety-objective terms, not permission to invent a probability from a conceptual reliability estimate. The wizard may store a **screening placeholder** for the target class, but it should not call that placeholder a Part 25/CS-25 probability finding until the source data, independence model, exposure basis, uncertainty treatment and authority-accepted method are documented.

### 4.3 What belongs in each analysis record

Each failure-condition record should contain:

- unique ID and aircraft function;
- system boundaries, interfaces, components, power/control/data sources and consumers;
- normal and degraded modes;
- initiating failure, propagation path, effects at equipment/system/aircraft level;
- phase of operation and exposure time;
- local, crew, maintenance, dispatch and ground-person response;
- severity class and rationale;
- detection/annunciation latency and false/missed-alert assumptions;
- active and latent failure state;
- independence/common-cause assumptions (fire, heat, fluid, vibration, lightning/HIRF, software/data, installation zone, maintenance error, manufacturing/configuration);
- redundancy and voting/monitoring logic;
- residual control, performance, structural and thermal margins;
- required evidence type and evidence owner;
- source references, model version, solver status and reviewer/authority decision.

This makes the system-safety graph usable by the optimiser and by the eventual compliance matrix. It also avoids the common failure mode where a diagram shows “three channels” without proving that they do not share a power bus, actuator, data source, zone, software defect, fire source or maintenance action.

## 5. Safety-driven architecture constraints

These constraints are suitable for a conceptual architecture filter. They are design intent and preliminary screening rules, not certification findings.

### 5.1 Fault tolerance and independence

- A catastrophic failure condition may not be caused by one modelled failure. If the concept cannot show this at the architecture level, reject it or mark it inconclusive rather than hiding the gap in a reliability target.
- Separate redundant channels by power generation/distribution, control/data paths, physical zones, thermal/fire exposure, fluid systems, structure/actuation and maintenance access where the safety assessment depends on independence.
- Model common-cause and cascading failures explicitly. A duplicated sensor on the same mounting, a redundant computer running identical defective logic, or two channels sharing one connector is not automatically independent.
- A monitored failure must have sufficient detection coverage and a safe response before the latent exposure becomes unacceptable. “Monitoring exists” is not equivalent to “latent failure is eliminated.”
- Record the transition between normal, degraded, fail-safe, reversionary and uncontrolled states. A temporary transient can impose structural or control loads even if the final state is recoverable.
- Preserve control authority and energy for continued safe flight and landing after required failures. This is a physical margin screen, not an assumption that the pilot can always compensate.

### 5.2 Power, propulsion and energy storage

- Allocate independent sources and distribution for functions whose simultaneous loss creates hazardous or catastrophic effects.
- Model engine/power failure yaw, roll, thrust asymmetry, thermal and electrical transients, including the case where automatic reconfiguration is unavailable or delayed.
- For batteries or other high-energy storage, include thermal runaway initiation, propagation, venting, smoke/toxic products, fire containment, isolation, detection, suppression, crash damage and safe landing/evacuation. Use the DLR study only as a conceptual architecture reference, not as an authority-approved battery basis.
- Treat fuel/energy tank ignition prevention, lightning protection, flammable fluid containment, hot-surface separation, and fire-zone isolation as linked constraints; solving only one will not close the hazard.

### 5.3 Crew, passenger and ground response

- A failure condition that requires crew action must include alerting, control/display design, time-to-recognise, procedure steps, workload and competing alerts. The assumption “crew responds correctly” must be an explicit, reviewable input.
- Include embarkation, disembarkation, taxi, dispatch and line maintenance when a failure can injure people on the ground or affect an evacuation/door/ground-operation function.
- Model passenger seat/aisle/exit geometry, blocked exits, emergency lighting, smoke and fire penetration, and post-impact access. A nominal seat count is not an evacuation result.
- Record non-occupant risks where the aircraft function or failure can affect ramp personnel, emergency responders or people below the aircraft.

## 6. Hazard-specific modelling guidance

### 6.1 Fire, smoke, flammability and EWIS

Relevant references include Part 25 §§25.853–25.867, §25.981 and associated appendices, [FAA AC 25-9A](https://www.faa.gov/regulations_policies/advisory_circulars/index.cfm/go/document.information/documentID/22464), [AC 25.981-1D](https://www.faa.gov/airports/resources/advisory_circulars/index.cfm/go/document.information/documentNumber/25.981-1D), [AC 25.981-2A](https://www.faa.gov/regulations_policies/advisory_circulars/index.cfm/go/document.list/?appliedFacets=%7B%22subjectclass%22:%2225.981%22%7D&statusID=2), [AC 25.954-1](https://www.faa.gov/regulations_policies/advisory_circulars/index.cfm/go/document.information/documentID/1032746), [AC 25.1701-1](https://www.faa.gov/regulations_policies/advisory_circulars/index.cfm/go/document.information/documentID/73476), and the matching CS-25/AMC material.

Preliminary hard screens:

- identify fire zones, hot surfaces, ignition sources, flammable-fluid lines, tanks, vents, batteries and high-energy conductors;
- provide plausible physical separation, containment, drainage/venting and shutoff/isolation paths;
- prevent one wiring or fuel-system failure from defeating detection, isolation and suppression simultaneously;
- preserve critical controls, power and egress after a credible local fire for the declared time/phase;
- identify EWIS routing/separation, chafe/arc protection, moisture/fluid exposure, heat, vibration, installation zone and maintenance access;
- for fuel tanks, retain both ignition-source protection and flammability exposure/reduction as separate requirements; and
- carry smoke penetration, detection latency, crew alerting, cabin visibility and evacuation procedure assumptions.

Evidence pending includes material flammability tests, fire penetration/burnthrough tests, fire detection and suppression tests, tank ignition analysis/test, EWIS inspection/installation evidence, zonal safety analysis, service-life/maintenance instructions and flight/ground demonstrations. The local copies include AC 25-9A, AC 25.981-1D, AC 25.981-2A, AC 25.954-1, AC 25.853-1 and AC 25.1701-1 for research only.

### 6.2 Bird strike

The current [14 CFR §25.631](https://www.law.cornell.edu/cfr/text/14/25.631) addresses bird-strike damage, with related structural/damage-tolerance and system consequences depending on the design. The open [DLR/MDPI Bird Strike Challenge paper](https://www.mdpi.com/2226-4310/7/3/26) describes how exposure depends on altitude, time, environment, airport and operational choices; it also discusses airframe, windshield, empennage, pitot and propulsor/engine consequences.

ALAS should:

- declare the bird mass, speed/energy, altitude and impact locations used by the preliminary screen;
- include windshield, radome/sensors, leading edges, empennage, control surfaces, propeller/fan/engine ingestion, pitot/static and critical wiring/actuator locations where applicable;
- propagate damage to continued control, displays, propulsion, fire risk and emergency landing rather than treating “skin damage” as an isolated result;
- expose operational mitigations such as speed/altitude or airport wildlife controls as assumptions, not hidden credit; and
- mark unmodelled impact zones as diagnostic/inconclusive, never pass-by-omission.

Later evidence can include detailed impact analysis, material/attachment substantiation, component or full-scale tests, engine/propulsor ingestion evidence, windshield/sensor qualification and accepted operating limitations.

### 6.3 Lightning and HIRF

Relevant references include Part 25 §§25.581, 25.899, 25.954 and 25.1316, [FAA AC 20-136C](https://www.faa.gov/regulations_policies/advisory_circulars/index.cfm/go/document.information/documentID/1045196), and applicable EASA material. The local AC 20-136C is the current FAA document available at the research date.

Preliminary screens should identify:

- direct-lightning attachment and current-return paths through the airframe;
- bonding, joints, fasteners, control surfaces, antennas, fuel tanks and vents;
- indirect transients and their effect on power, sensors, control computers, displays and data buses;
- HIRF/shielding/grounding zones and safe states after upset;
- whether redundant channels share an exposed route, connector, enclosure, software input or power source; and
- fuel-tank ignition prevention and flammable-vapour assumptions.

Later evidence includes lightning/HIRF analyses, installation inspections, test plans/results, environmental qualification and authority-agreed means of compliance. At concept stage, an unvalidated electromagnetic model is a diagnostic/evidence placeholder, not a compliance pass.

### 6.4 Icing

Relevant references include CS-25 Appendix C and Appendix O where applicable, Part 25 §§25.1419/25.1420 and associated systems/flight requirements, [FAA AC 25-28](https://www.faa.gov/regulations_policies/advisory_circulars/index.cfm/go/document.information/documentid/1019691), and [FAA AC 25-25A](https://www.faa.gov/regulations_policies/advisory_circulars/index.cfm/go/document.information/documentID/1026166).

The wizard should make icing a declared environment and system state:

- identify protected and unprotected surfaces, sensors, detectors, activation logic and cycling;
- allocate thermal, bleed, pneumatic, mechanical and electrical capacity with degraded-source cases;
- evaluate iced lift, drag, stall speed/margin, control authority, stability, propulsor/engine performance, climb and go-around;
- include detection/activation latency and pilot procedure assumptions;
- carry SLD/Appendix O applicability as an evidence/requirements row rather than assuming Appendix C covers all icing; and
- expose icing as an operating limitation when the intended concept does not have the required protection.

The local AC 25-28 and AC 25-25A support research and model definition. Tunnel/flight-test substantiation and the final AFM/limitations remain evidence pending.

### 6.5 Emergency landing, evacuation and ditching

Relevant references include Part 25 §§25.561, 25.721, 25.801, 25.803, 25.807–25.813 and Appendices J as applicable, [FAA AC 25.803-1A](https://www.faa.gov/airports/resources/advisory_circulars/index.cfm/go/document.information/documentNumber/25.803-1A), and the current CS-25 equivalents.

Preliminary hard screens should cover:

- emergency landing inertia directions and occupant/restraint/load-path assumptions;
- fuel tank and energy-storage protection from impact, rupture and ignition;
- seat, floor, structure, landing gear and equipment retention concepts;
- exit count, exit type, door/slide/raft assumptions, aisle width, seat/exit adjacency, blocked-exit cases and access paths;
- emergency lighting, signs, smoke visibility, cabin pressure/ventilation and crew/passenger information;
- evacuation capacity and path geometry for the declared seating/configuration; and
- ditching applicability, water loads, flotation/survivability and life-saving equipment where the mission requires it.

Where the selected rule invokes a full-scale evacuation demonstration or formal analysis, including the applicable 90-second criterion, ALAS should store that value as a **substantially later evidence target**. It may run a conservative geometric/flow diagnostic, but a simulated time is not an evacuation demonstration and must not be displayed as a certification finding. [AC 25.803-1A](../../bib/certification-safety/faa-ac-25-803-1a.pdf) specifically describes full-scale demonstrations and formal analysis supported by tests/demonstrations.

### 6.6 Structural damage tolerance and failure-induced loads

Relevant references include Part 25 §§25.301, 25.305, 25.307, 25.571 and 25.631, and [FAA AC 25.571-1D](https://www.faa.gov/regulations_policies/advisory_circulars/index.cfm/go/document.information/documentID/865446). The wizard should include fatigue/damage-tolerance and structural inspection/maintenance assumptions as a separate evidence stream from static strength.

The structural model should account for loads caused by system malfunction, engine/power failure, control reversion, asymmetric thrust, emergency landing and impact, not only the nominal 1-g flight envelope. A safety analysis that assumes “the structure is strong” but does not pass the failure-induced loads to `alas-struct` is incomplete.

## 7. Hard preliminary constraints, later evidence and diagnostic placeholders

The following classification is recommended for the first wizard implementation. “Hard” means hard for the declared concept profile and architecture screen; it does not mean that ALAS has proved regulatory compliance.

### 7.1 Hard preliminary constraints

| ID | Preliminary rule | Typical policy | Why it can reject a concept early |
|---|---|---|---|
| `CERT_BASIS_SELECTED` | Authority/profile, issue, configuration and operation are declared | Hard input | An unscoped certification question cannot produce a stable requirement set. |
| `CERT_ENVELOPE_DECLARED` | Mass/CG, speed/Mach, altitude, temperature, runway, environment and phases are bounded | Hard input | An unbounded design cannot be screened for loads, performance or system exposure. |
| `SAFETY_FUNCTIONS_IDENTIFIED` | Critical aircraft functions and system boundaries are present | Hard | Missing functions make a “pass” meaningless. |
| `SAFETY_NO_SINGLE_FAILURE_CAT` | No single modelled failure produces a catastrophic aircraft effect | Hard architecture screen | Directly reflects the §25.1309 catastrophic/single-failure principle, subject to applicable exceptions. |
| `SAFETY_COMMON_CAUSE_SCREEN` | Shared power, data, zone, fluid, thermal, software, maintenance and installation threats are screened | Hard | Nominal duplication is not independence. |
| `SAFETY_DISPATCH_STATE_DECLARED` | All-available, latent-fault and MEL-like states are explicit | Hard | Hidden dispatch assumptions invalidate exposure and redundancy claims. |
| `SAFETY_CONTINUED_SAFE_FLIGHT_LANDING` | Required failure/reversion states retain control, propulsion, alerting and landing margins | Hard physical screen | A safety architecture without residual control authority is not credible. |
| `SAFETY_OEI_OR_DEGRADED_PROPULSION` | Declared engine/power failure cases meet the concept's flight/landing intent | Hard where applicable | OEI/degraded propulsion can drive control, loads, thermal, performance and evacuation consequences. |
| `SAFETY_EMERGENCY_LANDING_LOAD_PATH` | Occupant/structure/fuel/energy load paths exist for emergency cases | Hard geometry/loads screen | Unrecoverable load-path or ignition architecture cannot be fixed by later paperwork. |
| `SAFETY_EGRESS_GEOMETRY` | Declared seats, aisles, exits, access and blocked-exit cases are geometrically plausible | Hard geometry screen | A cabin that cannot physically evacuate cannot be certified by adding a report. |
| `SAFETY_FIRE_ZONES` | Fire zones, flammable fluids, hot surfaces, containment, shutoff and critical-function protection are identified | Hard architecture screen | Fire propagation can defeat multiple redundant systems. |
| `SAFETY_FUEL_IGNITION` | Fuel/energy ignition sources and protection paths are identified | Hard screen | Ignition prevention is a primary safety constraint, not a late material choice. |
| `SAFETY_EWIS_SEPARATION` | Critical wiring routes, protection and physical separation are plausible | Hard screen | Shared routing, chafe or fire exposure can defeat nominal system redundancy. |
| `SAFETY_BIRD_IMPACT_POINTS` | Applicable bird-impact masses/speeds/zones are declared and screened | Hard where applicable | Unmodelled critical impact zones must block a claimed positive result. |
| `SAFETY_ICE_APPLICABILITY` | Icing envelope and protection/limitation strategy are declared | Hard input | “Not evaluated” icing cannot be presented as a normal-flight pass. |
| `SAFETY_LIGHTNING_RETURN_PATH` | Direct/indirect current paths, bonding and affected safety functions are identified | Hard screen | A concept without a plausible return/shielding strategy has an architectural gap. |
| `SAFETY_EVIDENCE_GATES` | Required later demonstrations/tests/analyses have owners and status | Hard process gate for final wizard acceptance | A physical screen cannot promote itself to evidence. |

### 7.2 Later compliance evidence

These are outputs or gates for subsequent design phases, not numbers that a first-pass optimiser should invent:

- approved/authority-agreed certification basis, special conditions, exemptions and means of compliance;
- aircraft and system FHA, PSSA, SSA, FMEA/FMECA, fault trees, common-cause analysis, zonal safety analysis, particular-risk analysis and dependency/propagation analysis;
- allocated safety objectives, development-assurance plans and software/electronic-hardware evidence;
- validated reliability/probability data, exposure assumptions, uncertainty treatment, independence arguments and latent-failure coverage;
- structural static strength, fatigue, damage tolerance, crack-growth, inspection and limit-of-validity evidence;
- flight tests and qualification under the selected flight/handling/performance conditions;
- OEI/degraded propulsion demonstrations, engine/propulsor containment/ingestion and thermal evidence;
- fire/flammability, fire penetration, detection/suppression, fuel ignition, tank flammability and EWIS evidence;
- lightning/HIRF and environmental qualification, installation inspections and test results;
- icing analysis, tunnel/flight tests, detection/protection and AFM/limitations;
- bird-strike structural/windshield/control/sensor/engine substantiation and tests;
- emergency landing, crashworthiness, evacuation, cabin safety, ditching and lifesaving demonstrations/analyses;
- maintenance instructions, airworthiness limitations, certification maintenance requirements, ICA, CMR, wiring/zonal inspection tasks and dispatch/MEL assumptions; and
- compliance matrix review, configuration management, model/tool qualification where applicable, independent review and authority disposition.

### 7.3 Diagnostic placeholders

The following should be visible in the wizard as unresolved diagnostics or evidence placeholders:

- exact accepted numerical probabilities for catastrophic/hazardous/major failure conditions;
- component failure rates, common-cause beta factors, coverage, repair/maintenance intervals and fleet exposure;
- exact DAL/FDAL/IDAL assignments before the aircraft-level safety assessment and programme agree them;
- evacuation time from a conceptual flow model before a valid test/analysis basis exists;
- post-bird-strike residual control/propulsion capability in an unvalidated impact model;
- Appendix O/SLD icing performance when only a generic ice model exists;
- lightning/HIRF upset survivability without installation-specific electromagnetic evidence;
- smoke concentration/visibility and toxic-product predictions without validated fire/cabin models;
- battery thermal-runaway propagation, venting and suppression without validated cell/module/installation data;
- ditching survival/flotation where the mission/configuration is not yet defined;
- crew workload, alerting effectiveness and procedure success without a human-factors assessment; and
- cyber/security or intentional-threat assumptions if they affect a safety function but the selected certification/operational basis has not been defined.

For each placeholder, the wizard should show what is missing, why it matters, the blocking stage, the owner, and the evidence needed. It should not turn a placeholder into a green check merely because no solver exception occurred.

## 8. Residuals and algorithm integration

### 8.1 Reuse the requirements-first acceptance semantics

The current ALAS concepts already distinguish hard/soft policy, feasibility-first ranking, positive residual as violation, and evaluation failure. Certification/safety should use the same semantics:

```text
upper-bound residual = actual - limit
lower-bound residual = limit - actual
boolean architecture screen = 0 when the stated condition is satisfied,
                               1 when it is violated
```

`ConstraintResidual.kind = Physical` is appropriate when a valid model has evaluated a physical or architecture condition. `ConstraintResidual.kind = EvaluationFailure` is appropriate when a required translator, data source, solver, model coverage or evidence gate is unavailable. A failed evaluation must not become a plausible candidate with residual zero.

For an unknown safety probability, do not fabricate a numeric residual. Create an evidence/diagnostic row with `status = evidence_pending` or `inconclusive`, and apply the selected acceptance policy. A strict certification-profile run may treat the unresolved row as a hard acceptance blocker even though it is not a physical violation; a concept-exploration run may allow ranking while prominently showing the blocker.

### 8.2 Proposed safety requirement IDs and residuals

| Requirement ID | Actual/target or predicate | Default early policy | Evidence/status rule |
|---|---|---|---|
| `safety_no_single_failure_cat` | `single_failure_catastrophic_count` must equal `0` | Hard | Physical `0/1` only after model coverage; otherwise evaluation failure. |
| `safety_common_cause_separation` | Number/severity of unresolved common-cause paths | Hard | Missing CCA is `inconclusive`, not pass. |
| `safety_latent_exposure_time` | `actual_latent_exposure - allowed_exposure` | Hard where required | Allowed exposure requires declared detection/maintenance assumption. |
| `safety_dispatch_configuration` | Required channel/source/crew state exists for each dispatch case | Hard | No hidden all-available assumption. |
| `safety_continued_safe_flight_landing` | Minimum control/propulsion/landing margin after failure | Hard | Evaluate per phase/load case; worst case governs. |
| `safety_oei_climb_margin` | `required_climb - available_climb` | Hard where applicable | Include mass, altitude, temperature, CG and failed source. |
| `safety_emergency_landing_inertia` | Load-path margin for emergency landing directions | Hard screen | Detailed substantiation pending. |
| `safety_evacuable_seating_capacity` | `declared_seats - conservative_egress_capacity` | Hard geometry screen | Full-scale/approved analysis pending. |
| `safety_exit_egress_path` | Blocked/unblocked path predicate | Hard | Smoke, lighting, door and slide evidence pending. |
| `safety_evacuated_time_placeholder` | Simulated time versus declared target, if a model exists | Diagnostic/evidence | Never a certification pass without accepted method and evidence. |
| `safety_fire_zone_separation` | Unresolved propagation path count/severity | Hard | Fire tests/zonal evidence pending. |
| `safety_fuel_ignition_source` | Unprotected ignition-source predicate | Hard | AC/CS/Part 25 applicability and evidence recorded separately. |
| `safety_ewis_separation` | Critical shared-zone/chafe/arc exposure | Hard | Installation and zonal analysis pending. |
| `safety_bird_strike_structure` | Structural/attachment margin at each declared impact point | Hard where applicable | Detailed impact/test evidence pending. |
| `safety_bird_strike_control` | Residual control/propulsion/indication margin | Hard where applicable | Unmodelled zone is inconclusive. |
| `safety_icing_stall_margin` | `required stall margin - iced margin` | Hard if icing operation selected | Validated icing data pending. |
| `safety_icing_climb_margin` | `required iced climb - available iced climb` | Hard if icing operation selected | Appendix C/O applicability retained. |
| `safety_lightning_return_path` | Missing/invalid current-return path predicate | Hard | Installation test/evidence pending. |
| `safety_hirf_safe_state` | Safety function upset/recovery predicate | Diagnostic until model exists | Environmental qualification pending. |
| `safety_damage_tolerance` | Crack/damage/inspection margin | Evidence gate | Do not infer from static strength. |
| `safety_ica_maintainability` | Required inspection/maintenance action coverage | Evidence gate | ICA/CMR/AWL records pending. |

The optimiser should rank feasible candidates first, then minimize normalized hard residuals, failure count/severity, soft residuals and evidence debt according to the existing acceptance policy. The exact weighting must be documented; catastrophic/hazardous unresolved architecture gaps should not be hidden by a small mass or drag objective gain.

### 8.3 Safety residual record

The minimum traceable record should be:

```text
SafetyRequirement {
    id,
    source_reference,
    certification_profile,
    applicability_rationale,
    failure_condition_class,
    phase_and_load_cases,
    policy,
    target_or_predicate,
    architecture_assumptions,
    residual_ids,
    status,
    evidence_refs,
    owner,
    model_version,
    reviewer,
}
```

This makes an optimiser result reproducible: the candidate is not just “safe,” it is safe **with respect to a named preliminary predicate, in a named load case, under named assumptions, with a named evidence gap**.

## 9. Traceability mapping into ALAS

The mapping below is intentionally compatible with the repository's current requirements-first terminology and crate ownership. It is a design proposal for future implementation; this research task does not modify the source files.

| Requirement / source | Design-brief or wizard input | Preliminary stage / owner | Residual or record | Later evidence |
|---|---|---|---|---|
| Part 25/CS-25 profile and issue | New certification profile, authority, amendment, operation, configuration | Brief freeze / `alas-config` + `alas-pipeline` | `CERT_BASIS_SELECTED` | Approved basis, special conditions, authority records |
| §25.1309 / CS 25.1309 system safety | Safety functions, system boundaries, failure-condition classes, assumptions | Architecture / `alas-opt` + pipeline acceptance | `safety_no_single_failure_cat`, `safety_common_cause_separation` | FHA/PSSA/SSA, FMEA/FTA/CCA/ZSA |
| §§25.21–25.255 flight/performance | Existing cruise Mach/MMO/VMO, ICA/TTC, OEI ceiling, altitude, TOFL, landing, Vapp | Performance / `alas-aero`, `alas-prop`, `alas-perf` | Existing `brief_*` performance residuals plus `safety_oei_climb_margin` | Flight test and approved performance data |
| §§25.301–25.341 loads/gust | Mass/CG, speed/altitude, gust/turbulence, engine failure | Loads / `alas-struct`, `alas-atmo` | `LC-TURB-GUST`, failure-induced load residuals | Static/ultimate/fatigue/damage substantiation |
| §§25.561/25.721 emergency landing | Emergency-load profile, gear/structure/fuel/energy architecture | Structure/mass / `alas-struct`, `alas-mass`, `alas-prop` | `safety_emergency_landing_inertia` | Crashworthiness/retention/impact tests and analysis |
| §§25.631/25.571 bird/damage | Bird mass/speed/zone profile, structural materials, critical controls | Geometry/structure / `alas-geom`, `alas-struct`, `alas-stab` | `safety_bird_strike_structure`, `safety_bird_strike_control` | Impact analysis, component/full-scale tests |
| §§25.801–25.813 and Appendix J | Seats, rows, exits, aisles, doors, lighting, blocked exits | Accommodation / `alas-payload`, `alas-geom`, `alas-report` | `safety_evacuable_seating_capacity`, `safety_exit_egress_path` | Evacuation demonstration/accepted analysis |
| §§25.853–25.867 fire/interiors | Material classes, fire zones, hot surfaces, cabin/cargo layout | Geometry/propulsion/payload / `alas-geom`, `alas-prop`, `alas-payload` | `safety_fire_zone_separation` | Flammability, fire/smoke/penetration/suppression tests |
| §§25.954/25.981 fuel ignition/flammability | Fuel/energy tank, vapour, lightning and hot-surface assumptions | Propulsion/structure / `alas-prop`, `alas-struct` | `safety_fuel_ignition_source` | Tank ignition, flammability exposure/reduction, lightning evidence |
| §25.1301/1309/1311/1316 equipment/alerts/lightning | Power/data sources, sensors, alerts, recovery, transient environment | System architecture / `alas-opt`, `alas-stab`, pipeline | `safety_dispatch_configuration`, `safety_lightning_return_path`, `safety_hirf_safe_state` | System safety, human factors, DO-160/installation tests |
| §§25.1419/1420 and Appendix C/O icing | Icing applicability, protected surfaces, anti-ice capacity | Atmosphere/aero/propulsion / `alas-atmo`, `alas-aero`, `alas-prop` | `safety_icing_stall_margin`, `safety_icing_climb_margin` | Icing analysis/tunnel/flight tests and AFM limits |
| §§25.1701–25.1733 EWIS | Wiring zones, separation, protection, service-life/maintenance | Architecture/geometry / `alas-opt`, `alas-geom`, `alas-report` | `safety_ewis_separation` | EWIS/zonal safety analysis and ICA inspections |
| §§25.1501–25.1587 operating limits/info | Declared limits, procedures, dispatch and abnormal/emergency state | Reporting/acceptance / `alas-pipeline`, `alas-report` | Evidence matrix and acceptance status | AFM, limitations, procedures, MMEL/MEL/ICA |
| NASA requirements/architecture guidance | Requirement owner, verification method, uncertainty, review | Cross-cutting / `alas-pipeline`, `alas-report` | Traceability and evidence debt | Verification/validation records |
| DLR safety-based design-space research | Architecture candidates, redundancy, allocation, hard filters | Optimisation / `alas-opt` | Candidate architecture graph and screen log | Programme-specific safety assessment |

The existing design-brief residuals (`brief_design_passengers`, `brief_maximum_passengers`, `brief_design_payload`, `brief_design_cargo`, `brief_ld3_45`, `brief_span_limit`, and the existing performance/structures mappings) should remain separate from safety residuals. A passenger count can satisfy accommodation while still failing evacuation geometry; a wing span can satisfy an airport limit while a bird/lightning/EWIS zone remains unresolved.

Recommended acceptance statuses are:

| Status | Meaning |
|---|---|
| `preliminary_screened` | A declared model/load case evaluated the preliminary predicate; no claim beyond that screen. |
| `accepted_for_concept_search` | The concept passes the current internal gate for exploration; not certification approval. |
| `evidence_pending` | The preliminary design is plausible, but required analysis/test/inspection/authority evidence is not present. |
| `diagnostic` | The item is displayed for awareness or prioritisation, but the current model cannot support a pass/fail conclusion. |
| `inconclusive` | Required coverage, input, solver, translator or applicability decision is missing/contradictory. |
| `not_applicable_with_rationale` | The selected profile and documented configuration justify non-applicability; the rationale is retained. |
| `rejected` | A physical hard residual or architecture rule is violated. |

## 10. Compliance matrix design

The compliance matrix should be generated from the same requirement objects that drive the wizard, not manually reconstructed after optimisation. Minimum columns:

| Column | Required content |
|---|---|
| `requirement_id` | Stable ALAS ID, never only a paragraph string. |
| `source` | FAA regulation, EASA CS/AMC/GM, special condition, adopted standard, NASA/DLR research, or internal design rule. |
| `source_issue` | Amendment/revision/date and source URL. |
| `applicability` | Yes/no/conditional plus rationale and configuration assumptions. |
| `failure_condition` | Aircraft/system function and class where relevant. |
| `load_cases` | Phase, environment, mass/CG, dispatch and failure state. |
| `policy` | Hard minimum/maximum, hard architecture predicate, soft target, maximize, diagnostic, or evidence gate. |
| `actual/target` | Numeric value, boolean predicate, or explicit unknown. |
| `residual` | Signed normalized residual; no zero for unknown. |
| `status` | One of the statuses above. |
| `evidence_type` | Analysis, simulation, test, inspection, flight test, operational procedure, authority agreement, or future. |
| `artifact_ref` | Report/model/test/fixture/commit/configuration identifier. |
| `owner` | Crate/module/team role responsible for closure. |
| `assumptions` | Independence, crew, maintenance, dispatch, environment, data and model assumptions. |
| `review` | Reviewer, date, decision, open action and authority coordination state. |

For a certification-profile run, acceptance should fail closed on any hard architectural violation and should clearly separate that from an evidence blocker. For a concept-only run, the user may choose to rank evidence debt, but the UI must show the debt and must never relabel the candidate “compliant.”

## 11. Suggested wizard sequence

1. **Profile and applicability.** Select authority/basis/issue, operation, category, seating, propulsion/energy, automation, novel technology and intended environment.
2. **Requirements contract.** Capture value/unit, load case, policy, source, applicability, evidence and status. Freeze a versioned brief.
3. **Operational envelope.** Declare mass/CG, speed/Mach, altitude, temperature, runway, icing, lightning/HIRF, bird, fire, ditching and all phases.
4. **Aircraft functions and preliminary FHA.** Identify functions and failure conditions; classify severity and define crew/ground/maintenance responses.
5. **Architecture/PSSA filter.** Allocate functions, power, control, data, fire zones and redundancy; test independence/common-cause and dispatch/latent assumptions.
6. **Initial geometry and sizing.** Build geometry, cabin/egress, propulsion/energy, structural and control states under the safety constraints.
7. **Physical analyses.** Run aero/performance, atmosphere/icing, mass/CG, propulsion/OEI, structure/load cases, egress geometry, fire-zone and bird/lightning screens where translators exist.
8. **Typed residuals.** Produce physical hard/soft residuals, evaluation failures, evidence debt and diagnostics. Keep the governing load case and assumptions.
9. **Feasibility-first ranking.** Reject physical hard violations; rank remaining candidates by normalized hard residual, unresolved safety severity, performance and objective values, with evidence debt visible.
10. **Compliance/evidence matrix.** Export requirement status, links, hashes, model versions, evidence owners and open authority questions. Use “preliminary screen” language.
11. **Finalist gate.** Require review of high-severity unresolved items before high-fidelity optimisation. No finalist is “certified”; it is “eligible for further substantiation.”

## 12. Downloaded research library and rights ledger

All local PDFs below were downloaded on 2026-08-26 from the linked publisher/authority endpoint. Pages and byte sizes were read with Poppler `pdfinfo`; SHA-256 values were calculated with `Get-FileHash -Algorithm SHA256`. The local files are research copies. Publisher copyright, terms, and future document revisions control; hashes identify exactly what this report used.

### 12.1 Local copies

| Local file | Source / title | Issue or publication | Pages | Bytes | SHA-256 | Rights note |
|---|---|---:|---:|---:|---|---|
| [faa-ac-25-1309-1b.pdf](../../bib/certification-safety/faa-ac-25-1309-1b.pdf) | FAA, AC 25.1309-1B, System Design and Analysis | 2024-08-30 | 75 | 1,140,977 | `90E625593D472B74EC69FBCEFE97649F97794410DBE844F7684B397121AC5A92` | Official FAA public advisory circular; research copy. |
| [faa-ac-20-136c.pdf](../../bib/certification-safety/faa-ac-20-136c.pdf) | FAA, AC 20-136C, Aircraft Electrical and Electronic System Lightning Protection | 2026-05-15 | 55 | 3,259,276 | `A97137A34F9094AC10E6FB7CABA11688EDF5AB0B866DDF3D5FFA40F00E327CDD` | Official FAA public advisory circular; research copy. |
| [faa-ac-25-803-1a.pdf](../../bib/certification-safety/faa-ac-25-803-1a.pdf) | FAA, AC 25.803-1A, Emergency Evacuation Demonstrations | 2012-03-12 | 25 | 372,555 | `5550C3B008FBA3B65EC7FD5B320FC801196F5ED3B9B266601A8FA0C88F481FCE` | Official FAA public advisory circular; research copy. |
| [faa-ac-25-981-1d.pdf](../../bib/certification-safety/faa-ac-25-981-1d.pdf) | FAA, AC 25.981-1D, Fuel Tank Ignition Source Prevention | 2018-09-24 | 58 | 873,894 | `E9F1FE237C32B65E99BFBD8B0910AD2ED6688471D42313D875B0DD834B1DBF04` | Official FAA public advisory circular; research copy. |
| [faa-ac-25-981-2a.pdf](../../bib/certification-safety/faa-ac-25-981-2a.pdf) | FAA, AC 25.981-2A, Fuel Tank Flammability Reduction Means | 2008 | 76 | 510,828 | `399C6022D9719BF8D33C0E226CE31F6AC93BC28D3A344945765D57AEDE3596E2` | Official FAA public advisory circular; research copy. |
| [faa-ac-25-954-1.pdf](../../bib/certification-safety/faa-ac-25-954-1.pdf) | FAA, AC 25.954-1, Fuel Tank Lightning Protection | 2018-09-26 | 37 | 744,736 | `3CECDC26B416672EF4AF33E6099BE3DF4B7DF6C39C3B7B01428EEE850ED9004D` | Official FAA public advisory circular; research copy. |
| [faa-ac-25-1701-1.pdf](../../bib/certification-safety/faa-ac-25-1701-1.pdf) | FAA, AC 25.1701-1, Certification of EWIS | 2007-12-04 | 92 | 568,948 | `8E8C3CBD94674D48C767823B49CAE2C0B8D9FF88E827265E3DE4C994F00FDB15` | Official FAA public advisory circular; research copy. |
| [faa-ac-25-571-1d.pdf](../../bib/certification-safety/faa-ac-25-571-1d.pdf) | FAA, AC 25.571-1D, Damage Tolerance and Fatigue Evaluation | 2011-01-13 | 41 | 693,367 | `7E0BBAE7A71D174CBDFB3F45BB3902A1D606053E688844C2EAEBC9A4A8517926` | Official FAA public advisory circular; research copy. |
| [faa-ac-25-7d-change1.pdf](../../bib/certification-safety/faa-ac-25-7d-change1.pdf) | FAA, AC 25-7D, Flight Test Guide for Transport Category Airplanes | 2018-05-04, Change 1 included | 481 | 4,444,887 | `569BBCBB7AA2388D229983D1F124D2DD8DF6C73E915E9120388C3671B63E4316` | Official FAA public advisory circular; research copy. |
| [faa-ac-25-28.pdf](../../bib/certification-safety/faa-ac-25-28.pdf) | FAA, AC 25-28, Compliance for Flight in Icing Conditions | 2014-10-27 | 89 | 824,909 | `E3A88565DD1FAF35E36CF56CD980EE76A0CF24C80EACAD306D9EFD2AD5E9E0CD` | Official FAA public advisory circular; research copy. |
| [faa-ac-25-25a.pdf](../../bib/certification-safety/faa-ac-25-25a.pdf) | FAA, AC 25-25A, Performance and Handling Characteristics in Icing Conditions | 2014-10-27 | 72 | 611,958 | `AC324A9D52681B8108FAC4023CE96019E62EC9E73667C4D1D79906775460C2DF` | Official FAA public advisory circular; research copy. |
| [faa-ac-25-9a.pdf](../../bib/certification-safety/faa-ac-25-9a.pdf) | FAA, AC 25-9A, Smoke Detection, Penetration, Evacuation Tests and Flight Manual Procedures | 1994-01-06 | 28 | 9,699,160 | `76B00251817834995EF80890805822A54AFDD18D12B9AA775452750012F0B822` | Official FAA public advisory circular; research copy. |
| [faa-ac-25-22.pdf](../../bib/certification-safety/faa-ac-25-22.pdf) | FAA, AC 25-22, Certification of Transport Airplane Mechanical Systems | 2000-03-14 | 218 | 1,091,464 | `782A91CBC856D52655AD591BDA5104F0EA960A5034141CED588C022189BE6E43` | Official FAA public advisory circular; research copy. |
| [faa-ac-25-853-1.pdf](../../bib/certification-safety/faa-ac-25-853-1.pdf) | FAA, AC 25.853-1, Flammability Requirements for Aircraft Seat Cushions | 1986-09-17 | 11 | 940,569 | `61D96153BF26B0C3F0975BB5EB2D04F8B03D21A8CB58011D5D43BF7DF22D198B` | Official FAA public advisory circular; research copy. |
| [nasa-sp-2010-580-v1.pdf](../../bib/certification-safety/nasa-sp-2010-580-v1.pdf) | NASA/SP-2010-580, System Safety Handbook, Version 1.0 | 2011 | 120 | 2,248,218 | `A6B537FF573851D7D8A10475F137E94C7DF85882EF928FB0D435BBD751AEB7D2` | NASA NTRS record states Public Use Permitted; retain attribution and source link. |
| [nasa-sp-2016-6105-rev2.pdf](../../bib/certification-safety/nasa-sp-2016-6105-rev2.pdf) | NASA/SP-2016-6105 Rev 2, NASA Systems Engineering Handbook | 2017 | 356 | 4,122,125 | `3153AE2E53E29452D5997EFAFE280A5F05CD21B43A047E988A17E1DD5207A38E` | NASA NTRS record states Public Use Permitted; retain attribution and source link. |
| [dlr-jeyaraj-2022-safety-filtering.pdf](../../bib/certification-safety/dlr-jeyaraj-2022-safety-filtering.pdf) | DLR/AGILE4.0, Systems Architecting: Design Space Modeling and Safety-Based Filtering | 2022 | 16 | 1,661,111 | `BA305DCEA9395654491BF1359DFB168102126A096712D1F4802E1CEBDDE5D180` | DLR repository marks the record Open Access; local research copy, no redistribution assumed. |
| [dlr-bird-strike-challenge.pdf](../../bib/certification-safety/dlr-bird-strike-challenge.pdf) | DLR/MDPI, The Bird Strike Challenge | 2020 | 19 | 344,648 | `A7938DB786B070166EC8293E0BB1289AF0D4EB94C572540F5CFF8D85BD54F1CA` | Open-access publication; publisher record identifies CC BY terms. Retain attribution. |
| [dlr-battery-electric-propulsion-system-design.pdf](../../bib/certification-safety/dlr-battery-electric-propulsion-system-design.pdf) | DLR, System Design and Analysis of a Battery-Electric Propulsion System | 2024 | 46 | 1,408,423 | `57EB7012F8265BBA12FA5779A2937CB390C58302009DD9AFA42F7A501CFB5608` | DLR repository marks Open Access; local research copy, no redistribution assumed. |
| [easa-cs-25-amendment-28.pdf](../../bib/certification-safety/easa-cs-25-amendment-28.pdf) | EASA, Certification Specifications for Large Aeroplanes CS-25 Amendment 28 | 2025-11-20 update | 1,515 | 23,584,619 | `32F1A9ACF26E8CECEB291D206BF07A7071F7F59F40F10C096BB17F34C6A58007` | Official EASA public download; local research copy, no redistribution assumed; check EASA for revisions/corrections. |

### 12.2 Link-only references not downloaded

| Reference | Link | Why link-only |
|---|---|---|
| FAA 14 CFR Part 25 | [eCFR Part 25](https://www.ecfr.gov/current/title-14/chapter-I/subchapter-C/part-25) | Live legal text; the report records the official link rather than freezing a regulatory copy. |
| EASA CS-25 landing page and online rules | [CS-25](https://www.easa.europa.eu/en/document-library/certification-specifications/group/cs-25-large-aeroplanes), [Easy Access Rules](https://www.easa.europa.eu/en/document-library/easy-access-rules/online-publications/easy-access-rules-large-aeroplanes-cs-25) | Live controlled publication; local Amendment 28 copy is separately hashed. |
| ICAO Doc 9859 / Annex 19 / GASP | [ICAO SMM](https://www.icao.int/safety-management/SMI/SMM), [access policy](https://www.icao.int/safety-management/access-icao-annexes-and-guidance), [GASP](https://www.icao.int/GASP) | Controlled/read-only or purchased access; rights and redistribution conditions were not clear for local downloading. |
| SAE ARP4761A, ARP4754B, AIR7209 | [ARP4761](https://saemobilus.sae.org/standards/arp4761-guidelines-methods-conducting-safety-assessment-process-civil-airborne-systems-equipment), [ARP4754](https://saemobilus.sae.org/standards/arp4754a-guidelines-development-civil-aircraft-systems), [AIR7209](https://saemobilus.sae.org/standards/air7209-development-assurance-principles-aerospace-vehicles-systems) | Paid/copyrighted SAE standards; no copy downloaded. |
| RTCA DO-178C/DO-254/DO-160 family | [RTCA DO-178](https://www.rtca.org/do-178/) | Copyrighted/purchased standards; no copy downloaded. |
| Historical FAA AC 25.1309-1A | [FAA PDF](https://www.faa.gov/documentLibrary/media/Advisory_Circular/AC_25.1309-1A.pdf) | Link retained for historical context; current AC 25.1309-1B is the downloaded working reference. |

## 13. Limitations and open decisions

This research deliberately does not choose a single aircraft category, seat count, propulsion technology, operating rule, certification amendment, special condition, authority interpretation, MMEL/MEL philosophy, or quantitative reliability database for ALAS. Those are user/design-brief inputs. The following decisions should be made before implementing a certification-profile translator:

- Which FAA/EASA amendment and operating categories are first-class supported profiles?
- Will ALAS support Part 25/CS-25 only, or also commuter/small-aircraft/UAS/eVTOL bases with different rules?
- What level of preliminary architecture analysis is required before a candidate may enter performance optimisation?
- Which environmental and novel-technology translators are validated sufficiently for physical residuals rather than diagnostics?
- What is the evidence object/version schema, and how will external authority review be recorded?
- What safety data may be used for screening, and how will uncertainty and independence assumptions be shown?
- Which results block concept acceptance versus merely reduce ranking score?

Until those decisions are recorded, the safe default is to fail closed on high-severity architecture gaps, keep quantitative probability rows evidence-pending, and expose all missing applicability or translator decisions to the user.

## 14. Verification of this research artifact

Only the new research directory and this report were added for this task. Existing source, tests, fixtures, and dirty-worktree changes were not modified. Before delivery, re-run the following read-only checks when the repository environment is available:

```powershell
Get-FileHash -Algorithm SHA256 .\bib\certification-safety\*.pdf
Get-ChildItem .\bib\certification-safety\*.pdf | Select-Object Name,Length
```

The hashes and page counts in §12.1 are the values observed during this research run. This document should be reviewed against the live FAA/EASA/ICAO/SAE/RTCA sources before being used to define a certification programme or an actual compliance submission.
