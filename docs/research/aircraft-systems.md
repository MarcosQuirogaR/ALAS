# Conventional transport-aircraft systems architecture and preliminary sizing

Research note for ALAS
Prepared: 2026-08-27
Scope: ECS and pressurisation, electrical generation and distribution, avionics loads, hydraulics and actuation, ice protection, fire protection, APU, water and waste, and aircraft thermal management
Recommended new owner: `alas-systems`, with typed interfaces to geometry, payload, propulsion, mission, mass, aerodynamics, performance, structures, reporting, and acceptance

## Executive conclusions

ALAS should not represent aircraft systems only as fixed operating-empty-mass fractions. The next useful conceptual-design fidelity is a **mission- and failure-case resource network**. Components consume, transform, store, distribute, reject, or isolate electrical power, hydraulic power, pneumatic power, air, heat, water, waste, and fire-extinguishing agent. The governing case sizes each component; the same evaluation returns installed mass and centre of gravity, engine/APU extractions, rejected heat, cooling or ram-air drag, consumables, and unavailable functions.

The existing FLOPS-derived system-mass implementation is worth retaining as a fast Class-I prior, parity reference, and fallback. It should not be deleted or silently presented as a physical architecture. A new `alas-systems` crate should add the physical ledger and compare its result with the legacy estimate.

The minimum credible architecture has five properties:

1. stable, serialisable component and connection identities rather than solver graph indices;
2. explicit normal and failed load cases, including ground operation and transient peak demands;
3. shared mass, power, heat, fluid, drag, volume, station, uncertainty, and provenance contracts;
4. a discrete architecture and reconfiguration layer outside the continuous aircraft fixed point;
5. an evidence status that can report `NotEvaluated` or `Unverified` without converting missing data into a nominal pass.

Subsystem coupling is material at conceptual level. Bleed extraction changes engine performance and supplies packs, starting, and thermal anti-ice. Electrical and hydraulic loads change shaft extraction and fuel. Nearly every consumed watt becomes heat that must be rejected. Cooling air and heat exchangers add drag and installation mass. Cabin layout determines ventilation, heat, water, waste, galley, and lavatory loads. Actuator availability changes usable control authority. Pressure differential and equipment installation affect structure. These feedbacks must converge with mission fuel, propulsion, mass properties, and geometry rather than being added after sizing.

The sources below define requirements, representative architectures, and validation anchors. They do **not** supply all supplier-specific component maps, reliability data, or installation rules needed for a certified aircraft. This note is therefore an implementation and screening plan. It does not claim compliance with CS-25, FAA rules, an aircraft type design, or any approved means of compliance.

## Scope and evidence boundary

The evidence hierarchy used here is:

1. official EASA and FAA material for requirement and safety-objective anchors;
2. official NASA technical reports and software manuals for analysis methods and verification cases;
3. official Airbus and Pratt & Whitney publications for representative transport-aircraft topology and public interface values;
4. DLR institutional-repository research for preliminary design architecture and multidisciplinary integration.

The local EASA and NASA PDFs are repository inputs. Web links point to the official record or official publication. Equations explicitly labelled **proposed ALAS analytical model** are engineering formulations recommended by this note; they are not quotations from, nor prescribed means of compliance in, the regulatory sources. Manufacturer data describe their named products and are validation targets, not universal design constants.

NotebookLM was not used because its authenticated service was unavailable during this review. No result in this note is attributed to NotebookLM.

## 1. Current ALAS baseline

### 1.1 Existing capability

The present systems-mass selector in [`alas-config`](../../crates/alas-config/src/systems_mass.rs) offers `ReferenceCompatibleFractions` and `FlopsTransportV1`. The FLOPS transport path in [`alas-mass`](../../crates/alas-mass/src/flops_transport/equations.rs) estimates surface controls, APU, instruments, hydraulics, electrical equipment, avionics, furnishings, air conditioning, anti-ice, and operating items. This is a useful empirical mass estimator and already covers more system categories than a single undifferentiated fraction.

The current path does not define system sources, buses, pipes, consumers, isolation, load schedules, failure reconfiguration, state inventories, or heat rejection. It cannot answer which load case governs a generator or pack, whether an essential consumer remains supplied after a source failure, how much bleed or shaft power is extracted during a mission segment, or where operating fluids and consumables move the centre of gravity.

The repository's [mass and balance research](mass-balance.md) already identifies system fractions as Class-I priors and calls for a component ledger carrying mass, station, uncertainty, provenance, and state. The [propulsion and energy research](propulsion-energy.md) already requires bleed, accessory, anti-ice, electrical, and thermal loads to feed the propulsion and mission models. The proposal here supplies the missing system-level owner and contracts.

### 1.2 Preserve the baseline as evidence

The first `alas-systems` implementation should preserve existing FLOPS golden outputs. Each physical result should report:

- the bottom-up installed mass;
- the comparable FLOPS or fraction prior when applicable;
- their difference and subsystem allocation;
- the governing load case;
- the model fidelity, evidence source, and uncertainty;
- any quantity that was assumed, unverified, or not evaluated.

FLOPS should remain selectable for fast screening and regression. It must not silently fill a missing physical map inside a nominal high-fidelity run. The fallback should be visible in machine-readable diagnostics.

