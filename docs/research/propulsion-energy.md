# Propulsion and energy modeling for requirements-first aircraft design

Research report for ALAS-rust
Prepared: 2026-08-26
Scope: conceptual aircraft synthesis, propulsion/airframe coupling, and energy-system screening
Repository owner boundary: alas-prop, with interfaces to alas-config, alas-mission, alas-perf, alas-mass, alas-aero, alas-struct, alas-opt, and alas-report

## Executive conclusion

The recommended ALAS propulsion architecture is a requirements-traceable, multi-fidelity propulsion contract with a discrete architecture stage outside the continuous aircraft optimizer.

The contract should:

- enumerate propulsion families and energy carriers before continuous sizing;
- evaluate a named set of operating points and failure/load cases, not only one cruise point;
- distinguish uninstalled gross performance, installed net performance, and aircraft-level propulsion drag;
- represent fuel, battery, hydrogen, shaft power, bus power, thermal rejection, and reserve as conserved quantities with explicit volume, mass, and centre-of-gravity effects;
- return residuals and provenance for every result;
- keep the current scalar and parametric models as fast screening fallbacks, but label them as such;
- promote map-based steady off-design decks and installation corrections to the main conceptual-design fidelity;
- defer transient handling, detailed component life, certification compliance, and environmental certification to later evidence stages.

The most important design change is replacing a single thrust-lapse number with a load-case evaluator. A scalar lapse can remain useful for a cheap first screen, but it cannot represent altitude, Mach, throttle, hot-day, installation, engine-out, bleed/extraction, variable geometry, or hybrid power-split effects at the same time.

The current repository already contains useful pieces: a closed-form two-spool turbofan cycle, a station-based mission turbofan network, an engine/nacelle catalogue, and a scalar OEI matching-chart model. The research recommendation is additive. Preserve those paths for parity and introduce a typed propulsion model behind a common evaluator rather than making the first requirements-first implementation depend on a high-fidelity engine code.

This report is a conceptual-design recommendation. It is not a type-certificate compliance statement, an engine approval, a noise certification calculation, an emissions certification calculation, or a substitute for manufacturer data, test data, or authority-approved means of compliance.

## Scope, method, and interpretation

The review covers:

1. engine architecture selection and coupled engine-airframe sizing;
2. thermodynamic cycle models, component maps, steady-state decks, and off-design matching;
3. thrust lapse and installed performance;
4. nacelle, inlet, pylon, plume, boundary-layer-ingestion, and acoustic integration;
5. engine-out and powerplant-installation safety boundaries;
6. turbofan, geared turbofan, turboshaft, turboprop, open-rotor, turboelectric, hybrid-electric, battery-electric, hydrogen, and SAF assumptions;
7. fuel/energy mass, volume, reserve, power, thermal, and centre-of-gravity closure;
8. requirements-first typed interfaces, operating points, residuals, and architecture seeds.

CEA in this report means NASA Glenn Chemical Equilibrium with Applications. CEA is a thermochemistry/equilibrium-property tool. It can support a combustion-property or fuel-screening model, but it is not, by itself, a turbofan component map, an engine deck, an installation model, an aircraft mission solver, or an emissions-certification tool.

The source strategy prioritised primary institutional sources and openly accessible academic work. NASA NTRS records were checked for distribution/copyright determinations; official FAA and EASA downloads were checked as official freely accessible documents; the DLR thesis was downloaded under its stated CC BY-NC-ND licence; MDPI articles were downloaded under the CC BY 4.0 licence shown by Crossref/article records. AGARD proceedings and selected DLR/NASA items with no explicit local redistribution permission were cited but not downloaded.

All downloaded PDFs were checked for a PDF signature and page count. SHA-256 values below identify the exact local files downloaded on 2026-08-26.

## 1. ALAS starting point

### 1.1 Requirements-first contract

This section maps directly to the repository’s [requirements-first design document](../REQUIREMENTS_FIRST_AIRCRAFT_DESIGN.md) and the current [alas-prop crate boundary](../../crates/alas-prop/src/lib.rs). The requirements document makes four pieces of data first-class: value and unit, load case, policy, and evidence/status. This is the correct shape for propulsion. A propulsion requirement such as OEI ceiling or reserve fuel is not a scalar property of an engine; it is a result under a mass, atmosphere, configuration, failure, control, and reserve convention.

The requirements document also defines a mixed-variable split: discrete choices such as engine family and engine count belong in an outer architecture stage, while geometry and continuous propulsion variables are sized inside each candidate family. This is consistent with conceptual engine-cycle practice. NASA work on integrating engine-cycle and aircraft-configuration optimisation explicitly treated engine design variables and configuration variables as a coupled but structured problem, while DLR hybrid-electric work automates the construction and evaluation of alternative propulsion architectures.

The ALAS source of truth should therefore grow conceptually from:

DesignBrief + AlasConfig

to:

DesignBrief + PropulsionBrief + AlasConfig

The requested change can be made through a narrow adapter. The existing DesignBrief currently owns mission, accommodation, reserve, performance, and airport requirements; it does not yet carry a propulsion/energy family policy. A future PropulsionBrief should be adjacent to it rather than hidden in an engine preset.

### 1.2 Existing propulsion capabilities

The current repository provides the following baseline:

| Repository element | Current behaviour | Conceptual interpretation |
| --- | --- | --- |
| alas-prop cycle.rs | Parametric two-spool turbofan; Mach, altitude, bypass ratio, OPR, FPR, and turbine-inlet temperature; station temperatures/velocities, specific thrust, TSFC, and efficiencies | Fast Level-1 on-design or near-design cycle screen |
| alas-prop cycle/sweeps.rs | Carpet, bypass-ratio, efficiency, and altitude sweeps with feasibility masks | Useful design-space exploration and seed generation |
| alas-prop mission_turbofan.rs | Station-based network; sizes at one cruise condition and replays at sea-level static and mission points using a solved flow scale | Useful station diagnostics, but not a component-map deck |
| alas-config engines.rs | EngineSpec includes catalogue thrust, fan diameter, bypass ratio, nacelle dimensions, OPR, FPR, TIT, and cruise TSFC | A named baseline catalogue; several values are explicitly engineering estimates |
| alas-config geometry/engine.rs | EngineConfig carries nacelle profile, position, thrust, and cycle parameters; fan diameter is currently informational | Geometry seed and installation placement, not a full installed-performance model |
| alas-perf performance/constraints.rs | Cruise T/W uses scalar PerformanceConfig.thrust_lapse; OEI uses an algebraic N/(N−1) factor and extra drag/gradient terms | Fast screening only; not asymmetric engine/powerplant evaluation |
| alas-mission vehicle.rs | EngineRequest passes engine count, thrust, bypass ratio, dimensions, and cycle values into vehicle construction | Narrow adapter point for a typed propulsion request |
| alas-mission segments/analyses.rs | Mission control points call evaluate_thrust with throttle and a station-network flow scale | Natural consumer of a load-case propulsion evaluator |

The key gaps are maps, deck provenance, corrected-flow matching, variable geometry/control schedules, installation losses, engine-out asymmetry, power-split and state-of-charge dynamics, tank/battery volume, thermal rejection, noise/emissions assumptions, and uncertainty/status attached to every value.

For implementation traceability, the current owners are [the parametric cycle](../../crates/alas-prop/src/cycle.rs), [the mission turbofan network](../../crates/alas-prop/src/mission_turbofan.rs), [the engine catalogue](../../crates/alas-config/src/engines.rs), [engine geometry configuration](../../crates/alas-config/src/geometry/engine.rs), [mission engine requests](../../crates/alas-mission/src/vehicle.rs), and [performance constraints](../../crates/alas-perf/src/performance/constraints.rs). These are references for an additive interface design; none is modified by this research deliverable.

### 1.3 Current-model parity rule

The first implementation should preserve the existing answers for the existing turbofan tests and named presets. A new model can be selected explicitly by fidelity or source. The old scalar path remains valid as a fallback for candidates that have not yet been promoted to a deck. Its result must carry a diagnostic such as ScalarLapseFallback rather than being reported as a passed physical requirement.

## 2. State of the art relevant to conceptual ALAS

### 2.1 Architecture selection is an outer discrete problem

At conceptual level, the architecture decision is a mixed discrete/continuous problem:

- discrete: family, number of engines or propulsors, shaft/bus topology, geared or direct drive, fuel/energy carrier, electric-assist location, distributed-propulsor count, engine-out strategy, and installation region;
- continuous: thrust or shaft-power rating, bypass ratio, pressure ratios, turbine temperature, propeller diameter and rpm, electric power fraction, battery mass, tank volume, nacelle placement, inlet area, pylon geometry, and control schedules.

Trying to encode family or topology as arbitrary floating-point variables makes infeasible concepts look numerically close to feasible ones. Enumerating a small supported catalogue or generating architecture seeds is more transparent and lets each family expose the right constraints.

NASA’s early engine-cycle/aircraft-configuration integration work is a direct precedent: the engine design variables, aircraft configuration, cycle selection, weight/dimensions, noise, and emissions were considered together at a conceptual level. NASA FLOPS is another useful precedent because it combines weights, aerodynamics, engine-cycle analysis, propulsion-data scaling/interpolation, mission, field performance, noise footprint, and cost in one synthesis context. The lesson is not that ALAS should copy FLOPS; it is that propulsion data must be available to the aircraft synthesis loop with clear scaling and validity boundaries.

