# Cabin, interiors, cargo, and human-factors research for requirements-first ALAS

Research note for the ALAS requirements-first transport-aircraft workflow.

- Prepared: 2026-08-26
- Scope: passenger accommodation, seat pitch/width, aisles and exits, monuments, overhead stowage, accessibility, double decks, cargo holds and LD3-45 positions, freight/combi layouts, mass conventions, and centre-of-gravity effects.
- Repository inputs read: docs/REQUIREMENTS_FIRST_AIRCRAFT_DESIGN.md, the ALAS configuration/payload/pipeline owners named by AGENTS.md, and the downloaded files listed below.
- Download directory: bib/cabin-interiors-cargo/
- Evidence convention: every conclusion is labelled as geometric capacity, operational load case, certification constraint, or preliminary-design proxy. A proxy is not a finding of compliance.

## Bottom line

Accommodation must be a first-class, traceable design object. A passenger count is not a cabin definition, a ULD count is not a cargo-operational load case, and an exit-capacity formula is not evacuation certification. The requirements-first model should therefore carry four separate layers:

1. **Geometric capacity**: seats, monuments, aisles, exits, stowage, deck clearances, cargo stations, loading doors, and ULD fit.
2. **Operational load case**: passenger and baggage mass basis, class mix, carry-on/checked-bag assumptions, cargo net and ULD tare, loading strategy, boarding/accessibility service, and CG state.
3. **Certification constraint**: CS/FAR-25 seats, passageways, exits, evacuation, stowage retention, fire/smoke, crashworthiness, oxygen-mask reach, and applicable accessibility/operating rules.
4. **Preliminary-design proxy**: compact rules used to screen concepts before approved data and formal substantiation exist.

The current ALAS code already exposes useful pieces: DesignBrief.accommodation, per-class pitch/width, geometric seat placement, monument and exit placement, PayloadLayout, LD3/LD3-45 ULD definitions, loading strategy, and MAC-based CG. It also intentionally contains proxies. The main residual is integration: crates/alas-pipeline/src/acceptance.rs still reports that the canonical accommodation contract is not translated into the active payload layout as a complete acceptance result. That must remain an explicit inconclusive result until the adapter and higher-fidelity checks exist.

## Supplied DLR paper

