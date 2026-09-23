# Environmental and lifecycle modelling for requirements-first aircraft conceptual design

Research report for ALAS
Prepared: 2026-08-26
Scope: fuel burn and CO₂, non-CO₂ climate effects, airport noise and procedures, lifecycle assessment, materials/manufacturing/end-of-life, SAF, hydrogen, electric energy, and requirements/objective integration.

## Executive conclusion

Environmental performance is not one scalar property of an aircraft. It is a vector of results with different system boundaries, time scales, denominators, and evidence levels:

1. **Mission energy and fuel burn** are physical closure quantities. They belong in every candidate evaluation.
2. **Combustion CO₂** is a derived tank-to-wake quantity. It should be calculated once from the fuel or carbon inventory.
3. **Upstream fuel and energy impacts** belong to a separate well-to-wake or lifecycle calculation. SAF feedstock, process energy, hydrogen production, electricity mix, and battery manufacture must not be hidden in the flight-fuel number.
4. **NOₓ, nvPM/soot, SOₓ, water vapour, ozone/methane chemistry, and contrails** are not interchangeable. Early ALAS can screen species and contrail susceptibility, but a single fixed “non-CO₂ multiplier” should not be an optimizer objective.
5. **Noise** is both an aircraft-source problem and an airport-exposure problem. Certification event metrics, single-event community metrics, annual contours, population exposure, and procedure choices are distinct outputs.
6. **Lifecycle assessment (LCA)** must declare goal, functional unit, system boundary, allocation, data quality, and end-of-life credits. It should include production, operation, maintenance, replacement, and disposal only when each stage is represented and traceable.
7. **Energy-carrier feasibility** is a hard physical screen before it is an environmental preference. Battery usable energy, peak power, thermal rejection, hydrogen tank volume/temperature/boil-off, and airport turnaround can eliminate an architecture even when its operational emissions look attractive.

The implementable ALAS strategy is therefore a staged environmental evaluator. It starts with the existing mission and propulsion results, adds species and noise diagnostics at low order, adds parameterised LCA and energy-carrier accounting, and promotes only finalists to external tools or higher-fidelity climate and airport models. Every metric returns a value, unit, load case, model/fidelity, source, uncertainty, policy, and residual.

## Resume and evidence status

The requested directory did not exist in the resumed checkout. The relevant PDFs were present in the repository’s existing topic folders, especially propulsion-energy, operations-economics, performance-airport, and mission-sizing. To make this deliverable self-contained, 16 byte-identical local copies were staged in [bib/environment-lifecycle](../../bib/environment-lifecycle/). No external literature search or web download was restarted, no existing source file was edited, and nothing was deleted.

The local PDFs are reference evidence, not automatically licensed project assets. Rights notes below are deliberately conservative:

- NASA NTRS records identify public distribution or public-use permission for the NASA reports listed here.
- MDPI and Cambridge University Press articles identify open licences in the PDFs or publisher records.
- The EASA report and ICAO manual are public downloads but explicitly reserve rights; they are retained as local research copies.
- The DLR thesis record states CC BY-NC-ND; the licence permits reading and attribution but constrains derivatives and redistribution.

The report also records important standards, projects, and papers that were identified in the prior research pass but are not in the local evidence directory. They are labelled citation-only or unresolved rather than being presented as locally verified PDFs.

## ALAS starting point and boundary

The primary traceability anchor is the repository’s [requirements-first aircraft design document](../REQUIREMENTS_FIRST_AIRCRAFT_DESIGN.md). It gives each technical requirement a value and unit, load case, policy, and evidence/status, and distinguishes hard minimum/maximum constraints, soft targets, objectives, and diagnostics. The pipeline boundary is [crates/alas-pipeline/src/pipeline.rs](../../crates/alas-pipeline/src/pipeline.rs); feasibility and ranking are implemented around [crates/alas-opt/src/feasibility.rs](../../crates/alas-opt/src/feasibility.rs) and [crates/alas-opt/src/objective_model.rs](../../crates/alas-opt/src/objective_model.rs).

The existing flow already has the right broad order:

1. intent and preset;
2. DesignBrief/TLAR validation;
3. architecture and preliminary sizing;
4. geometry, cabin, mass, payload, centre of gravity, fuel;
5. aerodynamics, trim, stability, performance, mission, airport, and structure;
6. residuals, feasibility, ranking, finalist high-fidelity analysis, and reporting.

Environmental hooks fit into that flow without making the environmental module a second aircraft solver:

- mission and propulsion provide the primary fuel, power, thrust, profile, and operating-point quantities;
- atmosphere and trajectory provide altitude, temperature, time, position, and meteorological scenario;
- geometry, aero, propulsion installation, and operating procedures provide acoustic source inputs;
- mass and structures provide material/component inventories;
- configuration provides the fuel pathway, electricity mix, hydrogen pathway, fleet/airport scenario, and policy;
- alas-opt receives only named environmental residuals and objective terms after their ownership is explicit;
- alas-report and alas-viz publish the vector of metrics, uncertainty, assumptions, and missing evidence.

The current mission code explicitly documents that Noise.compute_noise returns immediately because no noise analysis is attached. That is a useful honest boundary: the environmental report should expose noise as not evaluated until an evaluator is connected, rather than implying that a fuel-only mission result is a noise result.

## State of the art and implications for ALAS

### 1. Governance, certification, and assessment practice

ICAO’s Committee on Aviation Environmental Protection (CAEP) separates the principal regulatory and assessment subjects across Annex 16 volumes: aircraft noise, aircraft engine emissions, aeroplane CO₂ emissions, and CORSIA. The associated technical manuals and airport guidance are not interchangeable:

- noise certification is an aircraft/type-level event assessment;
- engine emissions certification is an engine/LTO assessment with defined operating points;
- CO₂ certification is a metric for aeroplane fuel efficiency under a prescribed test method;
- CORSIA fuel accounting is a lifecycle sustainability and emissions-accounting scheme, not a replacement for physical flight fuel burn;
- airport air quality includes aircraft and non-aircraft sources, spatial/temporal distributions, and dispersion;
- the ICAO Balanced Approach treats source reduction, land-use planning, noise-abatement procedures, and operating restrictions as airport-specific measures.