The DLR 2024 thesis by Fouda is particularly relevant to requirements-first architecture selection. It constructs and evaluates conventional, turboelectric, all-electric, series-hybrid, and parallel-hybrid architectures automatically in a conceptual-design/MDO workflow. Its result supports an ALAS architecture graph in which power sources, conversion devices, shafts, buses, propulsors, and thermal paths are objects that can be composed and evaluated.

Recommended family seeds:

| Family seed | Good initial use | Main conceptual gates |
| --- | --- | --- |
| Turbofan or geared turbofan | High-subsonic transport, long range, high cruise Mach, large payload | Cruise thrust/TSFC, fan diameter, nacelle/pylon drag and weight, take-off thrust, hot/high rating, OEI, noise, emissions |
| Turboprop or turboshaft | Regional/utility aircraft, lower cruise Mach, short-to-medium range | Shaft power, propeller map, diameter/clearance, gearbox, take-off power, propeller noise, OEI control |
| Open rotor / advanced propfan | Short/medium range where propulsive efficiency can outweigh integration penalties | Blade diameter and clearance, acoustic shielding, pylon/wing integration, structural loads, gearbox, certification evidence |
| Parallel hybrid | Existing thermal shaft/propulsor retained with electric peak or cruise assist | Battery power and energy, shaft torque split, SOC, thermal rejection, asymmetric failure, extra mass/volume |
| Series hybrid / turboelectric | Distributed propulsors, boundary-layer ingestion, or flexible electrical placement | Generator rating, rectifier/inverter/motor efficiency and mass, cable losses, bus fault tolerance, heat rejection, installation |
| Battery-electric | Only short missions or technology studies with very high specific energy/power | Usable energy after reserve, pack volume, peak power, thermal runaway containment, recharge/turnaround, OEI |
| Hydrogen combustion or fuel-cell electric | Low-carbon concept studies with separate infrastructure/volume assumptions | Cryogenic or compressed storage, tank insulation and boil-off, centre of gravity, fuel-cell/generator efficiency, thermal system, airport assumptions |

The seed is a hypothesis, not a promise. A concept should be rejected early only for an explicit hard gate, such as impossible energy volume or an incompatible runway requirement. A technology target that is not yet available should be represented as an assumption with a sensitivity range.

### 2.2 Cycle model versus engine deck

A thermodynamic cycle model answers a design-space question: given architecture, pressure ratios, temperatures, efficiencies, bleed/extraction assumptions, and a flight condition, what station states and idealised performance result?

An engine deck answers an aircraft-simulation question: for a defined engine and control schedule, what net thrust, fuel flow, shaft power, ram drag, temperatures, and limits apply at a grid of Mach, altitude, throttle or power setting, and sometimes bleed/extraction conditions?

They are related but not interchangeable:

1. A cycle model supplies design-point thermodynamics and can generate a deck.
2. A component-map model matches fan/compressor/turbine/nozzle operating points at off-design conditions.
3. The deck records stable, converged results plus validity and limit flags.
4. The aircraft model interpolates inside the deck and reports extrapolation as a residual or diagnostic.

NASA’s 1978 Morris program is a useful lower-fidelity reference. It used a simplified one-dimensional cycle, thermodynamic component models, empirical off-design efficiency trends, and a maximum-compressor-efficiency-line assumption. It was intentionally fast and reasonable, not a substitute for detailed component maps. The AGARD paper by H. Wittenberg in Performance Prediction Methods made a similar historical trade: a gas-dynamics-only off-design method without detailed component maps, using choked-throat assumptions, gave fair agreement for selected existing engines. These methods remain useful as Level-1 fallbacks, but they should be labeled as such.

NASA’s 2025 MBSAE final report describes the modern conceptual pattern more directly: a steady-state engine deck is generated from a cycle analysis or NPSS-like simulation, then interpolated using linear or Lagrange methods. Typical deck outputs include unscaled thrust, fuel flow, ram drag, and NOx. The report also warns that strong aero-propulsive interactions can make an uncoupled model underpredict fuel burn; the reported 14 percent result is study-specific, but it is an important coupling warning.

CEA supports the property side of the cycle:

- CEA 1996 documents equilibrium composition and thermodynamic/transport properties;
- CEA2022 modernises the software and adds thread-safe parallel solves, updated data, negative/inert reactants, and APIs for several environments.

CEA should therefore be an optional property provider behind the cycle model. It should not be treated as the engine deck itself. Real engine performance also requires component efficiencies/maps, mechanical losses, cooling/bleed flows, controls, nozzle behaviour, installation, and calibration.

Recommended deck dimensions at conceptual fidelity:

- Mach or freestream velocity;
- pressure altitude and ambient deviation;
- throttle, corrected speed, or power setting;
- bleed/extraction or anti-ice state where relevant;
- engine health/degradation state if sensitivity is required;
- energy-carrier mode or electric power split for hybrid systems.

Recommended deck outputs:

- gross and net thrust;
- ram drag and inlet recovery;
- fuel flow and fuel energy rate;
- shaft power and electrical bus power;
- station temperatures/pressures and exhaust temperature;
- corrected mass flow and speed;
- compressor/turbine map margins;
- limit flags and interpolation/extrapolation status;
- NOx and other emissions indices when a calibrated model exists;
- model/source/fidelity identifier and uncertainty.

The deck should be generated at named load cases, not silently filled by extrapolation. Each interpolated result should retain the surrounding grid cell and an out-of-range flag.

### 2.3 Thrust lapse and off-design performance

Installed net thrust is not a fixed percentage of sea-level static thrust. It changes with:

- ambient density, temperature, and pressure;
- Mach number and ram compression;
- corrected fan/compressor flow and shaft speed;
- throttle/control schedule;
- inlet pressure recovery and distortion;
- bleed and power extraction;
- nozzle choking and pressure mismatch;
- turbine temperature and cooling flow;
- nacelle/installation losses;
- engine health and deterioration;
- propeller advance ratio and gearbox efficiency for shaft-powered systems;
- electric motor/inverter/bus voltage, current, speed, and thermal limits.

For a gas-turbine component map, the usual independent quantities are corrected mass flow and corrected rotational speed, for example:

W_corr = W × sqrt(Tt / Tref) / (Pt / Pref)

N_corr = N / sqrt(Tt / Tref)

The exact convention must be documented and applied consistently. The map returns pressure ratio, efficiency, surge/choke margin, and sometimes bleed or variable-stator schedule. The engine solver matches rotating shafts, mass flow, energy, pressure, and nozzle conditions while observing control and temperature limits.

For a turbofan, a useful conceptual accounting is:

T_net = gross core/fan momentum and pressure thrust − ram drag − inlet/spillage losses − extraction penalties

For a turboprop or turboshaft:

T_prop ≈ η_prop × P_shaft / V

is only a cruise approximation. At low speed and take-off, propeller maps in advance ratio, rotational speed, and helical Mach number are required. A power-to-thrust conversion that becomes singular or unphysical near zero speed must not be used for take-off.

The current PerformanceConfig.thrust_lapse is therefore appropriate only for the cheapest T/W chart. The next model should query a PropulsionEvaluator at each load case and return installed thrust and fuel/power flow. The scalar can be retained as an explicit fallback and as a calibration target against deck ratios.

Off-design does not require transient simulation at the first useful fidelity. A steady map-based deck is enough to handle climb, cruise, descent, hot/high take-off, maximum continuous, idle, and OEI screening. Transient models become necessary for acceleration time, surge margin during throttle movement, spool dynamics, generator/battery control transients, and flight-control interaction.

### 2.4 Installation, nacelle, pylon, and airframe integration

The model boundary must state whether a reported value is:

1. uninstalled engine gross thrust;
2. installed propulsion net thrust at the aircraft reference station;
3. aircraft drag including nacelle, pylon, inlet, exhaust, and interference; or
4. a combined propulsion-plus-airframe net force.

Double counting is easy. If the engine evaluator subtracts inlet spillage and ram drag, the aircraft aero model must not subtract them again. If the pylon is represented in the aircraft drag build-up, it should not also be hidden in a generic installed-thrust correction.

A useful installed-performance decomposition is:

T_installed = T_uninstalled
  − inlet pressure/recovery and spillage penalty
  − ram drag and bleed/extraction penalty
  − installation/interference force assigned to the propulsion boundary

and separately:

D_aircraft = D_wing/body/tail
  + D_nacelle
  + D_pylon
  + D_inlet/exhaust
  + D_interference

The exact split is a bookkeeping convention; the total aircraft force must be invariant.

NASA’s turbofan weight/dimensions work shows why geometry belongs in the coupled loop: engine and nacelle geometry directly affect nacelle weight and drag, and noise-oriented designs can change size and mass. NASA’s high-wing pylon-integration report measured the effect of pylon geometry and installation position on drag and lift. The older NASA aeroacoustics overview documents that under-wing and above-wing placement changes shielding/reflection and that pylon/plume interactions affect jet noise.

The Level-2 installation model should expose:

- nacelle length, maximum radius, inlet area, outlet area, and wetted area;
- fan diameter or propeller diameter;
- pylon wetted area, thickness, sweep, cant, and spanwise/vertical/longitudinal position;
- inlet recovery and distortion factors;
- nozzle/exhaust pressure or velocity correction;
- nacelle/pylon/interference drag correlations;
- engine-out windmilling or dead-engine drag;
- clearance and ground/wing/flap interaction flags;
- optional boundary-layer-ingestion or wake-recovery factor with explicit calibration source.