**S1: Fuchs, Mara; Ghanjaoui, Yassine; Biedermann, Jörn; Nagel, Björn (DLR, 2023), “An Approach for Linking Heterogenous and Domain-Specific Models to Investigate Cabin System Variants.”** Presented at the 33rd Annual INCOSE International Symposium, Honolulu, 15–20 July 2023. Official record: [DLR eLib record/PDF](https://elib.dlr.de/196218/1/IS2023_paper_112.pdf). Local copy: [S1 PDF](../../bib/cabin-interiors-cargo/dlr-fuchs-ghan%20jaoui-biedermann-nagel-2023-cabin-system-variants.pdf).

The paper is directly relevant to a requirements-first accommodation model, but it is not a complete aircraft-sizing or evacuation-certification method.

- **Requirements and architecture**: SysML carries system architecture, functional requirements, and traceability. Initial overall-aircraft-design parameters, including the passenger count, arrive through XML and instantiate a cabin/LOPA object model.
- **Domain-specific fidelity**: Matlab performs geometry and placement; Blender supplies high-fidelity 3D geometry; Unity supplies 1:1 virtual-reality design review. The paper’s point is that no single model can represent requirements, installation space, FEA-level detail, ergonomics, and subjective design quality at the same fidelity.
- **Stable object identity**: objects carry a unique ID through SysML, Matlab, Blender/Unity, and the return path. Derived values such as positions and verification properties are returned to the requirements model.
- **Overhead-bin example**: three overhead-stowage variants are compared on an A321 Qatar Airways LOPA with four business rows, 25 economy rows, and 166 passengers. The passenger-service-function (PSF) count per overhead bin changes with seat pitch: the paper describes one PSF per bin in business class and up to three in economy. This is a coupling between seat layout, overhead geometry, oxygen masks/ventilation/PSU, and preassembly, not merely a bin-volume problem.
- **Checks**: the example measures seated-passenger reach to oxygen masks using a 5th-percentile Asian-woman grip-range datum, checks whether PSF parts fit within the hatrack/preassembly envelope, and sends a 1:1 design/clash review back to the requirements model.
- **Reported study result**: the extra-large hatrack variant improved the study’s modularity and mean oxygen-mask distance relative to the regular and large variants. The numerical results are variant-specific; they are not universal cabin rules.
- **Fidelity and cost**: SysML and Matlab checks are quick, while Blender and Unity are much slower. This supports using cheap geometry/constraint screening early and deferring high-fidelity human-factors, clash, and certification evidence to finalist configurations.

**ALAS interpretation.** The DLR ID/data-thread pattern should be adopted for every accommodation object: requirement ID, load-case ID, deck/hold station ID, geometric placement ID, mass-property ID, and verification-evidence ID. The current DeckItem/SeatMeta/ExitMeta/ContainerMeta/summary structures are a good start, but the evidence and fidelity fields should be made explicit. DLR’s paper does not justify using the current ALAS exit-capacity or service-ratio proxies as certification results.

## Evidence synthesis by accommodation topic

### Seating, seat pitch, seat width, and class mix

**Geometric capacity.** A seat is a longitudinal and lateral footprint, not just a passenger integer. Capacity depends on usable deck length, seat pitch, seat width, seats abreast, aisle count/width, fuselage-section variation, monument bays, exits, and deck assignment. The current ALAS configuration supports both a class share and explicit counts/abreast values. The payload engine places rows against the actual geometry, can stretch pitch to use available floor, and reports seated versus unseated passengers. This is the right place to derive a geometric seat count.

**Operational load case.** A design passenger count must identify whether mass means passenger only, passenger plus baggage, or an operator-provided convention. ALAS already has PassengerMassBasis, class mass per passenger, checked-bag mass, carry-on/belly-cargo settings, design/max passenger cases, and design/max cargo cases. These are distinct from the number of seats physically placed. A maximum seating layout and a design-day load case can therefore have different occupancy and baggage/cargo.

**Certification constraint.** EASA CS-25.785 covers a seat or berth for each occupant over two years, emergency-landing protection, approval/restraint, and seat structural design. The CS-25 text uses a 77 kg occupant design value in the relevant seat-load context; that value is not a substitute for the operator’s loading convention. CS-25.817 limits a single aisle to three seats on each side. Passageway/aisle widths are set by passenger capacity and location under CS-25.813/815. A seat pitch or width alone cannot establish compliance.

**Preliminary-design proxy.** ALAS SeatClassConfig guards pitch at 0.7112 m (28 in) and width at 0.4064 m (16 in) in its conceptual schema. These are useful screening floors, but they are not asserted here as universal regulatory minima, and they do not cover seat structure, armrests, egress, comfort, accessibility, crashworthiness, or operator-specific LOPA approval. They should be tagged PreliminaryProxy in evidence and never promoted to CertificationConstraint.

**Accessibility and human factors.** Seating accommodation includes movable aisle armrests, adjoining seats for an attendant/assistant in applicable cases, bulkhead or greater-legroom seats for a fused/immobilised leg, service-animal seating, clear identification of accommodated seats, and transfer/aisle-chair geometry. DOT 14 CFR Part 382 also connects the wheelchair, aisle width, maneuvering space, and seat height. Therefore an optimizer that maximizes seat count without reserving accessible seats is evaluating the wrong design space.

### Aisles, exits, monuments, and evacuation

**Geometric capacity.** The ALAS payload model places aisles implicitly through seats-abreast, reserves monument bays, distributes exits across deck bays, and separately handles an upper deck. It reports aisle width, aisle count, maximum abreast, exit type/pairs/capacity, and deck utilization. These values are excellent candidate-screening outputs.

**Certification constraint.** The consolidated CS-25 rules and associated AMC include, among other constraints:

- CS-25.803: for the relevant large-aeroplane passenger capacities, the maximum certificated configuration must be evacuated to the ground within 90 seconds under the prescribed demonstration conditions, unless an accepted analysis/test path is used.
- CS-25.813: passageway and cross-aisle clearances depend on exit type and configuration; Type A/B access is materially wider than Type I/II/C access.
- CS-25.815: minimum aisle widths depend on passenger capacity and whether the aisle is below or above the stated floor-height condition.
- CS-25.817: a single aisle may serve no more than three seats on either side.
- CS-25.819: lower-deck service compartments have additional requirements.

FAA AC 25.803-1A is guidance, not regulation, but it identifies the design changes that can invalidate an earlier evacuation result: exit type/number/location, increased passenger capacity, passenger redistribution that overloads an exit pair, partitions and galleys that restrict/merge flow, altered passageways or cross-aisles, changed flight-attendant locations, and more than one occupied deck requiring adequate communication and inter-deck transit. It also states that analysis in lieu of a demonstration must be supported by appropriate tests and source data.

**Preliminary-design proxy.** ALAS currently selects an exit type from fuselage diameter, estimates pairs from deck length and a spacing constant, derates Type-A nominal capacity with exit_capacity_realism_factor, and caps pairs per deck. This is a transparent proxy for screening. It is not an approved exit selection, a flow model, or a 90-second evacuation substantiation. The report must expose a residual whenever the requested passenger distribution, exit allocation, aisle/passage width, or deck transition is not demonstrated.

**Monuments.** The current fitting model uses a 0.95 m longitudinal monument bay, approximately 0.85 m galley width and 0.90 m lavatory width, and derives lavatories/galleys from passenger counts when the user gives zero. The current ratios (about one lavatory per 45 passengers and one galley per 100 passengers plus one) are useful conceptual provisioning heuristics. They do not establish code-required numbers, service quality, crew work area, accessible-lavatory capability, fire compliance, water/waste capacity, or evacuation acceptability. Monument placement must be part of the same layout that is checked for flow and accessibility.

**Fire and smoke.** CS-25.853/Appendix F addresses interior material flammability, seat cushions, panels, partitions, galley structures, large cabinets/stowage, and related components. CS-25.855/857 classifies and protects cargo/baggage compartments. FAA AC 25.853-1 provides a seat-cushion fire-test means of compliance. These should be represented as certification evidence requirements, not collapsed into a zero-mass monument placeholder.

### Overhead bins, carry-on baggage, and boarding assumptions

**Geometric capacity.** Bin capacity must include usable volume/linear length, contour, door opening, clearance to the passenger’s head/seat, lid/door swing, attachment locations, PSF/oxygen/ventilation integration, and the accommodation of mobility aids where required. DLR’s hatrack study demonstrates that seat pitch changes PSF density and that modular preassembly is a separate 3D fit check. A volume-only bin estimate misses these dependencies.

**Operational load case.** FAA AC 120-27F distinguishes standard, survey-derived, and actual passenger/baggage weights; carry-on items are handled separately from the passenger weight table and must fit in the seat or approved stowage. For an operational weight-and-balance method, baggage/freight may be represented at a compartment centroid only when the approved procedure supports that simplification; floor, ULD, running-load, and restraint limits still apply. A requirements-first load case should therefore state:

- passenger mass basis and season/operator datum;
- checked-bag, carry-on, gate-checked, and mobility-aid assumptions;
- bin/closet/under-seat capacity and priority rules;
- boarding and preboarding/accessibility assumptions;
- zone or row distribution used for CG;
- whether the case is a design case, maximum case, or an approved operational method.

**Safety constraint and research evidence.** CS-25.787 requires stowage compartments to withstand placarded contents and critical load distributions, prevent shifting, and, for the applicable passenger configurations, enclose cabin stowage other than defined convenience compartments. NASA’s Fokker F28 drop test used a 32 in (81.28 cm) seat pitch, 5th- to 95th-percentile ATDs, and overhead-bin mass simulators of about 11.34 kg per linear foot. This is valuable crash-test evidence about the mass and structural interaction of overhead bins; it is not an overhead-bin design standard or a universal allowed loading value. NASA’s full-scale F28 crash-test report similarly records overhead-bin, seat, floor, lower-cavity/cargo, mass, balance, and instrumentation interactions.

### Accessibility

DOT 14 CFR Part 382 (the local GovInfo 2022 text is a historical regulation extract; current applicability must be checked against the current DOT rule) provides concrete accommodation implications:

- aircraft with 100 or more passenger seats require priority cabin space for at least one folding manual wheelchair, outside overhead and routine under-seat stowage;
- twin-aisle aircraft with lavatories must provide at least one accessible lavatory that permits entry, maneuvering, use, exit, privacy, accessible controls, and grab bars using the on-board wheelchair;
- aircraft with more than 60 passenger seats and an accessible lavatory require an operable on-board wheelchair, and the chair must be compatible with aisle width, maneuvering space, and seat height;
- seating accommodation includes movable aisle armrests and other request-driven seat assignments.

The EASA reduced-mobility research report adds design considerations for multi-deck aircraft: assisted reduced-mobility passengers, wheelchairs and mobility aids, hoists/training, on-board wheelchair availability on all decks/classes, and high-contrast lighting/signage/pictograms. These are operational and human-factors requirements in addition to the physical aisle width.

In ALAS, accessibility should be an explicit accommodation requirement and load-case/operational scenario, not a post-hoc note. A layout that has a nominal aisle but cannot move the on-board wheelchair to the lavatory, reserve the priority wheelchair space, or provide a usable transfer seat must emit a residual.

### Double-deck cabins

**Geometric capacity.** ALAS detects a double deck from a fuselage height/diameter relationship and creates separate main/upper deck cabin specifications while retaining a lower hold. This is a useful early architecture discriminator. Each occupied deck needs its own seating, monuments, exits/egress path, service provisions, deck clear height, and mass/CG contribution.

**Certification and operations.** A double deck introduces inter-deck stairs/ramps or lifts, communication and crew-station requirements, fire/smoke zoning, pressure/structural integration, boarding/deplaning flow, accessibility across decks, and evacuation from each occupied deck. FAA AC 25.803-1A explicitly calls for tests of communication and, if necessary, transit between decks when more than one deck is occupied. A simple height threshold does not address these issues.

NASA’s C-Wing concept study is useful negative evidence: it says its single-deck arrangement accommodates more than 600 passengers and avoids many of the difficult loading and emergency-egress issues associated with double-deck cabins. That is a concept-study observation, not proof that a single deck is always optimal. The attempted NASA NTRS double-deck PDF download is retained but invalid locally (HTML returned with HTTP access failure); no claim from it is used.

### Cargo holds, LD3-45, freight, and combi layouts

**Geometric capacity.** The ULD object must carry at least code, contour/fit envelope, length, width, height, volume, maximum gross mass, tare mass, deck/hold, station, restraint interface, loading door/path, and fire-compartment classification. A station count is not equivalent to net cargo capacity:

- a ULD position can exist geometrically but be inaccessible through the door/loading path;
- the ULD gross limit, floor/running-load limit, restraint limit, or compartment limit can bind before volume;
- tare belongs to aircraft gross payload and CG, while the user’s cargo requirement is normally net cargo;
- an LD3-45 lower-hold position is a geometric/accommodation target; it is not automatically loaded in every mission case.

ALAS’s current local ULD table records, as conceptual data, LD3 at about 1.56 × 1.53 × 1.63 m, 4.5 m³, 82 kg tare, 1,588 kg maximum gross, and LD3-45 at about 1.56 × 1.53 × 1.14 m, 3.6 m³, 82 kg tare, 1,134 kg maximum gross. The lower-hold fallback order includes LD3-45 and BLK, and the fit check does not clamp a container through the structure. These are good engineering inputs for the current solver, but the source of truth for a certifiable aircraft/ULD interface must be the approved aircraft contour, restraint, and current ULD standard/operator data.

**Operational load case.** Define separately:

- required minimum LD3-45 positions;
- available compliant positions by hold/deck;
- chosen loaded ULDs and their tare/net/gross;
- hold loading density and floor/rail limits;
- door/loading equipment and station access;
- cargo class/fire/temperature/dangerous-goods restrictions;
- target and allowed CG range;
- passenger-belly-baggage priority versus revenue freight;
- main-deck freighter, lower-deck passenger, and combi partition cases.

The current cargo engine supports main-deck/lower-deck ULD choices, loading strategies (target CG, minimum pallets, door proximity, uniform), a target CG percent MAC, ULD tare in item mass, and a summary that distinguishes net cargo, tare, gross payload, capacity, slots, and achieved CG. It should preserve those distinctions in the canonical requirements-first result.

**Freighter/combi.** Passenger/freighter/combi is a discrete architecture choice, not a continuous payload variable. A combi must model the passenger-cabin boundary, main-deck freight door and loading route, partition/fire barrier, restraint and floor loads, simultaneous or sequential loading, passenger evacuation impact, and CG changes when the passenger/cargo split moves. An “all lower hold” passenger case and an actual main-deck freighter case must not share a silent capacity formula.

IATA’s ULD Regulations and SAE AS36100 are the appropriate industry/airworthiness references for ULD characteristics and performance, but their official pages indicate paid standards/manual access. They are therefore cited below and not copied into the repository.

### Mass conventions and centre of gravity

Accommodation mass must be decomposed by item and by load case:

| Component | Geometric model | Operational mass model | CG consequence |
|---|---|---|---|
| Occupant | seat/seat row and deck | passenger mass basis, class/zone occupancy | longitudinal and lateral passenger distribution |
| Seat/interior | seat row/monument object | dry interior mass, supplier/operator datum | OEW/CG; not the same as occupant mass |
| Carry-on | bin/closet/under-seat envelope | policy, count, mass distribution | cabin CG and stowage loads |
| Checked bags | hold/bag or ULD envelope | per-pax or actual/survey mass | hold CG and floor/ULD limits |
| Revenue cargo | ULD/slot/door/hold | net cargo + ULD tare + gross limit | CG, MZFW/MTOW, local station limits |
| Galley/lav/monument | footprint and service clearance | dry mass, fluids, crew/service equipment | OEW/CG and egress/fire/access |

The current ALAS PassengerMassBasis and class mass_per_pax_kg are valuable because they prevent passenger count from silently becoming a universal mass. The current default of 90 kg with passenger-plus-baggage basis is a project default, not a certification or international standard. FAA AC 120-27F should be used to specify whether a case uses standard, survey, or actual weights and how baggage/carry-on and CG curtailment are handled.

The payload layout computes item mass properties from positive-mass items and reports deck/hold CG. Monuments and exits currently carry zero mass in the layout while being accounted for as furnishings; that is acceptable only as a documented preliminary proxy. A future mass model should allow dry mass, fluids, equipment, and an evidence source for every monument/exit/ULD/seat assembly.

For cargo, ALAS back-calculates a required payload CG from OEW, OEW CG, target aircraft CG, and target payload mass, then re-solves after adding actual ULD tare. In abstract form:

~~~
x_payload_target =
  ((m_OEW + m_payload) * x_aircraft_target
    - m_OEW * x_OEW) / m_payload
~~~

That is the right structure for a preliminary load planner, but it must be bounded by approved aircraft CG envelopes, fuel states, ZFW/MZFW/MTOW limits, floor/rail/running-load limits, and worst-case or operator-approved zone loading. A single target percentage of MAC is not a certification envelope.

## Exact mapping to the ALAS requirements-first algorithm

The mapping below follows docs/REQUIREMENTS_FIRST_AIRCRAFT_DESIGN.md and current owner files. It is intentionally explicit about what is already implemented and what remains a residual.

| Requirements-first stage | Cabin/interiors/cargo materialization | Existing ALAS owner/evidence | Required result and residual |
|---|---|---|---|
| TLAR entry | passenger design/max, mass basis, design/max cargo, minimum LD3-45, architecture intent, deck/class/seat/aisle/monument/ULD/accessibility requirements | crates/alas-config/src/design_brief.rs; DesignBrief.accommodation; requirements-first doc sections on accommodation | Store policy, unit, source, status, fidelity, and load-case ID. Existing brief lacks several typed LOPA/accessibility/ULD-door fields. |
| Architecture and load-case definition | Passenger, Freighter, Combi; single/double deck; occupied decks; class mix; design/max passenger case; cargo/ULD case; boarding/baggage/accessibility case | crates/alas-payload/src/layout.rs modes and cabin/cargo configuration; design brief adapter | Select discrete architecture first. Emit a residual if passenger/cargo mode is inferred or if a required deck/hold case is not materialized. |
| Initial geometry and bounds | fuselage section, cabin deck length/height, wall inset, seats, aisles, monument bays, exits, overhead/stowage envelope, cargo door and ULD stations | crates/alas-payload/src/geometry.rs; cabin.rs; SeatClassConfig; cargo ULD table | Build an actual bounded geometry. Geometry-only capacity must be reported separately from loadable/certifiable capacity. |
| Preliminary sizing and feasibility | row placement, seats-abreast, pitch, class allocation, service reserve, monument count/footprint, exit distribution, hold slots, LD3-45 positions | crates/alas-payload/src/cabin/seating.rs, fittings.rs, engine.rs, cargo/engine.rs | Hard feasibility residuals: unseated required pax, class/deck minimum unmet, aisle/height/fit clash, missing ULD positions, inaccessible loading path, missing service reserve. |
| Physical analysis | payload mass, bags, ULD tare, OEW/CG, payload CG, CG envelope, MZFW/MTOW/structural payload, mission effects | crates/alas-pipeline/src/baseline.rs builds PayloadLayout; crates/alas-payload/src/cargo/engine.rs computes target/achieved CG | Run each named load case. Do not accept a geometric layout when mass/CG or structural limits are unknown. |
| Certification gate | formal seat/exit/aisle/evacuation, fire/smoke, stowage retention, oxygen reach, crashworthiness, accessibility and operational approvals | EASA/FAA/DOT sources in this note; current pipeline acceptance is explicit about inconclusive accommodation translation | Require authority/supplier evidence or mark Inconclusive/NotYetModeled; ALAS proxies cannot close this gate. |
| Optimization | discrete cabin architecture/deck/ULD choices plus continuous geometry and class variables; objective/penalty for residuals and mass/drag/mission | requirements-first optimizer/screening stages; payload summaries | Keep architecture and ULD type discrete. Rank by hard residual feasibility first, then soft seat/cargo/comfort/mission objectives. |
| Candidate report | requested/placed/capacity/unfilled, deck/hold source, ULD net/tare/gross, service/egress/accessibility flags, mass/CG, source IDs | PassengerSummary, CargoSummary, DeckItem, candidate-evaluation contract | Every reported number needs a source and fidelity. Preserve capacity, loaded, requested, and certified/operationally accepted as separate fields. |

The active pipeline’s accommodation check should not convert PassengerSummary.max_certifiable_capacity into a certification approval. It should expose that value as PreliminaryProxy and retain the formal CS/FAR requirement as Inconclusive until the required evidence is present.

## Proposed typed data model

The following Rust-like model is intentionally richer than the current brief. It separates requirement policy from evidence fidelity and separates capacity from load cases.

~~~rust
enum AccommodationArchitecture {
    Passenger,
    Freighter,
    Combi { passenger_deck: DeckId, freight_deck: DeckId },
}

enum EvidenceFidelity {
    GeometricCapacity,
    OperationalLoadCase,
    CertificationConstraint,
    PreliminaryProxy,
    HumanFactorsStudy,
    NotYetModeled,
}

enum RequirementStatus {
    Requested,
    Derived,
    Verified,
    Inconclusive,
    Failed,
    CitationOnly,
}

struct EvidenceRef {
    id: String,
    source: String,
    locator: String,       // rule, section, page, figure, or code path
    rights_note: String,
    fidelity: EvidenceFidelity,
    status: RequirementStatus,
}

struct Requirement<T> {
    id: String,
    value: T,
    policy: RequirementPolicy,
    units: String,
    load_case_id: Option<String>,
    evidence: Vec<EvidenceRef>,
}

struct CabinCargoBrief {
    architecture: Requirement<AccommodationArchitecture>,
    decks: Vec<DeckRequirement>,
    passenger: Option<PassengerAccommodation>,
    monuments: Vec<MonumentRequirement>,
    egress: EgressRequirement,
    stowage: StowageRequirement,
    holds: Vec<HoldRequirement>,
    uld_positions: Vec<UldPositionRequirement>,
    load_cases: Vec<AccommodationLoadCase>,
}

struct PassengerAccommodation {
    design_pax: Requirement<i64>,
    maximum_pax: Requirement<i64>,
    deck_minimums: Vec<(DeckId, Requirement<i64>)>,
    classes: Vec<SeatClassRequirement>,
    mass_basis: Requirement<PassengerMassBasis>,
    occupant_mass_kg: Requirement<f64>,
    checked_bag_kg_per_pax: Requirement<f64>,
    carry_on_kg_per_pax: Requirement<f64>,
    boarding_and_accessibility: Vec<OperationalAssumption>,
}

struct SeatClassRequirement {
    name: String,
    pitch_m: Requirement<f64>,
    width_m: Requirement<f64>,
    abreast: Option<Requirement<i64>>,
    share_or_count: ClassAllocation,
    seat_mass_kg: Requirement<f64>,
}

struct EgressRequirement {
    aisle_width_m: Requirement<f64>,
    cross_aisle_width_m: Option<Requirement<f64>>,
    exit_types_and_positions: Vec<ExitRequirement>,
    passenger_allocation_by_exit_pair: Vec<Requirement<i64>>,
    evacuation_fidelity: EvidenceFidelity,
}

struct StowageRequirement {
    overhead_volume_m3: Option<Requirement<f64>>,
    overhead_linear_m: Option<Requirement<f64>>,
    carry_on_policy: Requirement<String>,
    wheelchair_priority_spaces: Vec<Requirement<BoundingBox>>,
    psf_and_oxygen_reach: Vec<Requirement<HumanReachCheck>>,
}

struct HoldRequirement {
    id: String,
    deck: DeckId,
    clear_envelope: BoundingBox,
    loading_door: DoorId,
    fire_class: Option<String>,
    floor_and_rail_limits: Option<LoadLimit>,
}

struct UldPositionRequirement {
    id: String,
    hold_id: String,
    station_x_m: Requirement<f64>,
    uld_type: Requirement<String>,
    minimum_count: Option<Requirement<i64>>,
    net_capacity_kg: Requirement<f64>,
    tare_kg: Requirement<f64>,
    gross_limit_kg: Requirement<f64>,
}

struct AccommodationLoadCase {
    id: String,
    pax_by_deck_and_class: Vec<LoadItem>,
    checked_bags: LoadDistribution,
    carry_on: LoadDistribution,
    revenue_cargo_net_kg: f64,
    loaded_uld_tare_kg: f64,
    fuel_state: FuelState,
    cg_policy: CgPolicy,
    operational_status: RequirementStatus,
}
~~~

Requirement<T> is the important part: a 120-passenger hard maximum, a 120-seat geometry result, a 120-passenger standard-weight load case, and a 120-passenger certification capacity are different typed facts even when their numeric values coincide.

## Failure and residual taxonomy

Every residual should carry at least {code, severity, stage, requirement_id, load_case_id, observed, limit, units, evidence, remediation}.

| Residual family | Examples | Typical owner/stage |
|---|---|---|
| AccommodationGeometry | no valid fuselage section; insufficient deck clear height; seat/monument overlap; aisle or exit envelope clash; overhead/PSF interference; ULD does not fit width/height/contour; door-to-station path missing | geometry/materialization |
| AccommodationCapacity | unseated design/max passenger; class mix or main/upper-deck minimum unmet; monument/service reserve consumes all seats; exit-pair or aisle proxy below requested capacity | preliminary feasibility |
| EgressCertification | CS/FAR aisle/passage requirement not met; exit type/number/location not substantiated; passenger distribution exceeds exit rating; 90-second evacuation evidence absent; inter-deck evacuation/communications not tested | certification gate |
| MonumentAndFire | galley/lavatory count is only a ratio; accessible lavatory not placed; crew/service area missing; material/fire/smoke evidence absent; cargo compartment class/liner unknown | service/certification |
| StowageAndBoarding | overhead capacity unknown; placarded load/retention unknown; carry-on policy not supplied; wheelchair priority space displaced; preboarding/boarding path or turnaround assumption missing | operational/human factors |
| CargoGeometry | required LD3-45 positions absent; BLK fallback used; main-deck door or loading route absent; ULD contour/restraint interface unknown; hold fire/temperature classification unknown | cargo materialization |
| CargoMass | net cargo requested but only gross/tare capacity known; ULD tare omitted/double-counted; floor/rail/running-load limit exceeded; density assumption absent | cargo/load case |
| MassAndBalance | payload/OEW/MTOW/MZFW limit exceeded; CG outside envelope; target CG achieved only with unapproved rule; fuel burn or passenger-zone movement not checked; lateral balance absent | physical analysis |
| Accessibility | on-board wheelchair cannot turn or reach lavatory; aisle-chair/seat-height incompatibility; no movable armrest/adjoining-seat policy; multi-deck assistance unavailable; signage/lighting not checked | operations/human factors |
| ModelFidelity | preliminary proxy presented as certification; citation-only standard used as if loaded; local source revision is stale; canonical brief not translated to active solver; proprietary aircraft data substituted by assumption | all stages/reporting |

Recommended severity:

- **Hard**: violates a hard requirement or makes the load case physically/certifiably impossible.
- **Soft**: misses a preference/target but remains feasible.
- **Diagnostic**: evidence or model fidelity is insufficient to decide.
- **Citation-only**: the claim is traceable to an official page but the underlying standard/report was not legally copied into the repository.

## Source register, rights, and hashes

Access date for all URLs: 2026-08-26 (Europe/Madrid). SHA-256 values are for the exact local files, not for a later revision.

### Downloaded and retained

| ID | Metadata and source | Local file | Pages / validity | Rights note | SHA-256 |
|---|---|---|---|---|---|
| S1 | Fuchs et al., DLR, 2023, cabin-system variants; [official PDF](https://elib.dlr.de/196218/1/IS2023_paper_112.pdf) | [S1](../../bib/cabin-interiors-cargo/dlr-fuchs-ghan%20jaoui-biedermann-nagel-2023-cabin-system-variants.pdf) | 17 / valid PDF | Official DLR eLib research copy. The PDF says copyright 2023 by the authors and grants INCOSE permission to publish/use; no broad open licence is stated. Retained for internal research and citation; do not republish the PDF or figures. | 9e98f3cf46e12e5bec83f1334064ba100c755d4760e69f0e0bc08527c7437e48 |
| S2 | EASA, Easy Access Rules for Large Aeroplanes (CS-25), Amendment 27; [official download](https://www.easa.europa.eu/en/downloads/136694/en) | [S2](../../bib/cabin-interiors-cargo/easa-cs25-amendment-27-easy-access-rules.pdf) | 1,495 / valid PDF | Official EASA public download, no explicit open licence stated. Kept as a research copy. It is not the current Amendment 28 rule set; check the current EASA certification page before any compliance decision. | 76b28a91ee2a24ef5eea3a72f26fc6d95c9758d9c9342a8070e27b12fd9e28c6 |
| S3 | FAA AC 25.803-1A, Emergency Evacuation Demonstrations, 2012; [official PDF](https://www.faa.gov/documentLibrary/media/Advisory_Circular/AC_25.803-1A.pdf) | [S3](../../bib/cabin-interiors-cargo/faa-ac-25-803-1a-emergency-evacuation.pdf) | 25 / valid PDF | Official FAA public advisory circular. It is guidance, not regulation; no separate open licence is stated. | 5550c3b008fba3b65ec7fd5b320fc801196f5ed3b9b266601a8fa0c88f481fce |
| S4 | FAA AC 120-27F, Aircraft Weight and Balance Control, 2019; [official PDF](https://www.faa.gov/documentLibrary/media/Advisory_Circular/AC_120-27F.pdf) | [S4](../../bib/cabin-interiors-cargo/faa-ac-120-27f-aircraft-weight-and-balance-control.pdf) | 58 / valid PDF | Official FAA public advisory circular. Guidance only; no separate open licence is stated. | 726adba4c7050d366e430f17d117e25baf80308b564ffaaaacd4e7f337131f78 |
| S5 | FAA AC 25.853-1, Flammability Requirements for Aircraft Seat Cushions, 1986; [official PDF](https://www.faa.gov/documentlibrary/media/advisory_circular/ac_25.853-1.pdf) | [S5](../../bib/cabin-interiors-cargo/faa-ac-25-853-1-seat-cushion-flammability.pdf) | 11 / valid PDF | Official FAA public advisory circular. Guidance only; no separate open licence is stated. | 61d96153bf26b0c3f0975bb5eb2d04f8b03d21a8cb58011d5d43bf7df22d198b |
| S6 | U.S. DOT, 14 CFR Part 382, Title 14 Vol. 4, 2022 GovInfo compilation; [official PDF](https://www.govinfo.gov/content/pkg/CFR-2022-title14-vol4/pdf/CFR-2022-title14-vol4-part382.pdf) | [S6](../../bib/cabin-interiors-cargo/dot-cfr-2022-part-382.pdf) | 46 / valid PDF | U.S. government regulation compilation, publicly accessible from GovInfo. This is a historical 2022 edition; current DOT rules and transition dates must be checked separately. | 664c06da8b056c062a6243b4ff77c20a492bf2a902c9614f66834fa46c4aa6ee |
| S7 | EASA research report EASA.2008.C.25, reduced-mobility/accessibility final report, Issue 1.1; [official PDF](https://www.easa.europa.eu/sites/default/files/dfu/safety-and-research-research-projects-docs-flight-standards-EASA-2008.C.25-Final-report-Issue-1.1.pdf) | [S7](../../bib/cabin-interiors-cargo/easa-2008-c-25-reduced-mobility-final-report.pdf) | 503 / valid PDF | Official EASA public research report; no explicit open licence stated. Research/operational guidance, not a certification specification. | 5ece35f9c82aceb4900a5c7820221ec0dac99c4c7b04227b233fdee98450e40d |
| S8 | Littell/NASA-FAA, Full-Scale Drop Test of a Fokker F28 Wingbox Fuselage Section, NTRS 20180006305; [official PDF](https://ntrs.nasa.gov/api/citations/20180006305/downloads/20180006305.pdf) | [S8](../../bib/cabin-interiors-cargo/nasa-20180006305-f28-drop-test.pdf) | 16 / valid PDF | NASA/NTRS public technical report; U.S. government report status normally permits public research use, but retain attribution and do not imply certification approval. | 968fa9af684736023444ccb1f2150ecda5b0f494befc3a8ac8f0de3cc3b5f673 |
| S9 | NASA/FAA Fokker F28 full-scale crash-test report, NTRS 20200002946; [official PDF](https://ntrs.nasa.gov/api/citations/20200002946/downloads/20200002946.pdf) | [S9](../../bib/cabin-interiors-cargo/nasa-20200002946-f28-full-scale-crash-test.pdf) | 77 / valid PDF | NASA/NTRS public technical report; retain attribution. Test evidence is not a design approval or universal load allowance. | 1ba1f4f747cb3e47d4926f6b753e2c45079adfde59d70f263b91b8c42e2e5dd4 |
| S10 | NASA/CR-2013-217820, advanced supersonic concept study, NTRS 20130010174; [official PDF](https://ntrs.nasa.gov/api/citations/20130010174/downloads/20130010174.pdf) | [S10](../../bib/cabin-interiors-cargo/nasa-cr-2013-217820-supersonic-advanced-concept-studies.pdf) | 308 / valid PDF | NASA/CR public technical report; retain attribution. Cabin counts and pitch values are not certification or operator norms. | 77c0a8ee516684340e38a0337c4f162d8f8a97d3473f62de3c130342a33e73a9 |
| S11 | NASA, “C-Wing: Application to Large Aircraft,” NTRS 19960023622; [official PDF](https://ntrs.nasa.gov/api/citations/19960023622/downloads/19960023622.pdf) | [S11](../../bib/cabin-interiors-cargo/nasa-19960023622-c-wing-large-aircraft.pdf) | 40 / valid PDF | NASA public technical report; retain attribution. Concept-level single-deck/egress observations only. | df49e6c000bbdb722d19c3125fb44abb804200e2b83fe354ef235b78b91c4d30 |

### Retained artifact excluded from evidence

| ID | Source and local artifact | Status and reason | SHA-256 |
|---|---|---|---|
| X1 | NASA NTRS double-deck concept URL: [record/PDF URL](https://ntrs.nasa.gov/archive/nasa/casi.ntrs.nasa.gov/20140011907.pdf); local file [X1](../../bib/cabin-interiors-cargo/nasa-20140011907-double-deck-aircraft-concept.pdf) | Excluded. The server returned an HTML error page (magic bytes 60 33 68 74, not a PDF) while the NTRS endpoint was inaccessible. The file is intentionally retained because the instruction was not to delete anything; no claim in this note relies on it. | 452a9c982d33380400bf4782481ce8fd0092269ddca4eeaddcf9201a20d8d052 |

### Citation-only or current-revision sources not copied

| ID | Source | Why it is citation-only / unresolved |
|---|---|---|
| C1 | EASA current [CS-25 Amendment 28 page](https://www.easa.europa.eu/en/document-library/certification-specifications/cs-25-amendment-28) and [current Easy Access Rules page](https://www.easa.europa.eu/en/document-library/easy-access-rules/easy-access-rules-large-aeroplanes-cs-25) | The local open PDF is Amendment 27. Current certification work must check Amendment 28 and later corrections directly. |
| C2 | ICAO [Annex 6 public extract](https://www.icao.int/sites/default/files/safety/CAPSCA/PublishingImages/Pages/ICAO-SARPs-%28Annexes-and-PANS%29/Annex%206.pdf) | Public web extract was not copied because edition/provenance/licence boundaries are unclear. Used only for the high-level carry-on-stowage principle; CS/FAR and operator data remain the ALAS implementation sources. |
| C3 | ICAO [Annex 18 store page](https://store.icao.int/en/annex-18-the-safe-transport-of-dangerous-goods-by-air) and [Technical Instructions page](https://www.icao.int/Dangerous-Goods/Technical-Instructions) | Official store states that the publication is sold and access-controlled. No PDF copied. Dangerous-goods restrictions therefore remain a citation/evidence requirement, not a hardcoded ALAS cargo rule. |
| C4 | IATA [ULD Regulations](https://www.iata.org/en/publications/manuals/uld-regulations/) | Official page describes the ULDR as a paid manual containing ULD regulatory, technical, and operating specifications. No legal open PDF was available for repository inclusion. |
| C5 | SAE [AS36100](https://saemobilus.sae.org/standards/as36100-air-cargo-unit-load-devices-performance-requirements-test-parameters) | Official page identifies the air-cargo-ULD performance standard and revisions, but the technical standard is sold/paywalled. No PDF copied. |
| C6 | Airbus [aircraft characteristics](https://www.aircraft.airbus.com/en/customer-care/fleet-wide-care/airport-operations-and-aircraft-characteristics/aircraft-characteristics) and A380 characteristics PDF | The downloadable A380 document is marked proprietary/confidential and all-rights-reserved. It was not downloaded or used as a repository source. Public product pages may support high-level double-deck context only. |
| C7 | NASA NTRS [B737 overhead-bin crash simulation record](https://ntrs.nasa.gov/citations/20040086068) | Record is publicly discoverable but provides no downloadable report in the accessible endpoint. Cited as an unresolved lead, not used as evidence. |
| C8 | DOT [current disability rule index](https://www.transportation.gov/airconsumer/disability) and 2024 wheelchair final-rule page/PDF | The 2024 final-rule PDF endpoint returned HTTP 403 during this retrieval. The local S6 historical Part 382 extract is retained for concrete geometry, but current legal applicability must be rechecked from DOT/eCFR. |
| C9 | DOT [accessible lavatories final-rule page](https://www.transportation.gov/airconsumer/final-rule-accessible-lavatories-single-aisle-aircraft-PDF) | Current rule material is cited but not copied because the direct government PDF endpoint was access-denied in this run. |

## Recommended implementation sequence

1. Extend the canonical brief with typed architecture, deck, seat-class, monument, egress, stowage/accessibility, hold, ULD-position, and named accommodation-load-case requirements. Keep the existing passenger/cargo/LD3-45 fields as a compatibility projection.
2. Add EvidenceFidelity and RequirementStatus to the brief/result contract. Do not overload RequirementPolicy to mean certification status.
3. Materialize every candidate with stable IDs, as in S1: requirement → deck/hold station → DeckItem → mass item → residual/evidence.
4. Compute geometry capacity first: placed/seated/unseated passengers, class/deck allocation, monument reserve, aisle/exit proxy, overhead/stowage envelope, and compliant ULD positions.
5. Instantiate named operational cases: design passenger case, maximum passenger case, passenger-plus-bags case, cargo-only case, combi split case, accessibility/boarding case, and CG/fuel cases.
6. Propagate dry interior mass, occupant/bag/cargo net, ULD tare, and fluids to OEW/payload/CG. Check local floor/rail/ULD limits and aircraft envelopes before mission analysis.
7. Treat exit/evacuation, fire/smoke, accessibility, oxygen reach, seat/crashworthiness, and ULD approval as certification/human-factors gates. A proxy can rank candidates but cannot close the gate.
8. In reports, show requested, placed, geometric capacity, loaded, unfilled, net cargo, ULD tare, gross payload, deck/hold source, achieved CG, evidence IDs, and residuals separately.

No existing Rust source file was modified for this research note.