The [ICAO Airport Air Quality Manual, Doc 9889](https://www.icao.int/sites/default/files/2025-04/9889_cons_en.pdf) is especially relevant to ALAS because it has separate chapters for temporal/spatial emissions, dispersion modelling, reporting, and interrelationships between fuel, noise, and local-air-quality mitigation. It also lists aircraft and airport sources that are outside a simple engine LTO inventory, such as APU, ground support, road traffic, brake/tire wear, and start-up effects. The manual is a 2020 second edition and its local PDF says © ICAO 2020, all rights reserved; it must be treated as guidance background, not as a permissive data licence or current regulatory substitute.

FAA practice provides a useful operational decomposition. The [2021 U.S. Aviation Climate Action Plan](https://www.faa.gov/general/2021-united-states-aviation-climate-action-plan) covers efficient aircraft, operations and contrails, SAF, hydrogen/electric short-haul concepts, airports, international measures, and climate research. The current FAA AEDT family is an important reference for a later airport model because it combines aircraft performance, fuel burn, noise, emissions inventory, dispersion, ground operations, annualisation, and impact metrics. AEDT output is not a single aircraft-design number: it needs an airport, fleet/operations schedule, tracks, time-of-day, weather, and population/context.

The [European Aviation Environmental Report 2025](https://www.easa.europa.eu/en/domains/environment/eaer), locally copied and hashed below, has separate chapters for climate change, air quality, noise, technology/design, ATM/operations, airports, SAF, and mitigation. Its indicators distinguish full-flight CO₂, “net” CO₂ under stated assumptions, NOₓ, noise-contour population, and fuel consumption per passenger-distance. That separation is the correct pattern for ALAS. A “net CO₂” figure that includes market measures or SAF assumptions must never replace the physical flight CO₂ result.

European research programmes reinforce the same system boundary:

- Clean Aviation’s Technology Evaluator reports environmental effects at mission, airport, and fleet levels; its scope includes CO₂, NOₓ, and noise rather than only aircraft block fuel.
- Clean Aviation’s strategic agenda treats SAF, hydrogen, and electric/hybridisation as technology pathways with infrastructure and maturity constraints.
- DLR’s ALICIA project is a digital lifecycle and impact-assessment platform spanning aircraft, airport, and air-traffic-system scenarios and including production, operation, MRO, end-of-life, energy/fuel, and social/economic dimensions.
- DLR EXACT studies combine aircraft design, energy, and climate-impact analysis across future concepts.
- NASA’s Sustainable Flight and Model-Based Systems Analysis and Engineering work demonstrates the value of a typed, multi-fidelity, systems-level model with uncertainty rather than a single high-fidelity analysis called for every candidate.

### 2. Fuel burn and CO₂

The most defensible first environmental metric is the same quantity needed to close the mission:

    block_fuel = sum(segment_fuel_mass) + explicitly modelled reserve policy

The report must distinguish at least:

- trip fuel versus block fuel;
- reserve fuel versus fuel consumed in the nominal trajectory;
- fuel mass versus fuel energy;
- tank-to-wake combustion CO₂ versus well-to-wake/lifecycle greenhouse gas result;
- per-aircraft, per-flight, per-passenger, and per-passenger-distance denominators.

For a conventional hydrocarbon fuel, direct CO₂ can be calculated from the consumed fuel mass and a documented carbon-content/emission factor:

    CO2_TTW = fuel_mass × CO2_factor

The factor belongs in a versioned factor registry, not as an unexplained literal. The factor must identify the fuel definition, carbon content, rounding, and whether it is intended for regulatory inventory or conceptual comparison. For SAF, the physical combustion CO₂ should remain in the tank-to-wake result. The upstream credit belongs in a separate lifecycle pathway calculation.

The CORSIA lifecycle structure is a useful conceptual partition even when ALAS is not implementing CORSIA compliance: core lifecycle emissions cover feedstock cultivation/collection, processing, transport, conversion, distribution, and combustion; indirect land-use change and approved credits are separate terms; default and actual lifecycle values are different evidence modes. ALAS should carry these as named components:

    CO2e_WTW = CO2e_feedstock
              + CO2e_process_energy
              + CO2e_transport_and_distribution
              + CO2e_combustion
              + CO2e_ILUC
              − approved_credits

The sign and eligibility of credits must follow the selected methodology version. A generic “SAF percentage” is not enough: two pathways with the same blend fraction can have different feedstocks, process energy, land-use treatment, hydrogen source, transport, and evidence.

For ranking aircraft concepts, use a fixed mission, payload, reserve, load factor, and reference baseline. Otherwise an aircraft can appear environmentally better by carrying less payload, using a shorter mission, or omitting the reserve convention.

### 3. Non-CO₂ climate effects and air quality

The useful species inventory is broader than CO₂:

- NOₓ;
- non-volatile particulate matter (nvPM), soot number, and soot mass;
- SOₓ/sulphur species;
- water vapour;
- CO and unburned hydrocarbons;
- fuel-burn-derived CO₂.

At low order, a species inventory can be calculated at each mission operating point:

    mass_species_i = sum_over_segments(fuel_flow × EI_i × dt)

where EI_i is an emission index with units and validity conditions. LTO and cruise must be kept separate. NOₓ at the airport is a local-air-quality inventory; NOₓ-induced ozone and methane changes are climate effects requiring a chemistry/transport response model. They should not be summed into one “NOₓ penalty”.

Contrails and contrail cirrus depend on aircraft/engine emissions, soot number and composition, humidity and temperature, flight altitude, latitude, time, and trajectory. The open contrail literature (Lee et al.; Burkhardt et al.; Teoh et al.; Megill et al.; Matthes et al.) consistently supports three implementation rules:

1. do not apply one universal CO₂-equivalent multiplier to every route or aircraft;
2. preserve the trajectory and meteorological state that generated a risk result;
3. report uncertainty and model horizon when converting a climate response into RF, ATR, GWP, GWP*, or another metric.

The appropriate ALAS fidelity ladder is:

| Fidelity | Model | Output | Use |
| --- | --- | --- | --- |
| N0 | species factors by fuel/engine mode | CO₂, NOₓ, nvPM, SOₓ, H₂O masses | fast screening and bookkeeping |
| N1 | altitude/time/latitude susceptibility map plus soot-sensitive proxy | contrail opportunity/risk index, affected distance/time | architecture and route screening |
| N2 | trajectory and weather ensemble with CoCiP-like persistent-contrail model | contrail coverage, RF or ATR distribution | finalist trade study |
| N3 | calibrated engine emissions, chemistry/transport, plume microphysics, and operational avoidance | scenario-specific climate response | research substantiation; not the default optimizer loop |

At N0/N1, the result should be a diagnostic or a bounded sensitivity, not a hard pass/fail. A hard requirement is appropriate only when the organization has named a metric, horizon, scenario, threshold, and verification method.

Fuel composition can affect nvPM, sulphur, and contrail properties, but the effect is not a free operational credit. The evaluator must retain the fuel pathway and combustion assumptions and report the uncertainty in the response.

### 4. Noise footprint and airport procedures

Noise is created by engine/jet/fan, propeller, airframe, high-lift devices, landing gear, and their installation/interactions. The NASA propulsion-airframe aeroacoustics paper in the local corpus is a useful early-design reminder that pylon geometry, wing shielding/reflection, flaps, exhaust interaction, and installation can change the radiated field. An electric motor can remove combustion/jet noise but does not remove propeller/fan, gearbox, airframe, high-lift, or installation noise; high rotational speed and cooling flow can remain important.

Keep the following metrics separate:

- certification event metrics such as EPNL at prescribed approach, sideline, and flyover points;
- single-event community metrics such as SEL, LAmax, or related A-weighted measures;
- annual exposure metrics such as DNL/Ldn or Lden;
- population inside a contour;
- contour area and location;
- procedure cost in fuel, time, emissions, and capacity.

An individual event sound level cannot be added arithmetically to an annual contour or population metric. Decibel reductions must be applied to the source/propagation model before exposure aggregation.

The noise fidelity ladder is:

| Fidelity | Model | Output | Use |
| --- | --- | --- | --- |
| Q0 | source-power/thrust/rpm proxy with distance and shielding flags | relative event-noise index | architecture and installation screening |
| Q1 | empirical source tables or NPD/ANP-style model along the ALAS flight profile | event SEL/EPNL/LAmax at fixed observer points | preliminary procedure and source trades |
| Q2 | AEDT/ANP-like airport model with tracks, runway, procedures, day/night, and population | DNL/Lden contours, population, exposure | airport-specific design assessment |
| Q3 | certification-grade source separation, installation, propagation, and approved procedure | compliance evidence | later certification work |

The procedure is part of the environmental design space. Noise-abatement departure profiles, thrust cutback, continuous descent, runway selection, approach speed, flap schedule, and lateral routing can reduce one impact while increasing fuel, NOₓ, local air quality, delay, or community exposure elsewhere. ICAO’s Balanced Approach and Doc 9889’s interrelationship chapter support evaluating the trade explicitly rather than hard-coding one procedure.

### 5. Lifecycle assessment, materials, manufacturing, MRO, and end-of-life

An ALAS LCA needs a declared goal and scope before it needs more detail. Recommended functional units are:

- one aircraft over a stated service life for production/MRO/end-of-life;
- one completed design mission at a stated payload and reserve;
- one passenger-kilometre or tonne-kilometre for fleet comparison.

Do not mix these functional units in one score. If the aircraft life is the functional unit, service life, annual utilisation, replacement schedule, load factor, and retirement route must be explicit.

The minimum cradle-to-grave stage inventory is:

1. raw material extraction and refining;
2. material production;
3. part manufacture, forming, machining, lay-up, curing, joining, and scrap;
4. final assembly and delivery;
5. energy/fuel production;
6. aircraft operation;
7. scheduled maintenance, repair, overhaul, and replacement parts;
8. airport or energy infrastructure, when in scope;
9. retirement, reuse, recycling, downcycling, incineration, landfill, and credits.

The important aircraft-design variables are not just empty mass. The inventory should retain material family and process for each major component, part count, manufacturing yield, scrap rate, joining/adhesive process, replacement interval, and end-of-life route. Composite mass can lower fuel burn but may have different manufacturing energy and recycling/downcycling burdens. Battery and cryogenic-tank systems add upstream material, manufacturing, containment, and replacement impacts. A lightened structure should not receive an environmental credit twice: once through lower mission fuel and again through an unexamined generic “lightweight material” factor.

The open aviation LCA literature identified in the prior research pass provides a practical progression:

- Dallara, Kusnitz, and Bradley introduced parametric LCA for aircraft design;
- Vivalda and Fioriti provide a stream LCA model for preliminary aircraft design with production, operation, MRO, disposal, and uncertainty;
- DLR ALICIA work addresses fast, data-efficient lifecycle integration using aircraft parameters, discrete-event simulation, and surrogate prediction;
- DLR’s aviation life-cycle-inventory work highlights gaps in background data, MRO, end-of-life, non-CO₂, confidentiality, and representativeness;
- material-selection and end-of-life studies show that circularity results are component- and process-specific, so an aircraft-level recycling percentage is not a sufficient LCA input.

For ALAS, the initial LCA can use component families and transparent factors:

    LCA_stage = sum(component_mass × material_process_factor)
               + sum(energy_or_fuel_flow × pathway_factor)
               + MRO_replacements
               + EOL_and_credits

Each factor must identify geography, year, database/version, allocation, and data-quality grade. OpenLCA/Brightway or licensed inventory databases should be adapters, not implicit dependencies of the first screening loop. Until licensed LCI data are selected, ALAS should report a parametric LCA range and data gaps rather than a spurious precise number.

Non-CO₂ climate effects may be included in a lifecycle climate result only through a named, compatible metric and boundary. If a full LCA includes use-stage CO₂ and upstream fuel emissions, those terms must be removed from any separate sum before presentation.

### 6. SAF, hydrogen, battery-electric, and hybrid energy constraints

The aircraft energy system is a coupled mass-volume-power-thermal-infrastructure problem.

For any carrier, the mission closure should distinguish:

- delivered energy at the source;
- usable energy after reserve, state-of-charge/depth-of-discharge, boil-off, and conversion losses;
- peak power and continuous power;
- storage mass and volume;
- thermal rejection and cooling drag;
- centre-of-gravity travel;
- turnaround, charging, refuelling, and airport availability.

For battery-electric systems, a first-order closure is:

    E_usable = m_battery × specific_energy_pack × usable_fraction × chain_efficiency
    m_battery >= E_required / (specific_energy_pack × usable_fraction × chain_efficiency)

The value must be pack-level and temperature/aging qualified, not cell-level nameplate energy. Peak power and heat rejection can set a larger battery, motor, inverter, cable, or radiator than the energy equation alone. NASA’s hybrid-regional study found that battery specific energy is a decisive gate; its threshold is concept- and assumption-specific and must not be reused as a universal requirement. The local D328 and electrically assisted turboshaft studies likewise show that battery properties, thermal management, and non-propulsive electrical loads can erase a nominal fuel saving.

For hydrogen:

    m_H2 = E_flight / (LHV_H2 × chain_efficiency)
    V_tank = m_H2 / usable_volumetric_density
    m_system = m_H2 + m_tank + insulation + pumps + heat_exchangers + controls

Hydrogen has high gravimetric energy but low volumetric density relative to kerosene. Tanks, insulation, cryogenic equipment, boil-off, cabin/airframe volume, centre-of-gravity movement, airport storage, and safety zones are aircraft-level constraints. Hydrogen combustion can still produce NOₓ and water and may alter contrail behaviour; fuel-cell systems trade efficiency against mass, volume, thermal rejection, and electrical power density. “Zero in-flight CO₂” is not “zero climate impact”.

For SAF, the aircraft fuel system is usually close to a liquid hydrocarbon system at the first fidelity, but the model must retain blend/pathway/certification and supply assumptions. The environmental difference is primarily in the lifecycle pathway and potentially in species/nvPM behaviour, not in removing physical combustion from the mission.

For hybrid-electric systems, energy conservation must be explicit:

    fuel_energy + battery_energy = propulsive_energy
                                + conversion_losses
                                + thermal_rejection
                                + reserve_energy

Do not count battery energy as both an environmental benefit and a second “fuel saving” if the mission solver has already reduced fuel flow through the power split. The architecture model should expose the power-source topology, rather than infer it from a scalar hybridisation fraction.

## Staged implementable ALAS model

### Stage E0: Environmental brief and evidence contract

Add an environmental brief adjacent to the DesignBrief. It should contain:

- baseline aircraft and technology year;
- mission, payload, reserve, load factor, and service-life definition;
- selected fuel/energy carrier and pathway;
- airport, route, weather, fleet, and population scenario;
- functional unit for each LCA metric;
- factor registry version and source IDs;
- requested fidelity for CO₂, species, contrail, noise, LCA, and energy;
- policy for every metric: hard, soft, objective, or diagnostic;
- evidence/status and rights/provenance status.

Validation should reject missing units, denominators, incompatible functional units, unsupported carrier combinations, and unqualified credits. A placeholder should be a visible diagnostic.

### Stage E1: Deterministic mission energy and fuel baseline

Reuse the existing mission/propulsion outputs. At each segment/control point, store time, distance, altitude, atmosphere, mass, thrust or shaft power, fuel flow, electric power, and reserve state. Produce:

- trip fuel and block fuel;
- fuel energy and electric energy;
- direct CO₂;
- per-passenger/per-distance values using the frozen payload and load-factor convention;
- baseline deltas;
- an explicit extrapolation/failure status.

This is the only stage that should be required for every candidate in a conventional aircraft study.

### Stage E2: Species inventory and local-air-quality screen

Attach emission indices to the propulsion evaluator by engine mode and carrier. Separate LTO, climb, cruise, descent, holding, APU, and ground equipment when those sources are in scope. Return:

- CO₂, NOₓ, nvPM, SOₓ, CO, HC, and H₂O mass by phase;
- LTO totals and cruise totals;
- source/factor/model version;
- whether the value is calibrated, empirical, or a placeholder.

Use the ICAO airport-air-quality structure for source and spatial/temporal bookkeeping. Do not call a CEA thermochemical calculation an emissions certification result; CEA can support properties and combustion screening, while calibrated EI requires an engine/emissions model or deck.

### Stage E3: Non-CO₂ climate screen

Use the complete flight profile, including altitude, time, position and meteorological scenario, to calculate:

- ice-supersaturation/contrail opportunity;
- soot-sensitive relative impact;
- altitude/rerouting sensitivity;
- a confidence interval or scenario envelope.

Keep RF/ATR/GWP outputs optional and named. If a contrail-avoidance optimizer is later added, its objective should include extra fuel and NOₓ consequences and should be evaluated on the same weather/route ensemble. A route-specific climate diagnostic is not a universal aircraft requirement.

### Stage E4: Noise source and procedure screen

At Q0/Q1, predict source levels or relative acoustic power from thrust, fan/propeller rotational speed, jet velocity, airframe configuration, high-lift/gear state, and installation/shielding flags. Propagate along the mission/airport profile to fixed observer points. Evaluate procedure branches such as cutback, continuous descent, approach speed, and track.

At Q2, call an AEDT/ANP-style adapter with airport, runway, traffic, tracks, day/night, and population. Store contours and exposure as a separate assessment object. Never report a Q0 proxy as a DNL contour.

### Stage E5: Parametric aircraft LCA

Build a component/material/process inventory from mass, geometry, propulsion, energy, and structures:

- primary structure and skins;
- fuselage/cabin/interiors;
- engines, motors, generators, inverters, cables, thermal systems;
- battery packs, hydrogen tanks, insulation, pumps;
- landing gear, systems, and replacement items.

Multiply by versioned LCI factors for production, operation, MRO, and EOL. Keep production, use, MRO, and EOL columns visible. The first useful output is a range and a contribution breakdown, not a single rounded CO₂e number. Promote finalists to open or licensed LCI databases only after the goal/scope and allocation choices are frozen.

### Stage E6: Energy-carrier and infrastructure feasibility

Evaluate storage mass/volume, power, heat, reserve, centre of gravity, turnaround, refuelling/charging, and airport availability. These are physical or operational residuals. A concept that requires an unavailable tank volume or impossible peak power is infeasible independently of its environmental score.

### Stage E7: Finalist fidelity and uncertainty

Use higher-fidelity tools only for finalists:

- map-based or calibrated propulsion/emissions decks;
- external airport noise and dispersion models;
- trajectory/weather ensemble contrail analysis;
- detailed LCA/LCI and material process data;
- thermal, battery aging, hydrogen boil-off and infrastructure studies.

The local NASA design-under-uncertainty paper is a suitable pattern: polynomial-chaos or Monte Carlo surrogates can expose confidence intervals and allow robust objectives/constraints without running the expensive model on every optimizer call.

## Requirements, objectives, and diagnostics

### Policy

The policy must be declared per metric and load case. A recommended default is:

| Environmental quantity | Default ALAS role | Promote to hard requirement when |
| --- | --- | --- |
| mission energy/fuel closure | hard physical closure | always; a non-converged mission is not a valid result |
| usable battery/H₂ energy, storage volume, peak power, thermal limit | hard physical/architecture residual | whenever the carrier is selected |
| regulatory noise/emissions certification value | hard requirement | only with named certification metric, operating points, and compliance evidence |
| airport DNL/Lden/contour/population | scenario-specific hard or soft constraint | only with named airport, fleet, tracks, procedures, time period, and population |
| block fuel or physical CO₂ | objective or diagnostic | objective if the study’s primary trade is efficiency; do not also independently optimize a full WTW CO₂e total |
| LTO NOₓ/local-air-quality mass | objective or airport requirement | when the airport/airshed and metric are in scope |
| non-CO₂ climate RF/ATR/contrail risk | diagnostic or robust objective | only after metric, horizon, weather ensemble, and model validity are frozen |
| manufacturing/MRO/EOL LCA | objective or diagnostic | hard only when data quality and system boundary support a defensible threshold |
| SAF share or hydrogen/electric adoption | requirement only if it is an explicit scenario/policy | when certification, supply, and infrastructure assumptions are part of the brief |

Hard environmental constraints should use the existing signed residual convention: positive residual means violation. Soft targets should remain feasible while contributing a normalized preference penalty. Evaluation failures must remain distinct from physical misses.

### Double-counting control

Maintain a metric-ownership ledger. The following partition is recommended:

| Physical driver | Primitive owner | Derived metrics | Counting rule |
| --- | --- | --- | --- |
| fuel mass flow | mission/propulsion | fuel energy, tank-to-wake CO₂, WTW use-stage input | fuel burn is the primitive; CO₂ is derived once |
| upstream fuel pathway | fuel/LCA adapter | WTW/LCA upstream CO₂e, ILUC, credits | never alter the physical flight-fuel result |
| engine species | propulsion/emissions evaluator | LTO NOₓ, cruise NOₓ, nvPM, SOₓ, H₂O | local-air-quality and climate responses are separate |
| trajectory/weather/soot | climate evaluator | contrail opportunity, RF, ATR, uncertainty | no generic fixed multiplier; no add-on to a full climate metric |
| acoustic source/profile | aero/propulsion/noise evaluator | EPNL, SEL, DNL/Lden, contour/population | event, annual exposure, and certification metrics are not additive |
| component materials/processes | mass/structures/LCA evaluator | production, MRO, EOL, circularity | do not reuse a material factor in fuel burn and production LCA without a declared link |
| energy-carrier storage and powertrain | architecture/propulsion | mass, volume, power, thermal and infrastructure residuals | mass affects the mission naturally; environmental benefits are not an extra mass credit |

If the objective is full lifecycle climate impact, it should contain operational CO₂ and upstream fuel emissions exactly once, plus the declared production/MRO/EOL terms. In that run, block fuel and tank-to-wake CO₂ remain displayed diagnostics or secondary objectives, not additional unweighted terms. If the objective is block fuel, report LCA separately. If a multiobjective score combines them, each term needs a named normalization, weight, and rationale.

## Traceability to the requirements-first algorithm

| Requirements-first step | Environmental input/output | Evidence and policy |
| --- | --- | --- |
| Intent and preset | environmental goal, baseline, carrier, airport/route scenario | source and scenario IDs are mandatory |
| DesignBrief/TLAR | units, functional units, load cases, limits, targets, policies | hard/soft/objective/diagnostic is explicit |
| Validate and materialise load case | reserve, payload, load factor, airport, weather, service life | reject incompatible denominators and missing factors |
| Preview and freeze | selected fidelity, factor set, source versions, model validity | frozen evidence manifest |
| Architecture selection | fuel/SAF/H₂/battery/hybrid topology, engine/propulsor, installation | discrete architecture gates before continuous optimization |
| Preliminary sizing | energy storage, tanks, thermal systems, power electronics, mass/volume | physical residuals and uncertainty |
| Geometry/cabin/aero | surface area, high-lift/gear, nacelle/pylon, propeller/fan and shielding inputs | Q0/Q1 noise source inputs |
| Mass/payload/CG/fuel closure | material/component inventory and mission energy | production LCA and use-stage inputs |
| Aero/performance/mission | segment fuel, thrust/power, altitude/time/position, procedures | CO₂ and species inventory; profile provenance |
| Airport/field performance | runway, track, procedure, traffic, population, airshed | Q1/Q2 noise and local-air-quality context |
| Structures | material/process families, manufacturing yield, replacement, EOL | lifecycle data quality and allocation |
| Residuals | environmental hard/soft residuals and evaluator failures | signed, normalized, named residuals |
| Hard-feasible filter | only declared limits can reject the candidate | do not promote an uncertain diagnostic silently |
| Score and rank | block fuel, WTW/LCA, NOₓ, noise, climate, or cost terms | feasibility first, then explicit objective policy |
| Finalist high fidelity | AEDT/ANP, trajectory/weather, calibrated emissions, detailed LCA | higher-fidelity result replaces or cross-checks screening result |
| Report/export | metric vector, baseline delta, uncertainty, source ledger, hashes | no silent assumptions; unresolved items are visible |

## Suggested assessment data contract

The following is a report/API shape, not a request to edit source files in this research task:

~~~text
EnvironmentalAssessment {
    model_version,
    scenario_id,
    baseline_id,
    functional_units,
    fidelity_by_domain,
    metric_results[],
    energy_closure,
    source_ids[],
    factor_registry_version,
    uncertainty_summary,
    diagnostics[],
}

MetricResult {
    name,
    value,
    unit,
    denominator,
    load_case,
    policy,
    target_or_limit,
    signed_residual,
    model,
    fidelity,
    source_id,
    uncertainty,
    status,
}
~~~

Recommended metric names include:

- fuel.trip_kg, fuel.block_kg, energy.flight_MJ;
- climate.co2_tank_to_wake_kg;
- climate.co2_well_to_wake_kg or climate.lifecycle_kg_co2e, only with a declared boundary;
- emissions.lto_nox_g, emissions.cruise_nox_g, emissions.nvpm_mass_g, emissions.nvpm_number;
- climate.contrail_opportunity_fraction, climate.contrail_rf_W_m2, climate.atr_K, each with scenario/horizon;
- noise.event_epnl_dB, noise.event_sel_dB, noise.dnl_dB, noise.lden_dB, noise.contour_population;
- lca.production_kg_co2e, lca.operation_kg_co2e, lca.mro_kg_co2e, lca.eol_kg_co2e, lca.total_kg_co2e;
- energy.usable_battery_kWh, energy.peak_power_kW, energy.h2_mass_kg, energy.tank_volume_m3, energy.thermal_rejection_kW, infrastructure.turnaround_min.

## Uncertainty and reporting recommendations

Separate uncertainty types:

- **aleatory:** weather, traffic, route, daily operational variation, population/traffic realisations;
- **epistemic:** battery future performance, hydrogen tank mass, SAF pathway LCI, material process factors, emissions indices, contrail model, noise installation correction;
- **model-form:** surrogate versus deck, source model versus calibrated acoustics, simple contrail screen versus weather-resolved model;
- **scenario:** technology year, electricity mix, SAF availability, hydrogen production, airport procedure, fleet replacement.

At screening fidelity:

1. report a nominal value and a low/central/high or P05/P50/P95 range;
2. identify which inputs drive the range;
3. label confidence as verified/calibrated, evidence-based parametric, screening, or placeholder;
4. report baseline deltas under identical mission and denominator conventions;
5. use robust constraints only when the brief asks for them, for example probability of compliance at least 0.95 or an upper quantile below a limit;
6. do not turn an uncertainty interval into an unannounced deterministic margin;
7. record weather ensemble, fuel pathway, electricity mix, material database, service life, and end-of-life scenario.

For finalists, use Monte Carlo, polynomial chaos, or a design-of-experiments surrogate. Sensitivity should be reported by domain so the optimizer does not hide uncertainty behind one aggregate score. A design with lower mean climate impact but a wide, unbounded contrail or energy-system uncertainty should not be described as more mature than a slightly worse but well-characterised design.

Every report should include:

- a metric table with value, unit, denominator, load case, target/policy, fidelity, source, and uncertainty;
- a physical CO₂/fuel ledger separate from upstream and lifecycle terms;
- species by mission phase and LTO versus cruise;
- noise event results and, only when the airport scenario exists, contours/exposure/population;
- lifecycle contribution by production, operation, MRO, and EOL;
- energy-carrier mass/volume/power/thermal/infrastructure closure;
- baseline comparison and the exact baseline assumptions;
- sensitivity and missing-data diagnostics;
- a source/rights/hash manifest.

Useful visualisations are a mission energy timeline, phase-resolved emissions bars, altitude/contrail opportunity profile, noise source/procedure comparison, lifecycle waterfall, and uncertainty/sensitivity plot. A single composite “green score” should be optional and secondary to the traceable metric vector.

## ALAS mapping and implementation order

| Need | Current ALAS owner | Environmental use | Status |
| --- | --- | --- | --- |
| requirement values/policies/load cases | alas-config and requirements-first adapter | EnvironmentalBrief, units, scenario validation | contract exists conceptually; environmental fields not yet wired |
| architecture/carrier selection | alas-config, alas-opt | discrete fuel/energy/topology seeds and gates | propulsion architecture pattern exists; environmental gates are proposed |
| atmosphere and trajectory | alas-atmo, alas-mission | temperature, pressure, altitude, time, position, weather scenario | mission path exists; weather-resolved climate is not attached |
| engine/fuel/power | alas-prop | fuel flow, shaft/bus power, EI tables, carrier closure | fuel/propulsion baseline exists; EI and full carrier models are future adapters |
| geometry and installed source | alas-geom, alas-aero, alas-prop | nacelle/pylon/propulsor/airframe noise and installation inputs | acoustic evaluator not attached |
| mission and field performance | alas-mission, alas-perf, alas-pipeline | block fuel, phases, airport profile/procedures | existing mission/field result is the primary E1 input |
| mass, materials, structures | alas-mass, alas-struct | component/material/process inventory, EOL mass | mass/structure flow exists; environmental inventory is proposed |
| residual and ranking | alas-opt | hard/soft environmental residuals and explicit objective terms | feasibility-first machinery is available |
| external airport/climate/LCA | adapter boundary | AEDT/ANP, CoCiP/AirClim, open or licensed LCI | citation/evidence boundary only; no source edits in this task |
| reporting | alas-report, alas-viz, alas-gui | metric vector, uncertainty, baseline, source manifest | report interface exists conceptually; domain panels are future work |

Recommended implementation order is E0/E1 first, then E2 species and E6 energy closure, followed by Q0/Q1 noise and E5 parametric LCA. Add E3 weather/contrail and Q2 airport contours only after the trajectory, airport, and scenario contracts are stable. This order yields useful diagnostics early and prevents an expensive external model from becoming an undocumented optimizer dependency.

## Unresolved and citation-only sources

The following sources are relevant to the requested state-of-the-art coverage but were not copied into the local environmental directory during the resumed pass. They should be rechecked before being used as machine-readable factors or compliance requirements:

| Source | Why it matters | Status and rights note |
| --- | --- | --- |
| ICAO Annex 16 Vol I/II/III/IV and technical manuals Doc 9501, Doc 9829, Doc 9911, Doc 10013 | certification noise, engine emissions, CO₂, CORSIA, noise procedures, airport/ATM assessment | official ICAO pages and store/consultation links; some material is paid or all-rights-reserved; citation-only |
| ICAO CORSIA Default Life Cycle Emissions Values, November 2025, ICAO/CAEP | current default SAF LCEF, ILUC, co-processing, transport, hydrogen/process-heat updates | public official PDF URL identified; institutional rights, no permissive redistribution assumption; not copied locally |
| ICAO CORSIA Methodology for Calculating Actual Life Cycle Emissions Values, November 2025, ICAO/CAEP | actual SAF pathway boundaries, certification, chain of custody, allocation | public official PDF URL identified; institutional rights, not copied locally |
| FAA AEDT 4a Technical and User Manuals, FAA, 2026 | fuel burn, noise, emissions inventory, dispersion, airport exposure and annualisation | official public FAA download; rights/reuse should be checked; citation-only |
| Clean Aviation Technology Evaluator Second Global Assessment 2024 | mission/airport/fleet assessment of CO₂, NOₓ, and noise | public European project report; reuse terms should be checked; citation-only |
| Clean Aviation Strategic Research and Innovation Agenda 2024 | SAF, hydrogen, electric/hybrid technology targets and infrastructure framing | public EU/CAJU report; reuse terms should be checked; citation-only |
| DLR ALICIA high-level LCA and inventory-schema reports | fast LCA integration, discrete-event lifecycle, reusable inventory structure | DLR eLib public records; no local copy in this evidence set; citation-only |
| Albano et al., Life cycle inventories for aviation: Background data, shortcomings, and improvements, 2024 | LCI data quality, MRO/EOL gaps, non-CO₂ and material/recycling boundaries | public DLR/author PDF URL identified; citation-only pending local rights/hash |
| Vivalda and Fioriti, Stream Life Cycle Assessment Model for Aircraft Preliminary Design, Aerospace 11(2), 113, 2024, DOI 10.3390/aerospace11020113 | preliminary-design LCA with production, operation, MRO, disposal, and uncertainty | publisher open-access record; PDF not in this local environmental copy set |
| Lee et al., The contribution of global aviation to anthropogenic climate forcing for 2000 to 2018, Atmospheric Environment 244, 117834, 2021, DOI 10.1016/j.atmosenv.2020.117834 | global aviation CO₂/non-CO₂ forcing and uncertainty | open manuscript/PDF URL identified; citation-only |
| Burkhardt, Bock, and Bier, Mitigating the climate impact from aviation by reducing aircraft soot number emissions, 2018, DOI 10.1038/s41612-018-0046-4 | soot/contrail sensitivity and nonlinearity | open Nature PDF URL identified; citation-only |
| Teoh et al., Global aviation contrail climate effects from 2019 to 2021, ACP 24, 6071–6093, 2024, DOI 10.5194/acp-24-6071-2024 | trajectory/weather-resolved contrail magnitude and variability | CC BY publisher PDF URL identified; citation-only |
| Megill et al., Alternative climate metrics to the Global Warming Potential are more suitable for assessing aviation non-CO₂ effects, 2024, DOI 10.1038/s43247-024-01423-6 | RF/GWP/GTP/ATR/GWP* comparison for aviation non-CO₂ | open DLR/Nature PDF URL identified; citation-only |
| Adler and Martins, Hydrogen-powered aircraft: Fundamental concepts, key technologies, and environmental impacts, 2023, DOI 10.1016/j.paerosci.2023.100922 | hydrogen aircraft storage, efficiency, infrastructure, NOₓ/water/contrail implications | open-access author/publisher copy identified; citation-only |
| Jagtap, Childs, and Stettler, Conceptual design-optimisation of a subsonic hydrogen-powered long-range blended-wing-body aircraft, 2024, DOI 10.1016/j.ijhydene.2024.11.331 | aircraft-level LH₂ sizing and volumetric/gravimetric constraints | open-access publisher record identified; citation-only |
| EOL component/material studies and Timmis, Howe, and Dallara parametric LCA papers | composite, metal, battery, cabin, recycling, and process trade-offs | several open or author copies identified; paywall/licence varies; citation-only until each PDF is locally verified |

The absence of a local PDF or a permissive licence is not evidence that a claim is false. It means that the source is not yet suitable for an ALAS evidence manifest with a local hash.

## Local source ledger

All files below are in [bib/environment-lifecycle](../../bib/environment-lifecycle/). The copies were staged from existing repository PDFs on 2026-08-26 and their SHA-256 values were recomputed after staging. Hashes are lowercase hexadecimal. Page counts are from pdfinfo. The rights statement is a provenance note, not a legal opinion.

| Local PDF | Title, authors, year, identifier or DOI, and source URL | Rights/provenance note | Pages; bytes; SHA-256 |
| --- | --- | --- | --- |
| [easa-eaer-2025.pdf](../../bib/environment-lifecycle/easa-eaer-2025.pdf) | European Aviation Environmental Report 2025, EASA, 2025, DOI 10.2822/1537033, [EASA report page](https://www.easa.europa.eu/en/domains/environment/eaer), [official PDF](https://www.easa.europa.eu/sites/default/files/eaer-downloads/EASA_EAER_2025_Book_v5.pdf) | PDF states EASA copyright, all rights reserved, proprietary document; local research reference copy only | 200; 27993811; 4c3adb8ad9350f409db401cc334f44c4b7b0ecc24d1ea758b534c3ddd67d4800 |
| [faa-saf-redac-2019-03.pdf](../../bib/environment-lifecycle/faa-saf-redac-2019-03.pdf) | Nathan Brown (FAA), Alternative Jet Fuels R&D and ASCENT Analysis, FAA REDAC Environment & Energy Subcommittee, 2019-03, [FAA record](https://www.faa.gov/about/officeorg/headquartersoffices/ang/redac-environment-and-energy-2019-03-alternative-jet-fuels), [official PDF](https://www.faa.gov/sites/faa.gov/files/2022-07/eeSC-Mar2019-SustainableAviationFuels(SAF).pdf) | Official FAA/U.S. government presentation; public source access, attribution retained; no broader redistribution conclusion inferred | 19; 3445878; edfdec3b021c2354ab84115ba9d38ce12886087560895eb84f2800aab62daffa |
| [icao-doc9889-airport-air-quality-manual.pdf](../../bib/environment-lifecycle/icao-doc9889-airport-air-quality-manual.pdf) | Airport Air Quality Manual, ICAO Doc 9889, Second Edition, 2020, [public ICAO PDF](https://www.icao.int/sites/default/files/2025-04/9889_cons_en.pdf) | PDF states © ICAO 2020, all rights reserved, reproduction requires written permission; background reference only | 210; 2250885; 8e5e3d1701ded7bf936938d12d090f06f102b8ba3a45eb6d9cdf49c07360502c |
| [nasa-aeroacoustics-pai-thomas-2003.pdf](../../bib/environment-lifecycle/nasa-aeroacoustics-pai-thomas-2003.pdf) | Russell H. Thomas, Aeroacoustics of Propulsion Airframe Integration: Overview of NASA’s Research, NASA Langley, 2003, NOISE-CON Paper 105, [NTRS record](https://ntrs.nasa.gov/citations/20030065859), [official PDF](https://ntrs.nasa.gov/api/citations/20030065859/downloads/20030065859.pdf) | NASA NTRS distribution marked public/public-use-permitted; retain NASA and author attribution | 8; 953888; ef0300c7e4fa7b78f76e9a7171293809777543fbe68eeff2af566cb47db3e299 |
| [dlr-poll-schumann-full-flight-profile-2025.pdf](../../bib/environment-lifecycle/dlr-poll-schumann-full-flight-profile-2025.pdf) | D.I.A. Poll and Ulrich Schumann, An estimation method for the fuel burn and other performance characteristics of civil transport aircraft; part 3 full flight profile when the trajectory is specified, The Aeronautical Journal, 2025, DOI 10.1017/aer.2024.141, [article/PDF](https://doi.org/10.1017/aer.2024.141) | PDF identifies Cambridge Open Access under CC BY 4.0; preserve attribution and licence | 37; 2184532; ee868646aebd9279ef2582df436f6b439031dc289bb61aac93d99bc314fc2f09 |
| [nasa-mbsae-final-report-2025.pdf](../../bib/environment-lifecycle/nasa-mbsae-final-report-2025.pdf) | Jason Corman, Jimmy Tai, Evan Harrison, Jai Ahuja, Christian Perron, Bogdan-Paul Dorca, Josef D’cruz, and Samuel Moore, NASA Model-Based Systems Analysis and Engineering Final Report, NASA/CR-20250007059, 2025, [NTRS record](https://ntrs.nasa.gov/citations/20250007059), [official PDF](https://ntrs.nasa.gov/api/citations/20250007059/downloads/CR-20250007059.pdf) | NASA NTRS public distribution/public-use-permitted determination; local research copy | 60; 2570620; 57d7015e88a9044c5fb43a80c7833e93cc5060d883e1a9742a063a1a61b8d7bc |
| [nasa-hybrid-drive-kpi-2018.pdf](../../bib/environment-lifecycle/nasa-hybrid-drive-kpi-2018.pdf) | Kirsten P. Duffy and Ralph H. Jansen, Turboelectric and Hybrid Electric Aircraft Drive Key Performance Parameters, NASA Glenn, 2018, GRC-E-DAA-TN57353, [NTRS record](https://ntrs.nasa.gov/citations/20180005327), [official PDF](https://ntrs.nasa.gov/api/citations/20180005327/downloads/20180005327.pdf) | NASA NTRS public distribution/public-use-permitted; retain attribution | 19; 1097746; 22c69f5e13f36f49c9f49b163dcd3108c9d1d0a3dea120984f77cf0a571d9a53 |
| [nasa-hybrid-regional-sizing-2016.pdf](../../bib/environment-lifecycle/nasa-hybrid-regional-sizing-2016.pdf) | Kevin R. Antcliff, Mark D. Guynn, Ty V. Marien, Douglas P. Wells, Steven J. Schneider, and Michael T. Tong, Mission Analysis and Aircraft Sizing of a Hybrid-Electric Regional Aircraft, NASA/AIAA-2016-1028, 2016, [NTRS record](https://ntrs.nasa.gov/citations/20160007763), [official PDF](https://ntrs.nasa.gov/api/citations/20160007763/downloads/20160007763.pdf) | NASA NTRS public distribution/GOV_PUBLIC_USE_PERMITTED; local research copy | 16; 595106; 4a66d53a673201320c7cc6e55d106cc68245504c34697ec054d37f1f89061943 |
| [nasa-leaps-energy-mission-2018.pdf](../../bib/environment-lifecycle/nasa-leaps-energy-mission-2018.pdf) | Francisco M. Capristan and Jason R. Welstead, An Energy-Based Low-Order Approach for Mission Analysis of Air Vehicles in LEAPS, 2018, NTRS 20190000427, [NTRS record](https://ntrs.nasa.gov/citations/20190000427), [official PDF](https://ntrs.nasa.gov/api/citations/20190000427/downloads/20190000427.pdf) | NASA NTRS public distribution/GOV_PUBLIC_USE_PERMITTED; local research copy | 12; 878284; 0ea7235d03cc12ba38bd77a39a5f62191326a12e9b001e5b075efb5382ab1f3d |
| [nasa-design-under-uncertainty-2024.pdf](../../bib/environment-lifecycle/nasa-design-under-uncertainty-2024.pdf) | Ben D. Phillips, Joanna N. Schmidt, Eliot D. Aretskin-Hariton, and Robert D. Falck, Design Under Uncertainty for Conceptual Aircraft Design: Leveraging Analytical Gradients, 2024, NTRS 20240014863, [NTRS record](https://ntrs.nasa.gov/citations/20240014863), [official PDF](https://ntrs.nasa.gov/api/citations/20240014863/downloads/Phillips_SciTech_rev2.pdf?attachment=true) | NASA NTRS public distribution/GOV_PUBLIC_USE_PERMITTED; record year used because formal publication date is not exposed | 17; 6944507; 43cc62f314a886810af2a023aa313b3bdf1aa1889817c70ae315808bfb48dbb5 |
| [dlr-fouda-hybrid-electric-architecture-2024.pdf](../../bib/environment-lifecycle/dlr-fouda-hybrid-electric-architecture-2024.pdf) | Mahmoud Essam Abdelmoneam Fouda, Automated Hybrid-Electric Propulsion Architecture Modeling for Conceptual Aircraft Design: A Novel Approach to Integrating System Architecting in MDO, Master’s thesis, 2024, [DLR eLib record](https://elib.dlr.de/203543/), [handle](https://hdl.handle.net/11511/108190), [PDF](https://elib.dlr.de/203543/1/AUTOMATED%20HYBRID-ELECTRIC%20PROPULSION%20ARCHITECTURE%20MODELING%20-%20A%20NOVEL%20APPROACH%20TO%20INTEGRATING%20SYSTEM%20ARCHITECTING%20IN%20MDO.pdf) | Repository metadata states Open Access and CC BY-NC-ND; do not assume unrestricted derivatives | 107; 3326275; 30e8fe14c66d45dfe058e4d8bb5cbb33fda1b2b3666e77483f6f2813995abf0a |
| [mdpi-kellermann-thermal-management-2020.pdf](../../bib/environment-lifecycle/mdpi-kellermann-thermal-management-2020.pdf) | Hagen Kellermann, Michael Lüdemann, Markus Pohl, and Mirko Hornung, Design and Optimization of Ram Air-Based Thermal Management Systems for Hybrid-Electric Aircraft, Aerospace 8(1), 3, 2020, DOI 10.3390/aerospace8010003, [article/PDF](https://www.mdpi.com/2226-4310/8/1/3) | Open access, CC BY 4.0; retain attribution and licence | 22; 781438; 03b7cecf37c7fb452c59e755432fe5f8cddf0b6905413664c1553fad60486f82 |
| [mdpi-coutinho-thermal-management-2023.pdf](../../bib/environment-lifecycle/mdpi-coutinho-thermal-management-2023.pdf) | Maria Coutinho, Frederico Afonso, Alain Souza, David Bento, Ricardo Gandolfi, Felipe R. Barbosa, Fernando Lau, and Afzal Suleman, A Study on Thermal Management Systems for Hybrid-Electric Aircraft, Aerospace 10(9), 745, 2023, DOI 10.3390/aerospace10090745, [article/PDF](https://www.mdpi.com/2226-4310/10/9/745) | Open access, CC BY 4.0; retain attribution and licence | 24; 1320739; 39fd97e94c852d582a9317ca04ba8fa3e10aa4c767f1f5b236b27907c108463e |
| [mdpi-habermann-hybrid-turboshaft-2023.pdf](../../bib/environment-lifecycle/mdpi-habermann-hybrid-turboshaft-2023.pdf) | Anaïs Luisa Habermann, Moritz Georg Kolb, Philipp Maas, Hagen Kellermann, Carsten Rischmüller, Fabian Peter, and Arne Seitz, Study of a Regional Turboprop Aircraft with Electrically Assisted Turboshaft, Aerospace 10(6), 529, 2023, DOI 10.3390/aerospace10060529, [article/PDF](https://www.mdpi.com/2226-4310/10/6/529) | Open access, CC BY 4.0; retain attribution and licence | 30; 5870205; 6f051e3a8400ad134de2261a1df9bac6a63f9ba114533267969ccb9e93134f5a |
| [mdpi-staats-hybrid-electric-d328-2025.pdf](../../bib/environment-lifecycle/mdpi-staats-hybrid-electric-d328-2025.pdf) | Annika Nora Staats, Florian Troeltsch, and Andreas Bardenhagen, Conceptual Design of a Hybrid-Electric Aircraft Based on a Dornier 328 Demonstrator, Aerospace 12(12), 1085, 2025, DOI 10.3390/aerospace12121085, [article/PDF](https://www.mdpi.com/2226-4310/12/12/1085) | Open access, CC BY 4.0; retain attribution and licence | 13; 963967; 6442eaf6bc55060e7e560a50fa8ad363dc1d66494b443aedf3e03da9010c64c3 |
| [nasa-cea2022-2024.pdf](../../bib/environment-lifecycle/nasa-cea2022-2024.pdf) | Mark K. Leader, Thomas M. Lavelle, Xiao-yen J. Wang, Kevin W. Dickens, Michael McTague, and Jeffrey P. Hill, CEA2022: A Modernization of NASA Glenn’s Software CEA, NASA/TFAWS 2024, 2024, [NTRS record](https://ntrs.nasa.gov/citations/20240009728), [official PDF](https://ntrs.nasa.gov/api/citations/20240009728/downloads/TFAWS_2024_CEA.pdf) | NASA NTRS public distribution/GOV_PUBLIC_USE_PERMITTED; property/thermochemistry support only, not emissions certification | 11; 253873; 257593566280a0c7af10a2f9341aebb6557672be4f29b5e90ef0b1f691721df7 |

## References used in the synthesis

The local source ledger is the primary evidence set. The following linked sources provide the broader standards/project/academic context and are intentionally cited with their boundaries:

- [ICAO Environmental Report 2025](https://www.icao.int/environmental-protection/envrep2025) and [ICAO environmental protection](https://www.icao.int/environmental-protection).
- [ICAO CORSIA eligible fuels](https://www.icao.int/CORSIA/corsia-eligible-fuels), [lifecycle methodology](https://www.icao.int/CORSIA/fuels-lifecycle), and [Annex 16 catalogue](https://store.icao.int/en/annexes/annex-16).
- [ICAO aircraft noise and Balanced Approach](https://www.icao.int/environmental-protection/aircraft-noise).
- [FAA Aviation Climate Action Plan](https://www.faa.gov/general/2021-united-states-aviation-climate-action-plan) and [AEDT 4a information](https://aedt.faa.gov/4a_information.aspx).
- [DLR ALICIA impact assessment](https://www.dlr.de/en/research-and-transfer/projects-and-missions/alicia-impact-assessment), [DLR EXACT](https://www.dlr.de/en/research-and-transfer/projects-and-missions/exact-dlr-studies-of-sustainable-aviation), and [DLR climate-compatible aviation](https://www.dlr.de/en/research-and-transfer/featured-topics/climate-compatible-aviation).
- [Clean Aviation Technology Evaluator 2024](https://www.clean-aviation.eu/research-and-innovation/clean-sky-2/technology-evaluator-2024) and [Strategic Research and Innovation Agenda](https://www.clean-aviation.eu/research-and-innovation/clean-aviation/our-strategic-research-innovation-agenda).
- [NASA Sustainable Flight MBSA&E resources](https://www.nasa.gov/reference/resources-mbsae-sfnp-aiaascitech26/) and [AACES 2050](https://www.nasa.gov/reference/advanced-aircraft-concepts-for-environmental-sustainability-2050/).
- [NASA Environmentally Responsible Aviation concept assessment](https://ntrs.nasa.gov/citations/20160007652) and [N3-X noise and emissions assessment](https://ntrs.nasa.gov/citations/20150006703).
- [Lee et al. 2021](https://doi.org/10.1016/j.atmosenv.2020.117834), [Burkhardt et al. 2018](https://doi.org/10.1038/s41612-018-0046-4), [Teoh et al. 2024](https://doi.org/10.5194/acp-24-6071-2024), and [Megill et al. 2024](https://doi.org/10.1038/s43247-024-01423-6).
- [Adler and Martins 2023](https://doi.org/10.1016/j.paerosci.2023.100922), [Jagtap et al. 2024](https://doi.org/10.1016/j.ijhydene.2024.11.331), and [Vivalda and Fioriti 2024](https://doi.org/10.3390/aerospace11020113).

This document is a conceptual-design research and implementation recommendation. It is not a regulatory compliance determination, an approved noise/emissions calculation, a CORSIA claim, or a substitute for licensed LCI databases, manufacturer engine data, test data, or authority-approved means of compliance.