Boundary-layer ingestion and distributed propulsion should not be represented by a free efficiency bonus. The aircraft solver needs the velocity profile or a calibrated effective inlet condition, the drag and distortion penalty, the propulsor map, and a control law. NASA studies of turboelectric distributed propulsion and STARC-ABL are useful evidence that the coupling can be beneficial, but their percentage improvements are configuration-specific and should not become ALAS defaults.

### 2.5 Engine-out, powerplant failure, and safety boundary

The present algebraic OEI correction is a useful first screen but not a physical engine-out analysis. For a twin-engine aircraft, the OEI load case should:

- disable the failed engine or propulsor, including its residual drag/windmilling state;
- evaluate the live engine at the actual altitude, Mach, throttle, and hot-day condition;
- include asymmetric thrust and the yawing moment arm;
- include rudder/aileron trim, control authority, trim drag, and any speed schedule;
- account for the aircraft mass, flap/gear state, and specified climb gradient;
- evaluate the limiting engine-out ceiling and second-segment or equivalent take-off case;
- expose which engine/propulsor failed and whether reconfiguration is allowed.

For electric/hybrid systems, failure cases also include motor, inverter, bus, converter, generator, battery-string, cooling-loop, and supervisory-controller failures. Redundancy can preserve thrust but may increase mass and cooling requirements. An apparently symmetric distributed-propulsion design can still develop asymmetric power if a bus, converter, or thermal loop is lost.

The FAA AC 25.901-1 guidance makes the certification boundary explicit: powerplant and APU installations must be included in the overall safety assessment, and analyses cover thrust loss/excess drag, uncontrollable high thrust, fire, structural overload, debris release, personnel hazard, and system-level failure conditions. The advisory circular is guidance, not a conceptual-design pass/fail rule. It is nevertheless a good ALAS checklist for keeping engine/installation failure assumptions visible.

EASA CS-25 and CS-E provide a parallel European traceability boundary. Requirements concerning turbine-engine performance, engine failure effects, and installation/system safety belong in a later compliance matrix. ALAS should record the relevant requirement identifier and load case, but should not imply compliance from a conceptual residual.

The OEI residual should be a directional requirement residual:

- required climb gradient minus achieved climb gradient;
- plus a separate control-authority residual;
- plus a separate engine-out drag/asymmetry diagnostic;
- with positive values meaning violation.

The simple N/(N−1) factor can remain in the Level-0/Level-1 chart as an optimistic or conservative screen only if its assumptions are shown beside the result.

### 2.6 Turboprop, open rotor, electric, hybrid, and hydrogen alternatives

#### Turboprop and turboshaft

Turboprop concepts need a shaft-power model and a propeller map. The core cycle supplies gas-generator power, fuel flow, exhaust residual thrust, and shaft speed. The gearbox and propeller convert shaft power to thrust with advance-ratio and tip-Mach limits. The dominant conceptual variables are power rating, power-specific fuel consumption, propeller diameter, rpm, gearbox ratio/efficiency, blade count, and installation/ground-clearance geometry.

NASA’s advanced-open-rotor work is a useful precedent for combining cycle performance and engine weight. It also reinforces that acoustics, structural dynamics, gearbox, installation, and certification constraints can dominate an efficiency benefit. The local rights determination for that NASA open-rotor report was not explicit enough for this corpus, so it is cited only in the cited-only inventory.

#### Open rotor

Open rotor/propfan should be seeded when the mission rewards propulsive efficiency at lower Mach and the design accepts large rotating diameter, acoustic exposure, blade containment/clearance, and pylon/wing integration work. The seed must include:

- propeller/rotor disk area and diameter;
- rotational speed and helical Mach limit;
- advance-ratio map;
- gearbox mass and efficiency;
- blade structural/containment allowance;
- community-noise diagnostic;
- ground and high-lift clearance;
- asymmetric-thrust/OEI strategy.

It should not be treated as “a turbofan with a different bypass ratio.”

#### Parallel hybrid

Parallel hybrid power is a split between a thermal shaft and an electric machine. The model must solve both torque/power balance and energy-state balance:

P_prop_req = P_thermal_to_shaft + P_battery_to_shaft − conversion losses

The electric machine may assist the propeller shaft, a compressor spool, or a gearbox. Each location has different efficiency, rpm, mass, and thermal behaviour. The control law should be a named assumption: electric boost for take-off, climb assist, cruise assist, peak shaving, or reserve-only.

#### Series hybrid and turboelectric

Series architectures decouple the thermal source from propulsor placement but add generator, power electronics, cables, motors, and conversion losses. Their main conceptual value can be distributed propulsion, wake ingestion, or flexible packaging rather than raw energy efficiency. The seed must close generator rating, bus voltage/current, motor power, cable losses, thermal rejection, fault isolation, and propulsor count.

#### Battery-electric

Battery-electric screening is dominated by usable energy and peak power, not nameplate energy alone:

m_battery = E_usable / e_pack

m_power = P_peak / p_specific

The larger of energy-driven and power-driven mass is not sufficient. The battery must also fit in the available volume, remain within voltage/current/SOC/temperature limits, maintain reserve, and reject heat. A battery model that only has a gravimetric energy density is a diagnostic, not a closed aircraft model.

#### Hydrogen

Hydrogen must be a separate energy-carrier seed, not a SAF substitution. The aircraft needs a fuel mass and volume model, tank insulation and boil-off assumption, pressure/cryogenic state, centre-of-gravity envelope, thermal sink/rejection model, and airport/infrastructure assumptions. Hydrogen combustion and fuel-cell-electric architectures have different efficiency, emissions, water/heat, and powertrain interfaces.

#### SAF

SAF should be modelled as a fuel-property and lifecycle assumption with a pathway and blend fraction. It should not be labelled zero-emission. Combustion CO2 per unit of fuel remains relevant; lifecycle greenhouse-gas benefit depends on feedstock, processing, transport, land-use accounting, and the chosen system boundary. Non-CO2 climate effects and particulate/aromatic changes require separate assumptions. The FAA alternative-jet-fuels report and EASA environmental reporting are appropriate evidence anchors, while ALAS should expose a scenario range rather than a universal SAF factor.

### 2.7 Fuel/energy volume, reserve, mass, and thermal constraints

The requirements document already exposes reserve policy modes. Propulsion must consume that policy explicitly:

- trip-fraction reserve;
- fixed reserve mass;
- diversion plus holding time;
- minimum landing fuel or minimum landing SOC;
- failure-contingency energy where required.

For liquid fuel:

V_usable_fuel = m_usable_fuel / rho_usable_fuel

The model must distinguish usable fuel, unusable fuel, tank/system mass, ullage, tank geometry, transfer limits, and centre-of-gravity movement. A conceptual tank volume can be a hard capacity constraint even when fuel mass closes.

For batteries:

- usable energy is nameplate energy multiplied by allowed SOC window and reserve policy;
- peak power is limited by cell/pack power density, temperature, current, voltage, and degradation assumptions;
- pack volume is not inferred safely from energy mass alone;
- containment, isolation, venting, fire/thermal-runaway protection, and structural attachment require mass and volume allowances;
- charging time and turnaround can become an operational requirement.

Thermal management is a first-class aircraft discipline:

Q_reject = device and conversion losses
  + battery internal losses
  + gearbox/pump/fan losses
  + retained heat from fuel-cell or generator systems

The thermal system consumes mass, drag, power, volume, and sometimes fuel or ram-air sink capacity. NASA’s electrified-aircraft thermal-management report varies architecture, efficiency, and operating temperature and shows that lower allowable component temperature can increase system weight. The 2021 open-access ram-air TMS study models heat exchangers, ducts, pumps/fans, mass, drag, and fuel-burn effects; its numerical fuel-burn result is configuration-specific, but its model boundary is the right conceptual pattern. The 2023 TMS review likewise treats heat acquisition, transport, and rejection as an integrated architecture problem.

The thermal evaluator should return, per load case:

- heat generated by each device;
- heat accepted by each loop;
- heat rejected to ambient/fuel/structure;
- sink temperature and margin;
- pump/fan power;
- exchanger frontal area and drag;
- thermal limit and exceedance;
- mass/volume/CG contribution.

DLR and the open academic studies also show the importance of location. Batteries near the centre of gravity can reduce trim movement but may consume cabin/belly volume; distributed batteries can improve packaging or redundancy but increase wiring, protection, and thermal-loop complexity.

### 2.8 Noise, emissions, and environmental assumptions

At conceptual fidelity, ALAS should produce environmental diagnostics and scenario metrics, not certification claims.

Noise:

- distinguish source noise from installation/shielding/reflection;
- retain engine/propulsor operating point, jet velocity, fan/propeller tip Mach, and installation geometry;
- expose community-noise proxy or external-tool result with method/version;
- preserve a not-evaluated state when no valid noise translator exists.

NASA’s propulsion-airframe aeroacoustics research shows that installation can change noise through shielding, reflection, pylon, and plume interactions. A scalar noise offset is acceptable only as a named screening assumption.

Emissions:

- fuel burn and CO2 mass are direct conceptual outputs;
- NOx, CO, HC, smoke/PM, SOx, and water-vapour proxies require emission-index models or deck data;
- CEA can provide equilibrium species properties but does not predict a certified combustor emission index without a combustion/emissions model;
- SAF changes lifecycle accounting and potentially non-CO2 effects; the combustion model should carry fuel properties and pathway assumptions.