## 2. Source and evidence register

### 2.1 Requirements and safety objectives

- [EASA CS-25 Amendment 28, local official PDF](../../bib/propulsion-energy/easa-cs-25-amendment-28-2023-correction-2025.pdf) and the [official EASA download record](https://www.easa.europa.eu/en/downloads/139075/en) provide the principal large-aeroplane requirement anchors. Relevant sections include CS 25.831, 25.841, 25.1309, 25.1351, 25.1355, 25.1419, 25.1435, 25.1438, 25.851, 25.857, and 25.858.
- [FAA AC 25.1309-1B](https://www.faa.gov/regulations_policies/advisory_circulars/index.cfm/go/document.information/documentID/1043037) is the active FAA system-design and safety-assessment guidance. It is an analysis-process reference; this conceptual implementation will not claim to complete an aircraft system safety assessment.
- [FAA AC 25.901-1](https://www.faa.gov/documentLibrary/media/Advisory_Circular/AC_25.901-1.pdf) addresses powerplant installation safety assessment, including APU-related installation considerations.
- [FAA AC 25.1455-1](https://www.faa.gov/regulations_policies/advisory_circulars/index.cfm/go/document.information/documentID/22688) addresses drain systems and ice effects. It is relevant to water/waste discharge and freezing hazards, not a potable- or waste-capacity sizing rule.
- The [US EPA Aircraft Drinking Water Rule](https://www.epa.gov/dwreginfo/aircraft-drinking-water-rule) establishes drinking-water operational/public-health requirements for covered aircraft water systems. It does not prescribe a universal litres-per-passenger design allowance.

### 2.2 Analysis methods and architecture research

- The [NASA FLOPS documentation](https://ntrs.nasa.gov/citations/20170005851) is the provenance reference for the existing empirical transport-aircraft system mass path.
- NASA's [thermal-management-system design report, local PDF](../../bib/propulsion-energy/nasa-tm-20205011477-tms.pdf) and [NTRS record](https://ntrs.nasa.gov/citations/20205011477) couple liquid loops, compact plate-fin heat exchangers, pumps, fans, ram air, mass, power, and drag. The report warns that sizing depends on heat load, temperature limits, Mach number, and altitude; simple regression fits do not generalise across architectures.
- NASA's [hybrid AC/DC load-flow formulation](https://ntrs.nasa.gov/citations/20190004941) uses an object-oriented network, analytic derivatives, and verification against a published 13-bus case. It is an appropriate numerical pattern and electrical regression target.
- NASA's [generalised aircraft power-system architecture](https://ntrs.nasa.gov/citations/20180005332) supports topology-based analysis rather than one aggregate electrical load.
- [NASA LEWICE 3.2](https://ntrs.nasa.gov/citations/20080048307) documents icing analysis including electrothermal and bleed-air protection. [LEWICE 3.5 validation](https://ntrs.nasa.gov/citations/20170000242) provides a higher-fidelity comparison boundary. ALAS should exchange protected-zone geometry and conditions with such a validated solver; it should not imitate LEWICE with an undocumented scalar penalty.
- The [DLR function-oriented aircraft-system architecture dissertation](https://elib.dlr.de/216894/) separates power generation, transformation/distribution, and consumers, and evaluates candidate architectures for performance, safety/certification screening, and maintenance. Its function-first decomposition is directly useful for typed ALAS graphs.
- The [DLR/POLITO ASTRID aircraft-system MDO paper](https://elib.dlr.de/110981/1/Assessment%20of%20airframe-subsystems%20synergy%20on%20overall%20aircraft%20performance%20in%20a%20Collaborative%20Design%20Environment_Aviation%202016%20.pdf) sizes avionics, flight controls, landing gear, anti-ice, ECS, fuel, pneumatic, hydraulic, and electrical systems with mission-segment power budgets and iterative airframe/system convergence through CPACS. It supports the proposed coupled workflow, but its reported architecture results are not reusable supplier sizing laws.
- [DLR research on electrical architectures](https://elib.dlr.de/218584/) and [automatic fuselage system layout](https://elib.dlr.de/76979/) support explicit topology, installation, and routing models at later fidelity.

### 2.3 Representative public aircraft data

- Airbus [FAST 51](https://aircraft.airbus.com/sites/g/files/jlcbta126/files/2022-04/FAST51.pdf) documents the A320-family three-circuit hydraulic architecture and consumer allocation. Airbus's [A320 dual-hydraulic-loss safety article](https://safetyfirst.airbus.com/app/themes/mh_newsdesk/documents/archives/a320-dual-hydraulic-loss.pdf) identifies the nominal 3000 psi, approximately 20.684 MPa, circuits and provides a failure-behaviour anchor.
- Airbus [FAST 45](https://www.aircraft.airbus.com/sites/g/files/jlcbta126/files/2022-04/FAST45.pdf) documents a representative two-pack and bleed-consumer topology. The Airbus [cabin-air-quality article](https://www.aircraft.airbus.com/en/newsroom/news/2021-01-cabin-air-quality-key-to-a-comfortable-flight) describes conventional engine bleed being pre-cooled before the packs. These values characterise an A320 architecture, not every transport.
- Airbus [FAST 62](https://aircraft.airbus.com/sites/g/files/jlcbta126/files/2021-08/airbus-fast62-october2018.pdf) contains an A320 hot-soak cabin-cooling example: a 45 degC outdoor condition, about 1.0 kg/s of inlet air at -10 degC, and published cabin cool-down times. This is a useful transient ECS fixture.
- The [Airbus A320 aircraft-characteristics landing page](https://www.aircraft.airbus.com/en/customer-care/fleet-wide-care/airport-operations-and-aircraft-characteristics/aircraft-characteristics) and [July 2026 A320 ACAP PDF](https://mediaassets.airbus.com/pm_38_916_916266-iujedqawwy.pdf?fileName=aca32001-jul-2026-2.pdf) publish representative interfaces including 200 L usable potable water, 177 L usable waste capacity, 3.45 bar filling pressure, and a 90 kVA ground electrical interface.
- Pratt & Whitney's [APS3200 product page](https://www.prattwhitney.com/en/products/auxiliary-power-units) states that the A320-family APU can provide bleed air for conditioning and engine start concurrently with 90 kVA electrical power. This is a simultaneous-load validation point, not a generic APU rating rule.

## 3. Proposed typed contracts

### 3.1 Architecture root

Create a serialisable architecture owned by `alas-systems`:

```rust
pub struct SystemsArchitecture {
    pub schema_version: SchemaVersion,
    pub components: Vec<ComponentSpec>,
    pub connections: Vec<ResourceConnection>,
    pub installations: Vec<InstallationSpec>,
    pub schedules: Vec<LoadSchedule>,
    pub provenance: Provenance,
}
```

IDs should be stable newtypes such as `ComponentId`, `PortId`, `ConnectionId`, `ZoneId`, and `CaseId`. Never serialise `petgraph` node indices. The configuration representation should be a closed, versioned enum rather than generic strings or serialised trait objects:

```rust
pub enum ComponentSpec {
    Generator(GeneratorSpec),
    Bus(BusSpec),
    Battery(BatterySpec),
    Pump(PumpSpec),
    Accumulator(AccumulatorSpec),
    Actuator(ActuatorSpec),
    Pack(PackSpec),
    OutflowValve(OutflowValveSpec),
    HeatExchanger(HeatExchangerSpec),
    Apu(ApuSpec),
    Detector(DetectorSpec),
    AgentBottle(AgentBottleSpec),
    WaterTank(WaterTankSpec),
    WasteTank(WasteTankSpec),
}
```

An internal `ComponentModel` trait can evaluate a typed component, but public configuration and result schemas should remain explicit enums so that invalid combinations are rejected during deserialisation and migrations remain reviewable.

### 3.2 Resources and ports

```rust
pub enum ResourceKind {
    AcElectric { voltage_v: ElectricPotential, frequency_hz: Frequency, phases: u8 },
    DcElectric { voltage_v: ElectricPotential },
    Hydraulic { fluid: HydraulicFluid, design_operating_pressure_pa: Pressure },
    Pneumatic,
    CabinAir,
    ThermalLiquid { fluid: ThermalFluid },
    PotableWater,
    WasteWater,
    FireAgent { agent: FireAgent },
}

pub struct ResourcePort {
    pub id: PortId,
    pub resource: ResourceKind,
    pub direction: PortDirection,
    pub capacity: Option<ResourceCapacity>,
}

pub struct ResourceConnection {
    pub id: ConnectionId,
    pub from: PortRef,
    pub to: PortRef,
    pub route: RouteSpec,
    pub isolation: IsolationPolicy,
    pub provenance: Provenance,
}
```

`ResourceState` should carry the variables relevant to its kind: voltage/current/frequency and real/reactive power for electrical; pressure, temperature, and mass or volume flow for fluid resources; temperature and heat rate for thermal ports; remaining mass and usable volume for stored resources. Units should extend the repository's typed `alas-units` newtypes. Introducing another unit package in the core schema would duplicate the existing unit policy.

### 3.3 Case, schedule, result, and mass contracts

```rust
pub struct SystemLoadCase {
    pub id: CaseId,
    pub mission_state: MissionState,
    pub duration: Time,
    pub occupants: OccupantState,
    pub atmosphere: AtmosphereState,
    pub cabin_target: CabinTarget,
    pub icing: IcingCondition,
    pub active_engines: EngineAvailability,
    pub apu_state: ApuCommand,
    pub failures: Vec<FailureEvent>,
}

pub struct LoadSchedule {
    pub consumer: ComponentId,
    pub case: CaseSelector,
    pub demand: ResourceDemand,
    pub duty_cycle: Ratio,
    pub transient_duration: Option<Time>,
    pub priority: LoadPriority,
    pub failure_class: EvidenceClass,
    pub rejected_heat_zone: Option<ZoneId>,
}

pub struct CaseResult {
    pub id: CaseId,
    pub balances: Vec<ResourceBalance>,
    pub unsupplied_loads: Vec<UnsuppliedLoad>,
    pub shaft_extraction: Vec<ShaftExtraction>,
    pub bleed_extraction: Vec<BleedExtraction>,
    pub rejected_heat: Vec<ZoneHeatLoad>,
    pub cooling_drag: Force,
    pub mass_items: Vec<InstalledMassItem>,
    pub residuals: Vec<RequirementResidual>,
    pub status: EvidenceStatus,
}
```

`InstalledMassItem` must distinguish dry hardware, installation allowance, trapped or operating fluid, mission consumable, and payload-service consumable. It should carry station, optional inertia, capacity basis, governing case, uncertainty, model version, and provenance. This prevents water, extinguishing agent, hydraulic fluid, and trapped working fluid from being double counted or hidden inside an empirical operating-items term.

Use an evidence status compatible with the repository's staged-result semantics but more specific at quantity level:

```rust
pub enum EvidenceStatus {
    Evaluated,
    Screened,
    Unverified,
    NotEvaluated,
    Failed,
}
```

`Screened` means that a declared preliminary method was executed. It does not mean compliant. `NotEvaluated` means no suitable model or evidence was available; it is not a zero load, zero mass, or passed residual.

## 4. Required load and failure cases

Architecture sizing must evaluate named cases instead of taking a single utilisation factor. The initial standard set should contain:

### Normal cases

1. cold-soak ground start: batteries, APU start, pumps, avionics boot, heating, and engine start;
2. hot-soak full-occupancy turnaround: packs, recirculation, avionics, galleys, ground power/APU, and water servicing;
3. main-engine start, including simultaneous pneumatic and electrical demand where the architecture permits it;
4. taxi, with explicit one-engine, all-engine, APU, and external-power policies;
5. sea-level and hot/high take-off with high-lift actuation, anti-ice policy, peak generator or bleed extraction, and brake standby;
6. climb at the selected maximum icing condition;
7. cold/design cruise with pressurisation, minimum fresh-air supply, avionics, galley duty cycles, and steady thermal rejection;
8. idle descent, where low engine power can constrain bleed, generator, and hydraulic margins;
9. approach and landing with gear, high-lift, flight-control, braking, spoiler, and reverse-related loads;
10. post-landing and turnaround with brake cooling, cabin service, and tank inventory updates.

### Failure and reconfiguration cases

1. one engine and its driven sources unavailable;
2. one main generator or normal electrical source unavailable;
3. loss of the normal electrical network requiring the alternate source and load shedding;
4. one hydraulic circuit unavailable, including reduced actuator rate or function availability;
5. one pack or bleed source unavailable;
6. APU unavailable during ground, start, and backup cases;
7. one outflow or relief valve unavailable;
8. bleed duct burst or excessive leak with automatic isolation;
9. fire isolation that intentionally removes electrical, pneumatic, hydraulic, or fuel paths;
10. anti-ice source or protected-zone failure.

Every case needs duration and sequence, because a battery, accumulator, agent bottle, cabin pressure transient, or potable-water tank cannot be sized from steady power alone. Common-cause failures, zonal hazards, dispatch relief, and probabilistic independence remain later evidence tasks; the preliminary model must not imply that enumerating these cases completes CS 25.1309 analysis.

## 5. Subsystem physics and couplings

### 5.1 ECS and pressurisation

CS 25.831 supplies a quantitative normal-operation floor of 0.25 kg of fresh air per minute per occupant. CS 25.841 supplies the cabin-altitude and pressure-relief anchors: under normal operation cabin pressure altitude is limited to 2438 m at the maximum operating altitude; for aircraft operating above 7620 m, reasonably probable failures must not expose occupants to cabin pressure altitude above 4572 m. It also requires two pressure-relief valves sized so that failure of either does not appreciably increase differential pressure. These are test anchors, not a complete pack or fuselage sizing method.

For `N_occ` occupied seats, the minimum normal fresh-air mass flow is:

```text
mdot_fresh_min = N_occ * 0.25 / 60                  [kg/s]
```

The coefficient is sourced from CS 25.831. The following equations are a **proposed ALAS analytical model**:

```text
dm_c/dt       = mdot_in - mdot_out - mdot_leak
p_c           = m_c R_air T_c / V_c
m_c cp dT_c/dt = Q_occupants + Q_equipment + Q_solar
                 + UA (T_ambient - T_c)
                 + mdot_in h_in - (mdot_out + mdot_leak) h_c
dm_v/dt       = mdot_v,in + mdot_v,occupants - mdot_v,out - mdot_condensed
```

Outflow and leakage should use a compressible-orifice relation with choked and unchoked branches. Packs should expose supply mass flow, supply temperature, shaft/electrical/bleed demand, pressure loss, and rejected heat. The cabin equation uses pack-conditioned inlet enthalpy; a separate `Q_pack` term must not also be subtracted unless a pack heat exchanger is physically inside the cabin control volume. A first implementation may use calibrated maps or response surfaces, but it must key them by ambient state, source pressure/temperature, commanded flow, and pack condition. A constant pack power or constant bleed fraction is only a fallback.

Cabin nodes should include at least flight deck, occupied cabin zones, cargo zones when conditioned, electronics bay, and equipment/service zones. Geometry supplies pressurised volume, wetted/transparent area, zone adjacency, and leakage assumptions. Payload supplies occupants and activity schedules. Electrical and avionics models supply heat. The pressure differential goes to structures. Pack bleed or electrical power goes to propulsion. Ram-air and exhaust losses go to aerodynamics.

The Airbus A320 two-pack topology and FAST 62 transient should be implemented as representative fixtures. Passing those fixtures demonstrates internal consistency against public A320 data, not universal ECS validation.

### 5.2 Electrical generation, distribution, and avionics loads

CS 25.1351 requires generating capacity and source count to be established by an electrical load analysis and requires source failures not to impair the remaining sources' ability to supply essential loads. It also calls for an alternate, high-integrity supply for functions needed for controlled flight and safe landing unless total loss is shown extremely improbable. CS 25.1355 addresses separate sources and feeders where two independent supplies are needed. ALAS should translate these into network reachability, capacity, load-shedding, and independence screening; it cannot establish the probability clauses without reliability and common-cause evidence.

The electrical graph should represent engine generators, APU generator, external power, batteries, rectifier/transformer units, converters/inverters, AC and DC buses, contactors, feeders, protection, and consumers. Avionics should initially be a declared equipment/load inventory, not an unexplained residual mass. Each item needs nominal and peak real power, reactive or apparent power where relevant, bus voltage/frequency, boot/inrush duration, duty cycle, dispatch state, shedding priority, heat destination, and installation mass.

The following are **proposed ALAS analytical equations** for preliminary conductor and conversion sizing:

```text
I_dc       = P / (V eta)
I_ac,3ph   = S / (sqrt(3) V_line)
P_feeder   = I^2 R
DeltaV     = I R
Q_heat     = P_input - P_delivered
```

Conductor area is governed by current capacity, allowable voltage drop, temperature, installation bundle/route assumptions, and protection. Route length and installation zone must therefore come from geometry/layout rather than a single aircraft-level wire-mass fraction.

Use nonlinear or linearised load flow according to selected fidelity. The NASA hybrid AC/DC object model is a suitable high-fidelity numerical reference, including its published 13-bus verification case. Essential-load reachability and capacity can be checked before load flow. Every converter and feeder loss becomes a zone heat input. Generator extraction and APU fuel demand return to propulsion.

### 5.3 Hydraulics and actuation

The hydraulic network should represent engine- or electrically driven pumps, reservoirs, accumulators, pressure regulation, filters, isolation valves, pipes, and actuator consumers. Control-surface, landing-gear, steering, braking, spoiler, thrust-reverser, and door functions should refer to actuators by ID. A function can have multiple actuators or resource paths; after a failure, the available force, rate, travel, and energy must be returned to stability, performance, and landing analyses.

The following are **proposed ALAS analytical equations**:

```text
F_actuator   = DeltaP A_piston eta_mech
Q_actuator   = A_piston v / eta_vol
P_pump       = DeltaP Q / eta_pump
Qdot_loss    = P_pump - P_useful
p_acc V_acc^n = constant
```

The governing load is often a short simultaneous actuation or repeated-cycle case, not average cruise demand. Actuator kinematics supply stroke, rate, and hinge/gear load; system pressure and efficiency determine flow and power. Pump and line losses add thermal load.

CS 25.1435 supplies design-test anchors for pressurised hydraulic elements. The cited amendment gives proof/ultimate design operating pressure multipliers including 1.5/3.0 for tubes and fittings, 2.0/4.0 for hoses, and 1.5/2.0 for other elements, with distinct vessel categories. It also requires consideration of structural, thermal, fatigue, and environmental effects. ALAS may calculate the corresponding screening pressures but must not claim component qualification.

CS 25.1438 makes clear that engine bleed, ECS, pressurisation, starting, and hot-air ice protection are part of the relevant pneumatic-system scope and that burst or excessive leakage matters. These failure paths belong in the same topology/reconfiguration framework.

Use the public A320 three-circuit, approximately 20.684 MPa architecture as a topology and unit regression fixture. Do not infer universal circuit count or pressure from it.

### 5.4 Ice protection

Ice protection is a protected-zone problem. Each wing, tail, inlet, probe, windscreen, propeller/rotor, or drain zone needs protected area, collection efficiency, heat or fluid source, activation policy, and failure state. CS 25.1419 requires analysis supported by test and, where necessary, flight evidence across the relevant icing envelope. A conceptual heat balance alone cannot demonstrate this.

The following are **proposed ALAS screening equations**:

```text
mdot_water = LWC V beta A_protected
Qdot_req   = Qdot_warm + Qdot_freeze_or_melt + Qdot_evap
             + Qdot_convection + Qdot_conduction + Qdot_radiation
```

Electrothermal zones consume bus power and reject heat. Bleed-air zones consume pneumatic enthalpy, produce duct losses/leak hazards, and reduce engine performance. Fluid systems consume a finite inventory and add tank/pump mass. If protection is unavailable, ALAS should pass an icing degradation state to aerodynamics and propulsion rather than silently retaining clean performance.

LEWICE should be the external high-fidelity boundary for ice accretion/protection analysis. ALAS owns mission conditions, protected geometry, source availability, and aircraft coupling; the validated icing tool owns detailed local ice physics. Until that integration exists, results are `Screened` or `NotEvaluated`, not compliance findings.

### 5.5 Fire and smoke protection

The preliminary fire model should represent detection zones, detector loops, extinguishing zones, agent bottles, discharge paths, isolation commands, and the resources removed by isolation. CS 25.851 supplies portable-extinguisher count and placement anchors: for example, 7–30 passenger seats require one cabin extinguisher, 31–60 require two, and 61–200 require three, in addition to the section's flight-deck, galley, and accessible-compartment provisions. CS 25.857 and 25.858 govern cargo-compartment classification and detection aspects.

ALAS can implement deterministic checks for declared extinguisher inventory, detector/zone coverage, bottle inventory, discharge routing, isolation reachability, and the aircraft-level consequences of shutting down a source or duct. It should calculate mass, station, volume, and service/replacement state for agent bottles and portable units.

Agent concentration, discharge distribution, smoke transport, detector performance, hidden-fire propagation, flammability, zonal safety, and suppression effectiveness require validated tests and specialist models. No generic mass-per-volume equation should be used to report fire protection as adequate. Those results remain `NotEvaluated` unless supplied by accepted component/installation evidence.

### 5.6 APU and ground power

The APU is a multi-output source: electrical power, pneumatic flow/enthalpy, and fuel/thermal consequences. Its rating must be evaluated over ground conditioning, engine start, simultaneous electrical-plus-pneumatic use, hot/high operation, in-flight backup where permitted, and unavailable-APU cases. An aggregate APU mass correlation can seed the design, but source capacity requires a load/altitude/temperature map.

The **proposed ALAS preliminary rating rule** is to size to the maximum simultaneous accepted electrical load plus pneumatic or shaft-equivalent demand across declared cases, subject to an APU capability map:

```text
margin_case = capability(T_amb, altitude, spool_state)
              - simultaneous_demand_case
```

The Pratt & Whitney APS3200 public statement of 90 kVA concurrent with conditioning/start bleed is a representative validation point. The Airbus 90 kVA ground electrical interface is a separate ground-service anchor. Neither establishes the complete APU operating map, installation drag, start envelope, emissions, or fireworthiness.

### 5.7 Potable water and waste

Water and waste are mission state inventories coupled to cabin layout and turnaround policy. Payload defines passenger/crew count, galleys, lavatories, and service events. Geometry defines tanks and routing. Mass consumes changing tank mass and centre of gravity. Thermal/icing logic identifies freeze-risk zones. Ground operations define fill, drain, and servicing cases.

The following is a **proposed ALAS state model**:

```text
m_potable[k+1] = m_potable[k] - sum_i(use_i[k])
m_waste[k+1]   = m_waste[k] + r_return sum_i(use_i[k])
                 + m_lavatory_generated[k]
```

Consumption rates are airline/mission policy inputs with uncertainty, not universal certification constants. The EPA rule is an operational water-quality boundary. FAA drain-icing guidance informs external drain and freezing hazards. The July 2026 A320 ACAP values of 200 L usable potable and 177 L usable waste capacity are one public transport-aircraft fixture, not design rules.

The physical ledger must distinguish tank dry mass, initial contents, minimum reserve, unusable/trapped contents, mission use, waste generated, and turnaround servicing. Capacity violations should be explicit residuals.

### 5.8 Integrated thermal management

Thermal management should be a shared network rather than separate cooling allowances within propulsion, avionics, ECS, and electrical models. Heat sources include avionics, converters, feeders, batteries, pumps, motors, galleys, occupants, solar loading, propulsion accessories, and rejected pack heat. Sinks include fuel, ambient/ram air, skin, cabin exhaust, and dedicated heat exchangers.

The following are **proposed ALAS analytical equations**:

```text
C_i dT_i/dt = sum_j G_ij (T_j - T_i) + Qdot_i
Qdot_loop    = mdot cp DeltaT
Qdot_hx      = epsilon C_min DeltaT_max
```

Nodes require temperature limits and heat capacity; links require conductance or mass flow and pressure loss. Pumps and fans consume power and add heat. Heat exchangers, ducts, scoops, and exhausts add mass and drag. The NASA thermal-management report provides the appropriate coupled mass/power/drag philosophy and a simple-loop regression target, while explicitly showing why a single universal specific-mass fit is inadequate.

## 6. Aircraft-level closure

Every evaluated case should close the following balances within declared tolerances:

- electrical real power, reactive/apparent capacity where modelled, stored energy, and losses;
- hydraulic flow and power, including accumulator state and heat loss;
- pneumatic mass, pressure, enthalpy, leakage, and extraction;
- cabin dry-air mass, moisture, pressure, temperature, and outflow;
- thermal energy by zone and sink;
- potable water, waste, and fire-agent inventory;
- dry installed mass, operating fluid, consumables, station, and optional inertia.

The systems evaluator returns, at minimum:

```text
systems -> mass/CG/inertia, occupied volume, route envelopes
systems -> shaft power and bleed extraction by engine/APU and case
systems -> rejected heat by zone and case
systems -> cooling and installation drag
systems -> available actuation and protected functions after failures
systems -> consumable histories
systems -> requirement residuals, evidence status, and governing cases
```

These outputs couple to the aircraft iteration:

```text
geometry + payload + mission cases
              -> system topology and demands
              -> component sizing and reconfiguration
              -> mass + extraction + heat + drag + availability
              -> propulsion + mission + performance + structure
              -> updated fuel, mass, geometry and mission states
              -> repeat until converged
```

The convergence criterion must include system mass, peak source rating, cruise extraction, maximum zone temperature, cabin pressure/temperature residual, and mission consumable state, not only take-off mass.

## 7. Crate ownership

| Crate | Recommended responsibility |
| --- | --- |
| `alas-config` | Architecture choices, technology assumptions, source selections, schedules, policies, uncertainties, and provenance. No solved states. |
| new `alas-systems` | Typed resource graph, component models, load-case evaluation, failure isolation/reconfiguration, preliminary sizing, resource balances, and system result schema. |
| `alas-units` | Extend typed SI quantities required by resource states; retain the repository's unit strategy. |
| `alas-geom` | Pressurised and conditioned volumes, zone areas, protected areas, equipment stations, route lengths, and installation envelopes. |
| `alas-payload` | Occupants, crew, cabin zones, galley/lavatory inventory, service schedules, water use, and waste-generation events. |
| `alas-prop` | Engine/APU generator and pump extraction, bleed states, APU/fuel maps, and propulsion effects. |
| `alas-mission` | Phase timeline, duration, atmosphere, configuration, control policy, system state propagation, and consumable history. |
| `alas-mass` | Consume `InstalledMassItem`; retain FLOPS as prior/calibration/fallback and expose comparison, not double counting. |
| `alas-aero` | Icing degradation, cooling/ram-air drag, and external installation drag. |
| `alas-stab` / `alas-perf` | Consume failed-system actuator force/rate/availability and protected-function state. |
| `alas-struct` | Consume pressure differential, equipment/line installation loads, and declared containment/load cases. |
| `alas-pipeline` | Orchestrate the discrete architecture pass and continuous fixed point; aggregate status and residuals. |
| `alas-report` | Load tables, resource-flow/Sankey figures, thermal histories, cabin transients, consumables, governing cases, failures, and evidence status. |
| new `alas-acceptance`, or `alas-pipeline::acceptance` until extraction | Published fixtures, conservation/property tests, determinism tests, and evidence thresholds. |

CPACS import/export belongs at the pipeline boundary. The DLR ASTRID work supports CPACS as an exchange mechanism, but the internal Rust contracts should remain more strongly typed than generic XML fields.

## 8. Solver and parallel strategy

The recommended evaluation order is:

1. materialise geometry, cabin zones, mission points, durations, icing conditions, and failure cases;
2. construct immutable typed resource graphs from a candidate discrete architecture;
3. run topology validation and pre-solve reachability/isolation checks;
4. evaluate every normal and failure case;
5. select governing cases and size components, routes, storage, and cooling;
6. return mass, extraction, heat, drag, volume, availability, and residuals;
7. rerun propulsion, mission, mass, performance, and geometry until the coupled criteria converge.

Architecture enumeration, redundancy choices, bus/circuit assignment, and load-shedding policy are discrete. Keep them outside the smooth inner fixed point or optimiser. Within one architecture, component ratings and continuous aircraft variables can be iterated.

Recommended Rust libraries and patterns:

- `petgraph` for typed topology, reachability, cut/isolation analysis, and deterministic traversal over stable external IDs;
- `rayon` for independent load cases, failure cases, architecture seeds, and uncertainty samples;
- `indexmap` or explicit sorting by stable ID for deterministic serialisation and reports;
- the existing `alas-math` solvers for nonlinear balances and fixed points;
- sparse linear algebra only after profiling shows that network size justifies it.

Parallel tasks must be pure with respect to shared architecture inputs. Gather results by `CaseId` and sort before reduction so serial and Rayon runs are byte-stable where floating-point reduction order is controlled. Never parallelise a stateful mission sequence whose battery, water, waste, agent, accumulator, or thermal state depends on the preceding segment; parallelise independent sequences or scenarios instead.

Cache geometry, route, and immutable topology calculations by a content hash that includes schema version, architecture, case inputs, and component-model versions. Do not cache across changed provenance or fidelity.

## 9. Validation campaign

### 9.1 Regression and contract tests

- preserve every current FLOPS systems-mass golden equation and preset;
- round-trip serialise all architecture/resource/component enums with schema versions;
- reject duplicate IDs, incompatible port resources, impossible connection directions, missing schedules, negative capacities, and unknown model versions;
- verify serial/Rayon determinism and stable governing-case selection;
- require an evidence status and provenance for every capacity, mass, and pass/fail residual.

### 9.2 Conservation and graph properties

- every supplied resource terminates at a consumer, storage-state change, declared loss, or solver residual;
- no isolated source or connection remains available after its failure event;
- every essential unsupplied load appears in `unsupplied_loads` and in requirement residuals;
- source capacity is never reused on two paths without a conservation balance;
- dry mass, operating fluid, and consumables remain mutually exclusive ledger classes;
- successive water/waste, battery, accumulator, pressure, and thermal states conserve inventory or energy within tolerance.

### 9.3 Requirement-anchor tests

- CS 25.831 fresh-air calculation at several occupant counts;
- CS 25.841 normal and declared failure cabin-altitude residuals, plus single-relief-valve-unavailable topology;
- CS 25.1351 source-loss and alternate-source/load-shedding reachability cases;
- CS 25.1355 declared dual-independent-source path-disjointness screen;
- CS 25.1435 proof/ultimate pressure multipliers by component category;
- CS 25.851 portable-extinguisher count bins and declared placement coverage;
- CS 25.1438 bleed burst/leak isolation with downstream load consequences.

These are automated readings of selected clauses. They are not findings of compliance.

### 9.4 Published numerical and topology fixtures

- NASA published 13-bus hybrid AC/DC load-flow case;
- NASA simple liquid-loop/heat-exchanger case with mass, power, and drag outputs;
- Airbus A320 three approximately 20.684 MPa hydraulic circuits and representative consumer allocation;
- Airbus A320 two-pack/bleed topology;
- Airbus FAST 62 hot-soak transient and reported cool-down times;
- Pratt & Whitney APS3200 concurrent 90 kVA and bleed operating point;
- Airbus ACAP 200 L potable, 177 L waste, and 90 kVA ground-interface data;
- LEWICE public icing/protection cases at the external-tool boundary.

Each fixture must state exactly what is compared, the source edition/date, tolerance, and what remains unmodelled. Matching one published point validates that point and code path only.

### 9.5 Aircraft-level acceptance

Run at least one conventional narrow-body baseline through all normal and deterministic failure cases. Acceptance requires:

- converged aircraft/system fixed point;
- no hidden unsupplied essential load;
- resource and inventory closure;
- physical mass ledger with FLOPS comparison;
- identified governing cases for every source and storage component;
- explicit cooling drag, bleed, shaft/electrical extraction, and rejected heat;
- no `NotEvaluated` item reported as a passed requirement.

## 10. Delivery order

1. **Contracts and adapter:** introduce `alas-systems`, stable IDs, resources, component enums, cases, results, provenance, mass items, and a FLOPS adapter without changing legacy answers.
2. **Electrical, avionics, and APU:** implement topology, capacity, priority/load shedding, feeder/converter losses, batteries, basic load flow, generator/APU extraction, and the NASA/A320/APS3200 fixtures.
3. **Hydraulics and actuation:** implement pressure/flow/power balances, accumulators, transient demand aggregation, function-to-actuator mapping, circuit failures, and proof/ultimate screening values.
4. **ECS, pressurisation, and thermal:** implement zone mass/energy/moisture dynamics, compressible outflow/leakage, pack map interface, heat-source aggregation, liquid/ram-air sinks, and hot-soak validation.
5. **Ice, fire, water, and waste:** implement protected-zone/source accounting, external LEWICE contract, detection/agent/isolation inventory, mission consumable state, and service cases.
6. **Aircraft fixed point:** connect mass, station, bleed, shaft/electrical extraction, heat, cooling drag, actuation availability, and consumables to propulsion, mission, performance, structure, and geometry.
7. **Evidence and reporting:** add acceptance fixtures, uncertainty samples, CPACS exchange, governing-case tables, resource-flow figures, cabin/thermal histories, failure reports, and optimiser residuals.

The first vertical slice should be a narrow-body case with one normal cruise point and one source-failure point flowing from configuration through `alas-systems` into mass, propulsion extraction, heat, and a machine-readable report. Expand the case set only after the cross-crate contract is stable.

## 11. Fidelity limits and `NotEvaluated` rules

The following are not supported by the public evidence assembled here and must not be invented as high-confidence defaults:

- supplier component dry masses, efficiencies, transient maps, derating, control laws, and reliability data;
- detailed wiring, duct, pipe, bracket, segregation, clearance, and maintenance-access routing;
- complete engine generator, hydraulic pump, pack, heat-exchanger, battery, and APU operating maps;
- full avionics equipment inventory, inrush, cooling allocation, dispatch state, and installation qualification;
- validated fire-agent concentration/distribution, detector response, smoke transport, flammability, and hidden-fire behaviour;
- common-cause, zonal-hazard, probability, independence, latent-failure, maintenance, and dispatch evidence;
- validated ice-accretion/protection performance over the certification envelope without LEWICE or equivalent analysis and test correlation;
- rapid-decompression injury/oxygen, fuselage leakage, and relief-valve dynamics without validated geometry and component data;
- battery thermal runaway propagation and containment;
- potable-water demand as a universal per-passenger constant;
- emissions, environmental, electromagnetic, lightning, HIRF, software/hardware assurance, and equipment qualification.

Rules for the implementation:

1. missing mandatory input or component map yields `Unverified` or `NotEvaluated`, never zero;
2. a FLOPS correlation substituted for a physical component model yields a named fallback diagnostic;
3. a requirement excerpt produces a residual or screening status, never a `Certified` or `Compliant` enum;
4. manufacturer public data validate only the named aircraft/product and operating point;
5. DLR certification filters may inspire early architecture rejection but are not authority findings;
6. every report must separate `Evaluated`, `Screened`, `Unverified`, `NotEvaluated`, and solver `Failed`;
7. optimisation cannot improve a candidate by omitting a `NotEvaluated` mass, load, or constraint; apply a declared conservative prior or reject the candidate, and report which policy was used.

The practical fidelity target is **conceptual physical closure with traceable evidence**, not detailed design or certification. Achieving that target would nevertheless be a major increase in physical significance over independent empirical system-weight terms because the same architecture would explain its mass, power, heat, drag, resource availability, and governing aircraft cases.