The FAA AEDT tool is the appropriate later-stage reference for aviation environmental analyses, including noise, fuel burn, and emissions. The current AEDT page identifies the version and applicability of the tool; ALAS should export a traceable aircraft/engine/mission description rather than claim that an internal proxy is AEDT-equivalent.

### 2.9 Coupled propulsion-airframe sizing

A propulsion seed changes the airframe and the airframe changes the propulsion result:

1. thrust or shaft-power rating changes engine size, nacelle/propeller geometry, and mass;
2. nacelle and pylon change drag, lift, aeroelastic loads, noise, and wing structure;
3. engine location changes CG, trim, OEI yaw, and wing bending;
4. fuel/battery/hydrogen volume changes fuselage, wing, cabin, cargo, structure, and CG;
5. thermal hardware changes drag, power demand, volume, and mass;
6. mission fuel/energy burn changes take-off mass and therefore required thrust/area;
7. thrust and power schedules change the field-performance and OEI constraints.

The coupled loop should converge on at least:

- MTOW and operating empty mass;
- installed thrust/shaft power at design and hot/high cases;
- fuel/battery/hydrogen mass and volume;
- mission energy/fuel closure with reserve;
- thermal mass/drag/power;
- nacelle/pylon/propulsor geometry;
- OEI climb and control margin;
- CG and trim effects.

NASA’s 1994 integrated engine-cycle/aircraft optimisation report supports this loop structure. NASA FLOPS and WATE++ provide precedents for coupling propulsion performance with weights and dimensions. The MBSAE report’s warning about uncoupled aero-propulsive fuel-burn error is a reason to promote finalists to an installed/coupled model before ranking close concepts.

## 3. Conceptual fidelity ladder and explicit limits

| Level | Model | Suitable ALAS use | It must not claim |
| --- | --- | --- | --- |
| 0 | Scalar thrust lapse, fixed TSFC, algebraic OEI, energy-density-only battery | Broad architecture pre-screen, matching-chart seed, early rejection | Off-design accuracy, certification, engine-out control, installed thrust |
| 1 | Parametric 0-D/1-D cycle with station states and named design points | Cycle carpet, BPR/OPR/TIT sensitivity, initial mass-flow/thrust sizing | Valid compressor/turbine map margins or accurate throttle behaviour outside the calibration points |
| 2 | Steady map-based component matching and multidimensional engine deck | Main conceptual mission, hot/high, take-off, climb, cruise, descent, power split, fuel/power flow | Transient handling, life, surge certification, detailed distortion, authority compliance |
| 3 | Installed propulsion plus nacelle/pylon/propeller/thermal/airframe coupling | Finalist ranking, OEI, field performance, volume/CG/thermal closure, noise/emissions scenario | Certified installation, validated CFD/rig performance, failure compliance |
| 4 | Validated high-fidelity simulation, test-calibrated deck, external environmental and structural evidence | Technology maturation, preliminary design evidence, certification support inputs | Still not a type certificate unless the authority-approved compliance programme is complete |

The requirements-first policy follows directly: if a requirement has no translator at the selected level, it remains not-yet-evaluated or diagnostic. It is never silently passed. A Level-0 OEI result can help order candidates, but the report must say that the engine-out climb requirement is not physically evaluated until a suitable evaluator is selected.

## 4. Recommended typed propulsion contract

The following is an interface recommendation, not a source edit. Rust field names should use the repository’s established SI suffix convention or a units type. Every enum should serialize into the run manifest.

### 4.1 Architecture and energy types

PropulsionFamily:

- Turbojet
- Turbofan
- GearedTurbofan
- Turboprop
- Turboshaft
- OpenRotor
- Turboelectric
- SeriesHybrid
- ParallelHybrid
- BatteryElectric
- HydrogenCombustion
- HydrogenFuelCell

EnergyCarrier:

- JetA or SAF with fuel density, lower heating value, blend/pathway, and lifecycle scenario;
- HydrogenLiquid or HydrogenCompressed with storage state, usable density, boil-off/venting assumption, and tank system allowance;
- Battery with pack specific energy, pack volumetric energy, pack specific power, SOC window, reserve SOC, temperature limits, and degradation scenario;
- External or technology-study carrier with an explicit evidence/status field.

PropulsionArchitecture:

- family and stable architecture identifier;
- number of thermal engines, electric machines, generators, propulsors, and independent buses;
- shaft topology and gearbox ratios;
- energy-carrier identifiers;
- rated thrust or shaft-power vector;
- electric power fraction or supervisory-control policy;
- propulsor positions and handedness;
- engine-out/reconfiguration policy;
- installation model identifier;
- performance model/deck identifier;
- source, version, validity range, and uncertainty.

Do not put an entire physical engine model into EngineSpec. EngineSpec is a catalogue datum. The evaluator should carry a reference to a parametric cycle, map/deck, installation model, or calibrated source.

### 4.2 Operating point, load case, and model interfaces

OperatingPoint:

- phase and point identifier;
- altitude_m, mach, true airspeed, ambient temperature deviation;
- aircraft mass_kg and fuel/energy state;
- throttle or commanded power;
- bleed, anti-ice, extraction, and accessory loads;
- live/failed engine mask;
- propeller/rotor rpm or shaft-speed command where relevant;
- battery SOC and bus voltage where relevant;
- requested thrust or shaft power.

LoadCase:

- requirement identifier and policy;
- operating point or operating-point sweep;
- atmosphere convention;
- aircraft mass and configuration;
- engines/propulsors available;
- control/reconfiguration policy;
- reserve convention;
- required output and limit;
- evidence/status and selected fidelity.

PropulsionModel:

- metadata: model_id, family, fidelity, source, version, validity envelope;
- evaluate(OperatingPoint, LoadCase) → PropulsionEvaluation;
- optional batch evaluation for deck generation;
- optional sizing function for a target design point;
- explicit error for map extrapolation, non-convergence, infeasible limit, or missing translator.

InstallationModel:

- inlet recovery and distortion;
- ram/spillage/bleed penalties;
- nacelle/pylon/interference drag assignment;
- exhaust/plume and nozzle correction;
- propulsor/wake/BLI correction;
- dead-engine/windmilling drag;
- geometry and clearance checks;
- source and uncertainty.

The installation model can live in alas-prop initially and later call alas-aero for higher-fidelity geometry/force data. The important part is an explicit boundary and no double counting.

### 4.3 PropulsionEvaluation output

PropulsionEvaluation should return at least:

- thrust_gross_n;
- thrust_net_uninstalled_n;
- thrust_net_installed_n;
- ram_drag_n;
- shaft_power_w;
- electric_bus_power_w;
- fuel_flow_kg_s;
- carrier_energy_rate_w;
- battery_soc_rate or fuel state rate;
- core/fan/propulsor mass flow;
- station temperatures and pressures where available;
- shaft speeds and corrected flow;
- map margins and limit flags;
- installation losses and assigned aircraft drag;
- thermal generation/rejection and sink margin;
- powertrain mass_kg, volume_m3, and CG contribution;
- noise/emissions diagnostic values when supported;
- uncertainty and model status;
- a vector of typed residuals.

The result needs both gross and installed outputs because aircraft sizing often wants one while mission/performance wants the other. It also needs the state that produced the result so a failed or clipped evaluator cannot look numerically plausible.

### 4.4 Operating points and load cases

The minimum named set for a transport-style concept is:

| Identifier | Typical condition | Why it matters |
| --- | --- | --- |
| cruise_design | Initial cruise Mach and altitude, design payload, nominal atmosphere | Cycle/deck sizing and range efficiency |
| cruise_max_altitude | Maximum cruise altitude at residual climb criterion | Ceiling, lapse, and reserve |
| climb_ttc | MTOW, ISA+10, climb schedule from 1,500 ft to ICA | Time-to-climb and energy rate |
| takeoff_sl | Sea-level ISA+15, MTOW, take-off configuration | TOFL, rated thrust/power, thermal peak |
| takeoff_hot_high | Hot/high airport, MTOW, limiting runway | Density lapse, spool/propeller limits, thermal margin |
| max_continuous | Maximum continuous power/thrust at representative altitude | Engine rating and mission protection |
| descent_idle | Idle or minimum stable power across descent | Fuel burn, control, thermal and restart assumptions |
| approach_landing | MLW, approach/landing configuration | Go-around margin and installation/high-lift interaction |
| oei_second_segment | One engine/propulsor failed after take-off | Climb gradient, asymmetric control, dead-engine drag |
| oei_ceiling | One engine/propulsor failed at altitude | OEI ceiling and limiting live-engine schedule |
| engine_out_drift | Failed unit, drift-down/descent or emergency reserve | Mission reserve and controllability |
| field_go_around | Landing mass, go-around power | Recovery requirement and hot/high margin |

Electrified architectures add:

- electric_takeoff_peak;
- battery_max_continuous;
- generator_max_continuous;
- minimum_SOC_in_cruise;
- reserve_SOC_at_landing;
- thermal_sink_max;
- bus or converter failure;
- motor/inverter failure;
- cooling-loop failure;
- reconfiguration with asymmetric propulsive power.

The operating-point table belongs in the run manifest. Mission segments can interpolate between points only inside a declared envelope.

### 4.5 Residuals and ranking

Use a directional convention: positive residual means violation.

For a hard minimum:

r = (required − achieved) / scale

For a hard maximum:

r = (achieved − limit) / scale

For an equality or closure:

r = absolute(achieved − target) / scale

Recommended residual kinds:

- installed_thrust_deficit;
- shaft_power_deficit;
- fuel_flow_or_energy_rate_limit;
- mission_fuel_closure;
- reserve_fuel_deficit;
- reserve_SOC_deficit;
- fuel_volume_excess;
- battery_volume_excess;
- hydrogen_tank_volume_excess;
- usable_energy_deficit;
- peak_power_deficit;
- thermal_rejection_excess;
- thermal_sink_margin;
- battery_temperature_excess;
- corrected_flow_map_extrapolation;
- compressor_surge_margin;
- turbine_temperature_limit;
- nozzle_choke or pressure-mismatch limit;
- propeller_tip_Mach;
- propeller_ground_clearance;
- inlet_recovery;
- nacelle_or_pylon_drag;
- installation_weight;
- OEI_climb_gradient;
- OEI_control_authority;
- engine_out_drag;
- CG_or_trim_shift;
- takeoff_field_length;
- go-around thrust/power;
- noise_proxy;
- NOx/CO/HC/PM/SOx diagnostic;
- lifecycle_CO2e diagnostic;
- unsupported_translator or missing_evidence.

Hard/soft/diagnostic policy belongs on the requirement, not hidden in the evaluator. A candidate with a high noise proxy can remain eligible if noise is a soft target; a candidate with a hard fuel-volume excess cannot be rescued by a better aerodynamic objective.

## 5. Architecture-seed logic mapped to DesignBrief

The following logic is intended for the outer architecture stage in alas-opt or the pipeline’s preliminary sizing stage. It can be implemented later without changing the existing source files.

1. Freeze the validated DesignBrief and derive:
   - design range and reserve convention;
   - design and maximum payload;
   - passenger/cargo and deck requirements;
   - cruise Mach, ICA, MMO/VMO, TTC, OEI ceiling;
   - TOFL, landing distance, approach speed, span, and ACN;
   - route/airport and atmosphere conventions.

2. Read the propulsion policy:
   - allowed families;
   - allowed energy carriers;
   - technology-year/scenario;
   - maximum number of engines/propulsors;
   - minimum redundancy;
   - noise/emissions priorities;
   - whether experimental architectures are allowed.

3. Enumerate architecture seeds. For each seed, record the reason it was generated and the evidence class for every technology datum.

4. Apply cheap screens:
   - payload/cabin/cargo accommodation;
   - approximate mass closure;
   - fuel or stored-energy volume;
   - peak power and thermal feasibility;
   - cruise Mach/family compatibility;
   - TOFL and hot/high rating;
   - span/propeller/ground clearance;
   - minimum engine/propulsor count and OEI applicability.

5. Create the seed’s engine/powertrain request:
   - number of units;
   - design thrust or shaft-power rating;
   - cycle variables or map/deck reference;
   - nacelle/propulsor dimensions and positions;
   - electric power split;
   - battery/fuel/tank assumptions;
   - installation model;
   - operating-point set.

6. Evaluate at cruise_design, takeoff_sl, takeoff_hot_high, climb_ttc, and the relevant OEI/thermal cases. Reject only explicit hard violations; retain all residuals.

7. Pass surviving seeds to continuous aircraft sizing and geometry. Re-evaluate propulsion after wing, fuselage, mass, CG, and installation changes.

8. Promote close or policy-critical candidates from Level 0/1 to a deck-based Level 2 evaluator before ranking them on fuel burn or environmental performance.

Practical family heuristics:

- Higher cruise Mach and long range favour turbofan/geared-turbofan seeds, subject to fan diameter, nacelle, and noise limits.
- Lower Mach regional missions favour turboprop/turboshaft seeds; open rotor is a separate seed with explicit acoustic and clearance gates.
- Parallel or series hybrid seeds are most informative for regional/short-range studies or where the brief prioritises peak-shaving, distributed propulsion, or technology entry assumptions.
- All-electric seeds survive only when usable energy, reserve, power, volume, thermal, and turnaround assumptions close simultaneously.
- Hydrogen seeds must close tank volume, CG, storage state, thermal, and infrastructure assumptions independently of the propulsion conversion efficiency.
- No seed should receive an automatic “green” ranking from its energy carrier. Environmental results depend on lifecycle and mission assumptions.

This logic maps directly to the requirements document’s outer architecture stage, preliminary sizing, physical analysis, residual retention, and fidelity funnel. It also gives the user a reason when an architecture is not generated or is rejected.

## 6. Mapping to existing ALAS boundaries

| ALAS area | Recommended responsibility | Compatibility with current code |
| --- | --- | --- |
| alas-config | PropulsionBrief, family/carrier policies, reserve/energy policy, source/evidence metadata, serialisation | Additive; current EngineSpec and PerformanceConfig remain readable |
| brief adapter | DesignBrief + PropulsionBrief → legacy EngineConfig, EngineRequest, performance options | Keeps the existing pipeline stable and makes translation testable |
| alas-prop | PropulsionFamily, operating points/load cases, model/deck trait, cycle, map/deck, installation, energy and thermal evaluation | Natural owner; current cycle and mission_turbofan become Level-1 implementations |
| alas-mission | Ask PropulsionEvaluator at each mission control point; carry fuel/energy/SOC and reserve | Replaces direct assumptions only when a selected model is available |
| alas-perf | Build T/W and field constraints from installed thrust/power at the requested load case; keep scalar fallback | Avoids making thrust_lapse the hidden authority |
| alas-mass | Powertrain, nacelle/pylon, tank/battery/hydrogen, thermal, and CG contributions | Required for volume and mass closure |
| alas-aero | Nacelle/pylon/propulsor geometry and higher-fidelity installation forces | Optional at first; installation adapter can start with correlations |
| alas-struct | Engine/pylon loads, battery/tank containment, thermal and rotor/propeller structural diagnostics | Finalist-stage integration |
| alas-opt | Outer architecture enumeration, continuous design vector, cache, staged evaluation, residual ranking | Matches the requirements-first contract |
| alas-report / alas-viz | Assumption ledger, deck source, load-case table, residual matrix, energy Sankey/closure and fidelity badges | Prevents a diagnostic from looking like a pass |

Specific current-code recommendations:

1. Treat cycle.rs as a Level-1 parametric model and retain its sweeps.
2. Treat mission_turbofan.rs as a station-network evaluator and add explicit design/off-design labels rather than implying it is a manufacturer deck.
3. Add a deck/map-backed evaluator behind a common interface. It can initially be generated from the current cycle model, then replaced with calibrated tables.
4. Replace direct uses of PerformanceConfig.thrust_lapse only when a load-case evaluator is selected. Otherwise retain the scalar with a ScalarLapseFallback status.
5. Replace the OEI algebraic result progressively: first use a live/failed engine mask and dead-engine drag; later add asymmetric trim/control.
6. Extend EngineSpec with source/evidence/uncertainty in a future schema change. The current documentation already warns that FPR, TIT, and cruise TSFC are engineering estimates rather than certified data.
7. Make fan_diameter_m, nacelle profile, pylon position, and thrust rating part of the same installation calculation so a catalogue engine is not geometrically detached from its performance.
8. Add mission energy/fuel state to the mission analysis state. A turbofan’s fuel flow is only one special case of a carrier-flow interface.

## 7. Suggested implementation sequence

### Phase 0: evidence and manifests

- Add no new physics yet.
- Define model IDs, fidelity levels, source/license/evidence fields, load-case IDs, and residual schema.
- Put the selected propulsion model and assumptions in the run manifest.
- Preserve existing test outputs and mark all new fields diagnostic until a translator exists.

### Phase 1: architecture seeds and Level-0/1 parity

- Add family/carrier seed logic.
- Wrap the current turbofan cycle and mission network behind the common evaluator.
- Add static, cruise, hot/high, and simple OEI cases.
- Add fuel/energy mass and volume estimates with explicit technology assumptions.
- Keep scalar lapse and N/(N−1) as fallbacks only.

### Phase 2: steady off-design decks

- Define corrected-flow/speed map data structures.
- Generate a small deck from the current cycle or imported open data.
- Interpolate inside the deck and return cell/validity metadata.
- Add throttle/control schedules, bleed/extraction, and limit flags.
- Use deck outputs for mission and field performance.

### Phase 3: installation and OEI

- Introduce inlet recovery, ram/spillage, nozzle, nacelle, pylon, and dead-engine corrections.
- Connect nacelle/pylon geometry and placement to mass/aero.
- Evaluate asymmetric live/failed engines and a simple control-authority model.
- Promote finalists to Level 3 installed coupling.

### Phase 4: electric/hybrid/hydrogen energy and thermal

- Add energy carriers, SOC, peak/continuous power, conversion losses, and thermal loops.
- Add battery/tank/thermal mass, volume, CG, and drag.
- Add bus/motor/generator/cooling failure cases.
- Re-run reserve, OEI, field, and mission closure with stored-energy states.

### Phase 5: environmental and external evidence

- Add source-tagged NOx/CO/HC/PM/SOx and lifecycle CO2e scenarios.
- Export a traceable engine/aircraft/mission description for external noise/emissions tools.
- Add calibrated or test-derived decks only where rights and provenance permit.
- Keep certification requirements in a separate compliance/evidence matrix.

## 8. Explicit limits and non-goals

The ALAS conceptual evaluator should not:

- claim a type-certified engine, aircraft, powerplant installation, or environmental result;
- infer a valid off-design map from one cruise TSFC datum;
- interpret an engineering estimate in EngineSpec as manufacturer-certified data;
- treat CEA equilibrium composition as a complete combustor or emissions model;
- treat a scalar thrust-lapse ratio as a full installed engine model;
- replace a propeller map with shaft-power divided by speed at take-off;
- call a fuel or battery technology feasible without mass, volume, reserve, power, and thermal closure;
- use a green-fuel label as proof of zero lifecycle or non-CO2 impact;
- use an algebraic OEI factor as proof of engine-out controllability;
- silently extrapolate an engine deck beyond its validity envelope;
- report an unevaluated requirement as passed;
- mix installation drag into both the propulsion evaluator and aircraft aerodynamics.

The right conceptual output for an uncertain candidate is a feasible/infeasible/diagnostic state plus the assumptions and residuals that explain it.

## 9. Downloaded source corpus

Download date for all files below: 2026-08-26. SHA-256 is lowercase hexadecimal. The local copies are linked from this report; the official record/source link is included for provenance. NASA rights labels reproduce the NTRS metadata determination where available. FAA documents are official government publications. EASA files are official free downloads but are retained as local research copies without assuming a redistribution licence. The DLR thesis carries CC BY-NC-ND; the MDPI articles carry CC BY 4.0.

### NASA and CEA

| Local PDF | Complete citation metadata | Rights/provenance | Pages; bytes; SHA-256 |
| --- | --- | --- | --- |
| [nasa-tm-78653-morris-1978.pdf](../../bib/propulsion-energy/nasa-tm-78653-morris-1978.pdf) | S. J. Morris, Computer Program for the Design and Off-Design Performance of Turbojet and Turbofan Engine Cycles, NASA Langley Research Center, 1978-06-01, NASA-TM-78653, [NTRS record](https://ntrs.nasa.gov/citations/19780022179), [official PDF](https://ntrs.nasa.gov/api/citations/19780022179/downloads/19780022179.pdf) | NTRS distribution PUBLIC; copyright determination GOV_PUBLIC_USE_PERMITTED; local research copy | 76; 3333846; ee68743a76a619c9ad2c8ff024c3b0fc421c7243806638299d20c646b6c26929 |
| [nasa-cr-194475-cours-1994.pdf](../../bib/propulsion-energy/nasa-cr-194475-cours-1994.pdf) | Jeffrey T. Cours, Design and Implementation of a Distributed Version of the NASA Engine Performance Program, NASA Lewis Research Center, 1994-03-01, NASA-CR-194475, E-8619, NAS 1.26:194475, [NTRS record](https://ntrs.nasa.gov/citations/19940020697), [official PDF](https://ntrs.nasa.gov/api/citations/19940020697/downloads/19940020697.pdf) | NTRS distribution PUBLIC; GOV_PUBLIC_USE_PERMITTED | 63; 2552924; f13c3e810ce3cae23cbf69b01d546f15b5f83785c2ed38d01aef4b4b9c3f5e9d |
| [nasa-rp-1311-mcbride-gordon-1996.pdf](../../bib/propulsion-energy/nasa-rp-1311-mcbride-gordon-1996.pdf) | Bonnie J. McBride and Sanford Gordon, Computer Program for Calculation of Complex Chemical Equilibrium Compositions and Applications II: Users Manual and Program Description, NASA Lewis Research Center, 1996-06-01, NASA-RP-1311, E-8017-1, NAS 1.61:1311, [NTRS record](https://ntrs.nasa.gov/citations/19960044559), [official PDF](https://ntrs.nasa.gov/api/citations/19960044559/downloads/19960044559.pdf) | NTRS distribution PUBLIC; PUBLIC_USE_PERMITTED | 184; 6900353; 09bc8a8d215daa8d7902c194e812a67466074f8154c81fb4c94f95a42b435a1a |
| [nasa-cea2022-leader-et-al-2024.pdf](../../bib/propulsion-energy/nasa-cea2022-leader-et-al-2024.pdf) | Mark K. Leader, Thomas M. Lavelle, Xiao-yen J. Wang, Kevin W. Dickens, Michael McTague, and Jeffrey P. Hill, CEA2022: A Modernization of NASA Glenn’s Software CEA (Chemical Equilibrium with Applications), NASA/TFAWS 2024, 2024, [NTRS record](https://ntrs.nasa.gov/citations/20240009728), [official PDF](https://ntrs.nasa.gov/api/citations/20240009728/downloads/TFAWS_2024_CEA.pdf) | NTRS distribution PUBLIC; GOV_PUBLIC_USE_PERMITTED; NTRS also marks OPEN_ACCESS | 11; 253873; 257593566280a0c7af10a2f9341aebb6557672be4f29b5e90ef0b1f691721df7 |
| [nasa-tm-2017-219627-vol1-flops.pdf](../../bib/propulsion-energy/nasa-tm-2017-219627-vol1-flops.pdf) | Douglas P. Wells, Bryce L. Horvath, and Linwood A. McCullers, The Flight Optimization System Weights Estimation Method, NASA Langley Research Center/ATK, 2017-06-01, NASA/TM-2017-219627/VOL1, L-20820, NF1676L-27097, [NTRS record](https://ntrs.nasa.gov/citations/20170005851), [official PDF](https://ntrs.nasa.gov/api/citations/20170005851/downloads/20170005851.pdf) | NTRS distribution PUBLIC; PUBLIC_USE_PERMITTED | 91; 881689; 819a48fc9c8f34f14595d93f3e3d54dc8454298e83e64048c14ac7bda00bb51d |
| [nasa-cr-20250007059-mbsae-final-report.pdf](../../bib/propulsion-energy/nasa-cr-20250007059-mbsae-final-report.pdf) | Jason Corman, Jimmy Tai, Evan Harrison, Jai Ahuja, Christian Perron, Bogdan-Paul Dorca, Josef D’cruz, and Samuel Moore, NASA Model-Based Systems Analysis and Engineering Final Report, NASA, 2025-08-01, NASA-CR-20250007059, [NTRS record](https://ntrs.nasa.gov/citations/20250007059), [official PDF](https://ntrs.nasa.gov/api/citations/20250007059/downloads/CR-20250007059.pdf) | NTRS distribution PUBLIC; PUBLIC_USE_PERMITTED | 60; 2570620; 57d7015e88a9044c5fb43a80c7833e93cc5060d883e1a9742a063a1a61b8d7bc |
| [nasa-cr-191602-geiselhart-1994.pdf](../../bib/propulsion-energy/nasa-cr-191602-geiselhart-1994.pdf) | Karl A. Geiselhart, A Technique for Integrating Engine Cycle and Aircraft Configuration Optimization, NASA Langley Research Center, 1994-02-01, NASA-CR-191602, NAS 1.26:191602, [NTRS record](https://ntrs.nasa.gov/citations/19940022103), [official PDF](https://ntrs.nasa.gov/api/citations/19940022103/downloads/19940022103.pdf) | NTRS distribution PUBLIC; GOV_PUBLIC_USE_PERMITTED | 78; 3465997; a3c1a510087ae9d563da6b1811f69a632125bd2844f52951cc7ba58418d7bc29 |
| [nasa-wate-tong-et-al-2002.pdf](../../bib/propulsion-energy/nasa-wate-tong-et-al-2002.pdf) | Michael T. Tong, Louis J. Ghosn, Ian Halliwell, and Tim Wickenheiser, A Computer Code for Gas Turbine Engine Weight And Disk Life Estimation, NASA Glenn Research Center, 2002-01-01, GT-2002-30500, [NTRS record](https://ntrs.nasa.gov/citations/20020072843), [official PDF](https://ntrs.nasa.gov/api/citations/20020072843/downloads/20020072843.pdf) | NTRS distribution PUBLIC; PUBLIC_USE_PERMITTED | 8; 3846382; f66b58f4da68376b00a14337c45cfeaf332bcb151754f1e034edd7c2c221a4aa |
| [nasa-tm-x-73199-waters-schairer-1977.pdf](../../bib/propulsion-energy/nasa-tm-x-73199-waters-schairer-1977.pdf) | M. H. Waters and E. T. Schairer, Analysis of Turbofan Propulsion System Weight and Dimensions, NASA Ames Research Center, 1977-01-01, NASA-TM-X-73199, A-6890, [NTRS record](https://ntrs.nasa.gov/citations/19770012125), [official PDF](https://ntrs.nasa.gov/api/citations/19770012125/downloads/19770012125.pdf) | NTRS distribution PUBLIC; GOV_PUBLIC_USE_PERMITTED | 65; 1469513; fd4d9f83cf7b8036d349344f5f58d21a50200a4d72750182f338922a5da5324f |
| [nasa-tm-20205011477-tms.pdf](../../bib/propulsion-energy/nasa-tm-20205011477-tms.pdf) | Jeffryes W. Chapman, Hashmatullah Hasseeb, and Sydney Schnulo, Thermal Management System Design for Electrified Aircraft Propulsion Concepts, NASA Glenn Research Center, 2021-03-01, NASA/TM-20205011477, E-19916, AIAA-2020-3571, [NTRS record](https://ntrs.nasa.gov/citations/20205011477), [official PDF](https://ntrs.nasa.gov/api/citations/20205011477/downloads/TM-20205011477.pdf) | NTRS distribution PUBLIC; GOV_PUBLIC_USE_PERMITTED; NTRS license field is not permissive, so local reference only | 30; 2316946; 28084fcb174c3baf69873769b568f0dde3a4066bb94ec06e22b8b31965209b08 |
| [nasa-hybrid-drive-kpi-duffy-jansen-2018.pdf](../../bib/propulsion-energy/nasa-hybrid-drive-kpi-duffy-jansen-2018.pdf) | Kirsten P. Duffy and Ralph H. Jansen, Turboelectric and Hybrid Electric Aircraft Drive Key Performance Parameters, NASA Glenn Research Center, 2018-07-12, GRC-E-DAA-TN57353, [NTRS record](https://ntrs.nasa.gov/citations/20180005327), [official PDF](https://ntrs.nasa.gov/api/citations/20180005327/downloads/20180005327.pdf) | NTRS distribution PUBLIC; PUBLIC_USE_PERMITTED | 19; 1097746; 22c69f5e13f36f49c9f49b163dcd3108c9d1d0a3dea120984f77cf0a571d9a53 |
| [nasa-hybrid-regional-antcliff-et-al-2016.pdf](../../bib/propulsion-energy/nasa-hybrid-regional-antcliff-et-al-2016.pdf) | Kevin R. Antcliff, Mark D. Guynn, Ty V. Marien, Douglas P. Wells, Steven J. Schneider, and Michael T. Tong, Mission Analysis and Aircraft Sizing of a Hybrid-Electric Regional Aircraft, NASA, AIAA-2016-1028, 2016-01-02, [NTRS record](https://ntrs.nasa.gov/citations/20160007763), [official PDF](https://ntrs.nasa.gov/api/citations/20160007763/downloads/20160007763.pdf) | NTRS distribution PUBLIC; GOV_PUBLIC_USE_PERMITTED | 16; 595106; 4a66d53a673201320c7cc6e55d106cc68245504c34697ec054d37f1f89061943 |
| [nasa-tp-2877-pylon-integration-1989.pdf](../../bib/propulsion-energy/nasa-tp-2877-pylon-integration-1989.pdf) | John R. Carlson and Milton Lamb, Integration Effects of Pylon Geometry on a High-Wing Transport Airplane, NASA Langley Research Center, 1989-02-01, NASA-TP-2877, L-16489, NAS 1.60:2877, [NTRS record](https://ntrs.nasa.gov/citations/19890006517), [official PDF](https://ntrs.nasa.gov/api/citations/19890006517/downloads/19890006517.pdf) | NTRS distribution PUBLIC; GOV_PUBLIC_USE_PERMITTED | 78; 2554774; 5e8bb00caff8eefae7bc713ec343c14dd8a5a945d92a724fd77745824c129d98 |
| [nasa-aeroacoustics-pai-thomas-2003.pdf](../../bib/propulsion-energy/nasa-aeroacoustics-pai-thomas-2003.pdf) | Russell H. Thomas, Aeroacoustics of Propulsion Airframe Integration: Overview of NASA’s Research, NASA Langley Research Center, 2003-01-01, NOISE-CON Paper 105, [NTRS record](https://ntrs.nasa.gov/citations/20030065859), [official PDF](https://ntrs.nasa.gov/api/citations/20030065859/downloads/20030065859.pdf) | NTRS distribution PUBLIC; PUBLIC_USE_PERMITTED | 8; 953888; ef0300c7e4fa7b78f76e9a7171293809777543fbe68eeff2af566cb47db3e299 |

### FAA

| Local PDF | Complete citation metadata | Rights/provenance | Pages; bytes; SHA-256 |
| --- | --- | --- | --- |
| [faa-ac-25-901-1-powerplant-installations-2024.pdf](../../bib/propulsion-energy/faa-ac-25-901-1-powerplant-installations-2024.pdf) | Federal Aviation Administration, AC 25.901-1 Safety Assessment of Powerplant Installations, 2024-08-30, Advisory Circular AC 25.901-1, [FAA record](https://www.faa.gov/regulations_policies/advisory_circulars/index.cfm/go/document.information/documentID/1043036), [official PDF](https://www.faa.gov/documentLibrary/media/Advisory_Circular/AC_25.901-1.pdf) | Official FAA government guidance; advisory circular is guidance and has no force of law; local research copy | 26; 1337843; 1a57382a4bd77e74c6e69d63aa74de69995b558479735e0c541725f916befca5 |
| [faa-ac-33-2b-engine-type-certification-1993.pdf](../../bib/propulsion-energy/faa-ac-33-2b-engine-type-certification-1993.pdf) | Federal Aviation Administration, AC 33-2B Aircraft Engine Type Certification Handbook, 1993-06-30, Advisory Circular AC 33-2B, [FAA record](https://www.faa.gov/regulations_policies/advisory_circulars/index.cfm/go/document.information/documentID/22777), [official PDF](https://www.faa.gov/documentlibrary/media/advisory_circular/ac%2033-2b.pdf) | Official FAA government publication; cancelled in the FAA index, retained as historical background only | 123; 8000665; 79208d8a4dc577888141cb7d6f531897e893b4fd06ede7d10209b0a00f262f17 |
| [faa-saf-redac-2019-03.pdf](../../bib/propulsion-energy/faa-saf-redac-2019-03.pdf) | Federal Aviation Administration REDAC Environment & Energy, Alternative Jet Fuels R&D and ASCENT Analysis, 2019-03, REDAC Environment & Energy 2019-03, [FAA record](https://www.faa.gov/about/officeorg/headquartersoffices/ang/redac-environment-and-energy-2019-03-alternative-jet-fuels), [official PDF](https://www.faa.gov/sites/faa.gov/files/2022-07/eeSC-Mar2019-SustainableAviationFuels(SAF).pdf) | Official FAA government publication; local research copy | 19; 3445878; edfdec3b021c2354ab84115ba9d38ce12886087560895eb84f2800aab62daffa |

### EASA

| Local PDF | Complete citation metadata | Rights/provenance | Pages; bytes; SHA-256 |
| --- | --- | --- | --- |
| [easa-cs-e-amendment-8-2025-correction-2026.pdf](../../bib/propulsion-energy/easa-cs-e-amendment-8-2025-correction-2026.pdf) | European Union Aviation Safety Agency, CS-E Amendment 8, publication 2025-04-09; file replaced 2026-01-09 with correction highlighted on page 121; [EASA CS-E group](https://www.easa.europa.eu/en/document-library/certification-specifications/group/cs-e-engines), [official PDF download](https://www.easa.europa.eu/en/downloads/141875/en) | Official EASA certification specification, freely accessible; no open redistribution licence assumed; use the latest official source for compliance | 262; 6039894; 8f6baac61d416e2cb88f88fb8cc63da802b64751f1fb150342dc95a15f54d80b |
| [easa-cs-25-amendment-28-2023-correction-2025.pdf](../../bib/propulsion-energy/easa-cs-25-amendment-28-2023-correction-2025.pdf) | European Union Aviation Safety Agency, CS-25 Amendment 28, publication 2023-12-19; file replaced 2025-11-20 with correction highlighted on page 1171; [EASA CS-25 group](https://www.easa.europa.eu/en/document-library/certification-specifications/group/cs-25-large-aeroplanes), [official PDF download](https://www.easa.europa.eu/en/downloads/139073/en) | Official EASA certification specification, freely accessible; no open redistribution licence assumed; use the latest official source for compliance | 1515; 23584619; 32f1a9acf26e8ceceb291d206bf07a7071f7f59f40f10c096bb17f34c6a58007 |
| [easa-eaer-2025.pdf](../../bib/propulsion-energy/easa-eaer-2025.pdf) | EASA, European Aviation Environmental Report 2025, 2025, [EASA report page](https://www.easa.europa.eu/en/domains/environment/eaer), [official PDF](https://www.easa.europa.eu/sites/default/files/eaer-downloads/EASA_EAER_2025_Book_v5.pdf) | Official EASA report freely accessible for reference; no open redistribution licence assumed | 200; 27993811; 4c3adb8ad9350f409db401cc334f44c4b7b0ecc24d1ea758b534c3ddd67d4800 |

### DLR and open academic papers

| Local PDF | Complete citation metadata | Rights/provenance | Pages; bytes; SHA-256 |
| --- | --- | --- | --- |
| [dlr-fouda-automated-hybrid-electric-architecture-2024.pdf](../../bib/propulsion-energy/dlr-fouda-automated-hybrid-electric-architecture-2024.pdf) | Mahmoud Essam Abdelmoneam Fouda, Automated Hybrid-Electric Propulsion Architecture Modeling for Conceptual Aircraft Design: A Novel Approach to Integrating System Architecting in MDO, Master’s thesis, Middle East Technical University, DLR eLib, 2024-01-09, 107 pp, [repository record](https://elib.dlr.de/203543/), [official handle](https://hdl.handle.net/11511/108190), [official PDF](https://elib.dlr.de/203543/1/AUTOMATED%20HYBRID-ELECTRIC%20PROPULSION%20ARCHITECTURE%20MODELING%20FOR%20CONCEPTUAL%20AIRCRAFT%20DESIGN%20-%20A%20NOVEL%20APPROACH%20TO%20INTEGRATING%20SYSTEM%20ARCHITECTING%20IN%20MDO.pdf) | Repository metadata states Open Access and CC BY-NC-ND; derivatives/redistribution must preserve that licence | 107; 3326275; 30e8fe14c66d45dfe058e4d8bb5cbb33fda1b2b3666e77483f6f2813995abf0a |
| [mdpi-kellermann-tms-2020.pdf](../../bib/propulsion-energy/mdpi-kellermann-tms-2020.pdf) | Hagen Kellermann, Michael Lüdemann, Markus Pohl, and Mirko Hornung, Design and Optimization of Ram Air-Based Thermal Management Systems for Hybrid-Electric Aircraft, Aerospace 8(1), article 3, 2020-12-23, DOI 10.3390/aerospace8010003, [article](https://www.mdpi.com/2226-4310/8/1/3), [official PDF](https://mdpi-res.com/d_attachment/aerospace/aerospace-08-00003/article_deploy/aerospace-08-00003-v2.pdf) | CC BY 4.0; DOI metadata licence [record](https://api.crossref.org/works/10.3390/aerospace8010003) | 22; 781438; 03b7cecf37c7fb452c59e755432fe5f8cddf0b6905413664c1553fad60486f82 |
| [mdpi-coutinho-tms-2023.pdf](../../bib/propulsion-energy/mdpi-coutinho-tms-2023.pdf) | Maria Coutinho, Frederico Afonso, Alain Souza, David Bento, Ricardo Gandolfi, Felipe R. Barbosa, Fernando Lau, and Afzal Suleman, A Study on Thermal Management Systems for Hybrid-Electric Aircraft, Aerospace 10(9), article 745, 2023-08-23, DOI 10.3390/aerospace10090745, [article](https://www.mdpi.com/2226-4310/10/9/745), [official PDF](https://mdpi-res.com/d_attachment/aerospace/aerospace-10-00745/article_deploy/aerospace-10-00745-v2.pdf) | CC BY 4.0; DOI metadata licence [record](https://api.crossref.org/works/10.3390/aerospace10090745) | 24; 1320739; 39fd97e94c852d582a9317ca04ba8fa3e10aa4c767f1f5b236b27907c108463e |
| [mdpi-habermann-electrically-assisted-turboshaft-2023.pdf](../../bib/propulsion-energy/mdpi-habermann-electrically-assisted-turboshaft-2023.pdf) | Anaïs Luisa Habermann, Moritz Georg Kolb, Philipp Maas, Hagen Kellermann, Carsten Rischmüller, Fabian Peter, and Arne Seitz, Study of a Regional Turboprop Aircraft with Electrically Assisted Turboshaft, Aerospace 10(6), article 529, 2023-06-02, DOI 10.3390/aerospace10060529, [article](https://www.mdpi.com/2226-4310/10/6/529), [official PDF](https://mdpi-res.com/d_attachment/aerospace/aerospace-10-00529/article_deploy/aerospace-10-00529.pdf) | CC BY 4.0; DOI metadata licence [record](https://api.crossref.org/works/10.3390/aerospace10060529) | 30; 5870205; 6f051e3a8400ad134de2261a1df9bac6a63f9ba114533267969ccb9e93134f5a |
| [mdpi-staats-hybrid-electric-d328-2025.pdf](../../bib/propulsion-energy/mdpi-staats-hybrid-electric-d328-2025.pdf) | Annika Nora Staats, Florian Troeltsch, and Andreas Bardenhagen, Conceptual Design of a Hybrid-Electric Aircraft Based on a Dornier 328 Demonstrator, Aerospace 12(12), article 1085, 2025-12-04, DOI 10.3390/aerospace12121085, [article](https://www.mdpi.com/2226-4310/12/12/1085), [official PDF](https://mdpi-res.com/d_attachment/aerospace/aerospace-12-01085/article_deploy/aerospace-12-01085-v2.pdf) | CC BY 4.0; DOI metadata licence [record](https://api.crossref.org/works/10.3390/aerospace12121085) | 13; 963967; 6442eaf6bc55060e7e560a50fa8ad363dc1d66494b443aedf3e03da9010c64c3 |

## 10. Cited but not downloaded

These sources informed the synthesis but were deliberately not copied into bib/propulsion-energy because the accessible copy was a proceedings scan, an internal/limited-access record, or did not expose a clear local redistribution licence.

| Source | Metadata and reason for citation-only treatment |
| --- | --- |
| H. Wittenberg, Prediction of Off-Design Performance of Turbojet and Turbofan Engines, in AGARD-CP-242 Performance Prediction Methods, May 1978, pp. 4-1–4-31, ISBN 92-835-1282-0, N78-26077 | Historical off-design method using gas-dynamics relationships and throat assumptions. The article appears in an AGARD proceedings scan indexed by [NASA NTRS](https://ntrs.nasa.gov/api/citations/19790004826/downloads/19790004826.pdf?attachment=true); the scan’s local rights for all proceedings content were not independently verified. |
| H. I. H. Saravanamuttoo, Overview on Basis and Use of Performance Prediction Methods, AGARD Lecture Series 183, 1992 | Steady-state component matching and transient-performance motivation. The NTRS record warns that portions may include copyright-protected material: [record](https://ntrs.nasa.gov/citations/19920019216). |
| AGARD-AR-245, Recommended Practices for Measurement of Gas Path Pressures and Temperatures for Performance Assessment of Aircraft Turbine Engines and Components, 1990; AGARD-AR-320, Guide to the Measurement of the Transient Performance of Aircraft Turbine Engines and Components, 1994 | Measurement uncertainty and transient validation references. Official NATO publication copies are accessible, but local redistribution terms were not established: [NATO STO publications](https://publications.sto.nato.int/). |
| Eric S. Hendricks and Michael T. Tong, Performance and Weight Estimates for an Advanced Open Rotor Engine, NASA/TM-2012-217710, AIAA Paper 2012-3911, 2012 | Open-rotor cycle/weight/integration precedent. The NTRS record is [20120014381](https://ntrs.nasa.gov/citations/20120014381); its local copyright determination was not explicit enough for this corpus. |
| NASA, Progress in Open Rotor Research: A U.S. Perspective, 2015 | Open-rotor acoustic, structural, propulsion-airframe-integration, and certification context; [NTRS search](https://ntrs.nasa.gov/search?q=20150022391). Not downloaded because local rights were not confirmed. |
| NASA, Turboelectric Distributed Propulsion in a Hybrid Wing Body Aircraft, 2012; NASA STARC-ABL material, 2021 | Distributed propulsion, wake ingestion, nacelle/pylon, and coupled-airframe precedents: [TeDP NTRS record](https://ntrs.nasa.gov/citations/20120000856), [STARC-ABL NTRS record](https://ntrs.nasa.gov/citations/20210016661). Not downloaded because the accessible records did not establish a clear local redistribution licence for the complete files. |
| Iwanizki et al., Conceptual Design Studies of Short Range Aircraft Configurations with Hybrid Electric Propulsion, AIAA Aviation 2019, DOI 10.2514/6.2019-3680 | DLR architecture down-selection and low-physics conceptual workflow: [DLR record](https://elib.dlr.de/128761/). Repository marks the PDF DLR-internal/Open Access No. |
| Strack et al., Conceptual Design Assessment of Advanced Hybrid Electric Turboprop Aircraft Configurations, AIAA 2017, DOI 10.2514/6.2017-3068 | DLR regional hybrid-turboprop architecture and mission study: [DLR record](https://elib.dlr.de/112892/). Repository marks the PDF DLR-internal/Open Access No. |
| Georgi Atanasov, Concept Introduction: 70 PAX Plug-In Hybrid-Electric Aircraft (D70-PHEA), DLR EXACT Mid-Term Review, 2022 | Useful public DLR architecture presentation with range-extender and distributed propellers: [repository record](https://elib.dlr.de/193116/). The record says Open Access but does not expose an explicit licence, so it was cited only. |
| DLR, Research for climate-compatible aviation; DLR, Future climate-friendly air transport | Official context for hydrogen, electrification, sustainable fuels, distributed propulsion, and climate/non-CO2 assumptions: [DLR climate-compatible aviation](https://www.dlr.de/en/research-and-transfer/featured-topics/climate-compatible-aviation/), [SynergIE article](https://www.dlr.de/en/latest/news/2021/04/20211122_hybrid-electric-propulsion-enable-more-climate-friendly-air-transport). |
| FAA, Installation Requirements for Aircraft Engines; FAA, Aircraft Engines Original Design Approvals; FAA CLEEN; FAA AEDT | Current authority/process and environmental-tool boundaries: [installation requirements](https://www.faa.gov/aircraft/air_cert/design_approvals/engine_prop/engine_approvals/install_req), [engine approvals](https://www.faa.gov/aircraft/air_cert/design_approvals/engine_prop/engines_oda), [CLEEN](https://www.faa.gov/about/office_org/headquarters_offices/apl/eee/technology_saf_operations/cleen), [AEDT](https://aedt.faa.gov/4a_information.aspx). Web pages were cited rather than mirrored. |
| EASA, How sustainable are SAF?; EASA Easy Access Rules pages | Lifecycle and non-CO2 context plus current consolidated rules: [SAF environmental page](https://www.easa.europa.eu/en/domains/environment/eaer/sustainable-aviation-fuels/how-sustainable-are-saf), [CS-E Easy Access Rules](https://www.easa.europa.eu/en/document-library/easy-access-rules/easy-access-rules-engines-cs-e), [CS-25 Easy Access Rules](https://www.easa.europa.eu/en/document-library/easy-access-rules/easy-access-rules-large-aeroplanes-cs-25). The official pages should be consulted again when a compliance basis is defined because eRules documents can be updated. |

## 11. Verification notes

The new research artifact and the bibliography directory are the only requested source-side additions. No existing Rust, configuration, test, report, or documentation file was edited, and no existing file was deleted. The downloaded PDFs were checked locally with PDF signature validation, Poppler page-count inspection, and SHA-256 hashing. Full workspace tests and the repository gate were not run because this change adds only research material and does not alter executable source.
