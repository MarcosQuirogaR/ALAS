// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::{ClassSplit, DeclaredArchitecture, Evidence};
use crate::{CabinEquipmentMethod, CargoHoldLoading, FlopsInputEvidence, OperatingHaulClass};

/// FLOPS architecture values for the registered aircraft.
///
/// The registry deliberately keeps these values separate from the generic
/// configuration.  Values that are not a complete certification or design
/// mission are marked `UserDeclared` in the family provenance below; the
/// presence of a source title is not permission to call a scenario fact.
pub(super) fn declared_architecture(name: &str) -> Option<DeclaredArchitecture> {
    let preset = crate::presets::get(name).ok()?;
    let seats = usize::try_from(preset.requirements.num_passengers).ok()?;
    let (maximum_mach, design_range_nmi, flight_crew_count, hydraulic_pressure_pa,
        fuselage_mounted_engine_count, published_tank_count, maximum_fuel_capacity_kg,
        mission, architecture) = match name {
        "AVE" => (
            0.90,
            7_600.0,
            2,
            HYDRAULIC_3000_PSI_PA,
            0,
            Some(3),
            Some(200_000.0),
            Evidence {
                document: "AVE-v1 design requirements",
                revision: "2026-09-11",
                location: "conceptual mission and speed assumptions",
                applicability: "notional reference aircraft; no measured aircraft truth",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "design-range and MMO proxies; no physical aircraft validation",
            },
            Evidence {
                document: "AVE-v1 conceptual installation definition",
                revision: "2026-09-11",
                location: "engine, tank and systems assumptions",
                applicability: "notional reference aircraft; all values are declared scenario inputs",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "notional installation and fuel capacity; no manufacturer source",
            },
        ),
        "A340-300" => (
            0.86,
            7_200.0,
            2,
            HYDRAULIC_3000_PSI_PA,
            0,
            Some(6),
            None,
            Evidence {
                document: "Airbus A340-200/-300 Aircraft Characteristics",
                revision: "Rev 33, 2025-12-01",
                location: "sections 2-1-1 and 3-2-1",
                applicability: "A340-312 / WV029 geometry and performance family; design range is a declared study value",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "MMO and design-range mapping; no complete source mission load case",
            },
            Evidence {
                document: "EASA.A.015 A340 type-certificate data and Airbus aircraft characteristics",
                revision: "Issue 28 / Rev 33",
                location: "sections 2-1-1 and 2-9-0; three-tank plus trim arrangement",
                applicability: "A340-312 / CFM56-5C3/F, fixed wing, two wing and no fuselage engines",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "containerized cargo and tank-count mapping are conceptual FLOPS inputs",
            },
        ),
        "A380-800" => (
            0.89,
            8_000.0,
            2,
            HYDRAULIC_5000_PSI_PA,
            0,
            Some(7),
            None,
            Evidence {
                document: "Airbus A380 Aircraft Characteristics",
                revision: "Rev 20, 2025-12-01",
                location: "sections 2-1-1 and 3-2-1",
                applicability: "A380-841 / WV000 geometry and Trent 970-84 family; design range is a declared study value",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "MMO and design-range mapping; no complete source mission load case",
            },
            Evidence {
                document: "EASA.A.110 A380 type-certificate data and Airbus aircraft characteristics",
                revision: "Issue 17 / Rev 20",
                location: "sections 2-1-1 and 2-9-0; four-feed plus trim fuel system",
                applicability: "A380-841 / Trent 970-84, fixed wing, all four engines wing mounted",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "containerized cargo and equivalent tank count remain study mappings",
            },
        ),
        "B787-9" => (
            0.90,
            7_635.0,
            2,
            HYDRAULIC_5000_PSI_PA,
            0,
            Some(3),
            None,
            Evidence {
                document: "Boeing 787 Airplane Characteristics for Airport Planning",
                revision: "D6-58333 Rev Q, October 2025",
                location: "sections 2 and 3.2.2",
                applicability: "787-9 / legacy 561,500 lb MTOW planning variant; design range is a declared study value",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "MMO and design-range mapping; no complete source mission load case",
            },
            Evidence {
                document: "Boeing 787 ACAP and NASA/TP-20210023843",
                revision: "Rev Q / December 2022",
                location: "aircraft systems and installation descriptions",
                applicability: "787-9 / GEnx-1B74/75 P2, fixed wing, two wing-mounted engines",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "containerized cargo and FLOPS equivalent tank topology are study mappings",
            },
        ),
        "A320-200" => (
            0.82,
            3_400.0,
            2,
            HYDRAULIC_3000_PSI_PA,
            0,
            Some(3),
            None,
            Evidence {
                document: "Airbus A320 Aircraft Characteristics",
                revision: "Rev 46, 2026-07-01",
                location: "sections 2-1-1, 3-2-1 and Figure 3-2-1-991-017-A01",
                applicability: "A320-214 / WV017 / CFM56-5B4/3; advertised range is used as a study proxy",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "design range does not define payload, profile or reserves for WV017",
            },
            Evidence {
                document: "EASA.A.064 A318/A319/A320/A321 TCDS and Airbus A320 ACAP",
                revision: "Issue 62 / Rev 46",
                location: "sections 1 III.19, 2-1-1 and 2-2-0",
                applicability: "A320-214 / WV017, fixed wing, two wing-mounted engines",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "containerized cargo is explicitly zero by scenario declaration",
            },
        ),
        "A220-300" => (
            0.82,
            3_400.0,
            2,
            HYDRAULIC_3000_PSI_PA,
            0,
            Some(3),
            None,
            Evidence {
                document: "Airbus A220 Digital Pamphlet and A220 Aircraft Recovery Publication",
                revision: "FAI V5.2, July 2022 / May 2026",
                location: "range and aircraft-characteristics sections",
                applicability: "BD-500-1A11 / legacy 149,000 lb MTOW planning variant; range is a different-weight-variant proxy",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "advertised range lacks payload, profile and reserve definition for this preset",
            },
            Evidence {
                document: "EASA.IM.A.570 BD-500 TCDS and Airbus A220 WBM",
                revision: "Issue 24, 2026-02-20 / Table 3-1",
                location: "section 2 III.19 and fuel/system descriptions",
                applicability: "BD-500-1A11 / PW1521G-3, fixed wing, two wing-mounted engines",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "containerized cargo is zero for the bulk-cargo installation; FLOPS class split is all-economy",
            },
        ),
        "ATR72-600" => (
            0.55,
            740.0,
            2,
            HYDRAULIC_3000_PSI_PA,
            0,
            Some(2),
            None,
            Evidence {
                document: "ATR 72-600 Facts and Figures",
                revision: "product specification, accessed 2026-08-30",
                location: "range and performance summary",
                applicability: "ATR 72-212A / 23,000 kg MTOW; retained for explicit unsupported-domain reporting",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "FLOPS has no propeller/shaft-power mass equation; range is not used to fabricate a mass",
            },
            Evidence {
                document: "EASA.A.084 ATR 42/72 TCDS and ATR Airport Planning Manual",
                revision: "Issue 8, 2021",
                location: "aircraft systems, fuel and installation descriptions",
                applicability: "ATR 72-212A / PW127M / 568F-1, fixed wing",
                kind: FlopsInputEvidence::SourceBacked,
                uncertainty: "hydraulic and cargo mappings are preliminary; propulsion mass is unsupported by FLOPS",
            },
        ),
        "DC-10" => (
            0.88,
            5_700.0,
            3,
            HYDRAULIC_3000_PSI_PA,
            1,
            Some(4),
            None,
            Evidence {
                document: "Boeing DC/MD-10 Airplane Characteristics for Airport Planning",
                revision: "DAC-67803A Rev A",
                location: "Figure 2.1 and design-range study mapping",
                applicability: "DC-10-30 passenger / ACAP 572,000 lb option; design range is a declared study value",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "historical variant mission and crew mapping; no complete source mission load case",
            },
            Evidence {
                document: "FAA TCDS A22WE and Boeing DC/MD-10 ACAP",
                revision: "Rev 13, 2018-04-30 / DAC-67803A Rev A",
                location: "engine installation, fuel-system and aircraft-dimensions sections",
                applicability: "DC-10-30 / CF6-50C, two wing-mounted engines and one tail-mounted engine",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "containerized cargo is zero by study declaration; tank count follows the ACAP layout",
            },
        ),
        _ => return None,
    };

    // The registered A320 case has the same 150-seat total as Airbus' cited
    // typical layout, which also publishes four attendant positions.  Keep
    // the generic FLOPS-derived count for cases whose published cabin total
    // does not match the registered study cabin.
    let flight_attendant_count = match name {
        "A320-200" => 4,
        _ => seats.div_ceil(50),
    };
    let (class_split, cabin) = source_class_split(name, seats);
    Some(DeclaredArchitecture {
        maximum_mach,
        design_range_nmi,
        flight_crew_count,
        flight_attendant_count,
        galley_crew_count: 0,
        class_split,
        hydraulic_pressure_pa,
        fuselage_mounted_engine_count,
        published_tank_count,
        maximum_fuel_capacity_kg,
        containerized_cargo_kg: 0.0,
        cargo_loading: declared_cargo_loading(name),
        containerized_baggage_fraction: None,
        cabin_equipment_method: declared_cabin_equipment_method(preset),
        haul_class: declared_haul_class(name),
        mission,
        cabin,
        architecture,
    })
}

/// How each registered aircraft's cargo compartments are loaded.
///
/// FLOPS equations 125-126 charge a 175 lb unit load device for every 950 lb
/// of containerised mass, and NASA Aviary feeds that equation the checked
/// baggage as well as the revenue cargo. The tare is physical hardware, so it
/// belongs only to an aircraft whose holds take a container. The previous
/// blanket application put 238 kg of ULD into the ATR 72-600's operating empty
/// mass, 397 kg into the A220-300's and 476 kg into the A320-200's, none of
/// which has a containerised hold in its delivered configuration.
///
/// Per aircraft, from the manufacturer's airport-planning description of the
/// cargo compartments (full evidence, with the quotations and the retrieval
/// failures, in `evidence-cargo-loading.md` of the 2026-09-16 dispatch):
///
/// * **ATR 72-600 - bulk.** The passenger aircraft has no lower hold at all:
///   the forward and aft baggage compartments are on the main deck and are
///   loose-loaded against nets (ATR 72-600 factsheet p.22). Only the
///   72-600**F** freighter takes seven LD-3s, and it needs a cargo loading
///   system and a large door the passenger aircraft does not have (ATR
///   72-600F brochure, CM Marketing June 2018, pp.1-2).
/// * **A220-300 - bulk.** An operator weight-and-balance manual states it
///   verbatim: "Cargo compartments are used only for bulk load, Unit Load
///   Devices (ULD) are not used" and "All aircraft are equipped with bulk
///   compartments only. CLC not installed." (CSA/Smartwings A220-300 WBM
///   Rev 1, sections 1.6 and 1.6.1, p.1-13).
/// * **A320-200 - bulk.** Airbus AC A320 Rev 44 section 2-6-0 shows ULD
///   positions in the forward and aft holds, but containerised loading needs
///   a cargo loading system that is a line-fit option or a retrofit (FAA STC
///   ST02733LA / EASA 10074113), and the A320 volumes are published without
///   the "(based on LD3)" qualifier the A340 and A380 carry. The registered
///   aircraft is the A320-214 WV017 bulk arrangement; a provision is not a
///   configuration. This one is an inference from the certification and
///   volume evidence, not a manufacturer statement, and is recorded as such.
/// * **A340-300, A380-800, B787-9, DC-10-30 - containerised.** All four load
///   LD3/AKE-class containers and pallets in the forward and aft lower holds
///   (Airbus AC A340-200/-300 Rev 33 section 2-6-1; Airbus AC A380 Nov 2024
///   section 2-6-0; Boeing D6-58333 Rev Q section 2.6.2; Douglas ACAP
///   DAC-67803A Rev A section 2.1), which is where checked baggage goes.
///   Each also has a separate loose-loaded bulk compartment that cannot take
///   a container - 12.4 % of the hold volume on the A340-300, 8.2 % on the
///   A380-800, 6.6 % on the 787-9 and 11.4 % on the DC-10-30 - so declaring
///   them fully containerised overstates the tare by at most that share,
///   about 160 kg on the A340-300. [`CargoHoldLoading::Mixed`] exists for the
///   split, but no retrieved source gives the *baggage* share between the two
///   holds, and a volume ratio is not that number, so it is not invented here.
/// * **AVE - containerised**, as a notional widebody study declaration.
///
/// A capability the aircraft does not carry in its registered configuration is
/// never used here, and an undeclared aircraft keeps the FLOPS convention
/// rather than silently losing the tare.
///
/// The residual on the four widebodies is stated rather than removed: each has
/// a loose-loaded bulk compartment that cannot take a container, so declaring
/// the whole hold containerised overstates the *reported* tare by at most the
/// bulk share of the hold volume - 12.4 % (A340-300), 11.4 % (DC-10-30), 8.2 %
/// (A380-800), 6.6 % (B787-9), about 160 kg at the largest. No retrieved
/// source gives the baggage split between the container holds and the bulk
/// compartment, and a hold-volume ratio is not that number, so
/// [`CargoHoldLoading::Mixed`] is not declared on a fraction this project
/// would have had to invent.
///
/// The accounting caveat that used to sit here **is now resolved in the
/// evaluator, not here**: Boeing's own OEW definition (D6-58333 Rev Q section
/// 2.1) and the FAA basic operating weight of AC 120-27F exclude unit load
/// devices, and AC 120-85B treats a ULD as tare tracked with the load.
/// `alas_mass::flops_transport::FlopsOperatingItemsBreakdown` therefore
/// computes the tare for every configuration from this declaration and reports
/// it *outside* operating empty mass, with FLOPS' own `WOPIT` convention
/// retained alongside it. The consequence for this function is that the
/// declaration below no longer changes any operating empty mass at all: it
/// decides a separately reported quantity and the cargo-hold architecture, and
/// it cannot be used to move an error.
/// Which method prices each aircraft's cabin equipment and occupant-driven
/// operating items.
///
/// The rule is the **LTH relations' own stated validity domain** - a civil
/// transport whose maximum takeoff mass is *"mindestens 40 Tonnen ... bzw.
/// sich mindestens 70 Passagiersitze an Bord befinden"*, i.e. **at least 40 t
/// or at least 70 passenger seats** - applied uniformly through
/// [`CabinEquipmentMethod::for_civil_transport_size`], and nothing else. The
/// same function selects the method for a configuration built without a
/// preset, so the two modes cannot disagree.
///
/// An earlier revision coded only the mass half of that statement and defended
/// the result with *"the ATR 72-600 is the only preset below the floor, and it
/// is also the one aircraft the switch would make worse"*. The arithmetic was
/// right and the premise was not: at 72 installed seats the ATR is **inside**
/// the source's stated domain. The full rule is now applied, the ATR 72-600
/// takes the LTH relations with every other registered aircraft, and its
/// operating empty mass gets **280 kg worse** as a result - furnishings
/// -1,549 kg, occupant operating items +1,829 kg, +15.09 % to +17.17 % against
/// the ATR factsheet figure. That is the cost of a domain rule that is not
/// trimmed to the answer, and it is also evidence in its own right: the ATR's
/// cabin group is over-priced by *both* published methods, so its excess does
/// not live in the method selection.
///
/// Why the domain rule selects against FLOPS above 40 t, rather than merely
/// permitting the alternative there: on the one aircraft where a manufacturer
/// publishes the accounting boundary, FLOPS is 40 % low on the group and the
/// LTH relations are within 1.3 %.
///
/// | aircraft | Airbus accounting | LTH | FLOPS |
/// |---|---:|---:|---:|
/// | A320-200, 150 seats | 56.3 kg/seat | 55.7 | 54.2 |
/// | A340-300, 290-295 seats | 98.0 kg/seat | 99.1 | 57.1 |
///
/// Effect on the operating-empty-mass error against the published references,
/// measured with `cargo run -p alas-mass --example cabin_equipment_methods`:
/// A340-300 -14.00 % -> -4.71 %, A380-800 -17.83 % -> -8.80 %, B787-9
/// -10.31 % -> -1.11 %, DC-10-30 -11.01 % -> -2.44 %, A220-300 -4.60 % ->
/// -2.77 %, A320-200 +0.04 % -> +1.15 %.
///
/// **This is not validation.** Five of those references carry no stated
/// inclusion list, two are case anchors for a cabin the model does not have,
/// and the A380-800 remains out of domain for both methods because each
/// prices the cabin from a single-tube `length x diameter` proxy. The LTH
/// relations' own quoted accuracy is in-sample, by the restating author's
/// statement. The selection rests on the component-level agreement above and
/// on the domain statement, not on the totals moving in the right direction.
///
/// Evaluated **once, when the preset's declared architecture is built**, from
/// the reference aircraft's certified takeoff mass and the occupancy the mass
/// is actually computed on. It deliberately does not track the takeoff mass a
/// search moves, because a method that switched mid-search would put a step
/// discontinuity into the objective; the aircraft being priced is the
/// registered one.
///
/// **One seat number.** The occupancy here is
/// `requirements.num_passengers` - the same count
/// [`declared_architecture`] builds the class split from and the same count
/// the LTH operating-item relation is evaluated at. An earlier revision read
/// `reference.planning_seats` instead, which is a *reference* datum describing
/// the published cabin, not the cabin this model prices. A domain rule
/// deciding on one seat count while the mass is built on another is a second
/// source of truth whether or not it currently changes the answer. It does
/// not: every preset selects the same method under either reading, which is
/// what the test in `preset_flops/tests.rs` pins.
///
/// **Four of the eight registered aircraft seat fewer passengers in the model
/// than in the published cabin their reference OEW belongs to**, always in the
/// same direction: A340-300 290 against 335, A380-800 525 against 555,
/// A220-300 130 against 140, DC-10-30 250 against 255. Under the LTH method
/// the seat count reaches exactly one mass term, `m_opp`, so matching the
/// published cabins would add 3,454.8 / 2,451.7 / 372.4 / 374.7 kg
/// respectively. That is a **configuration mismatch, not a model error**, and
/// it is not closed here: changing the FLOPS seat input would price a cabin
/// the rest of the product does not fly. It has to be closed in the cabin
/// layout, which this module does not own - and until it is, a real share of
/// the remaining operating-empty deficits is a cabin difference rather than a
/// mass method being wrong.
fn declared_cabin_equipment_method(preset: &crate::AircraftPreset) -> CabinEquipmentMethod {
    let mtom_kg = preset
        .reference
        .mtow_kg
        .unwrap_or(preset.requirements.mtow_kg);
    CabinEquipmentMethod::for_civil_transport_size(
        Some(mtom_kg),
        Some(preset.requirements.num_passengers),
    )
}

/// Which of the two LTH operating-item relations each aircraft would take.
///
/// Read only by [`crate::CabinEquipmentMethod::LthCivilTransportV1`], which
/// publishes a short/medium-haul relation and a long-haul one without a range
/// threshold between them. The class is therefore the aircraft's own mission
/// role: the A340-300, A380-800, B787-9, DC-10-30 and the notional AVE are
/// long-haul types, the A320-200, A220-300 and ATR 72-600 are not. The ATR at
/// 72 installed seats is inside the LTH domain statement's seat clause, so
/// this class is now load-bearing for it and not merely declarative: the
/// ATR 72-600 takes `m_opp = 32.907 n_pax^1.021`.
fn declared_haul_class(name: &str) -> OperatingHaulClass {
    match name {
        "A340-300" | "A380-800" | "B787-9" | "DC-10" | "AVE" => OperatingHaulClass::LongHaul,
        _ => OperatingHaulClass::ShortMediumHaul,
    }
}

pub(crate) fn declared_cargo_loading(name: &str) -> CargoHoldLoading {
    match name {
        "ATR72-600" | "A220-300" | "A320-200" => CargoHoldLoading::Bulk,
        _ => CargoHoldLoading::Containerized,
    }
}

/// Select a published cabin layout only when its passenger total is the same
/// as the registered FLOPS study case.  Typical layouts from a different
/// seat-count configuration are not resized into an installed configuration.
fn source_class_split(name: &str, seats: usize) -> (ClassSplit, Evidence) {
    match name {
        "A320-200" if seats == 150 => (
            ClassSplit {
                first: 12,
                business: 0,
                tourist: 138,
            },
            Evidence {
                document: "Airbus A320 Aircraft Characteristics - Airport and Maintenance Planning",
                revision: "Revision 46, 2026-07-01",
                location: "section 2-4-1, Figure 2-4-1-991-002-A01",
                applicability: "A320-214 / WV017; typical two-class 150-seat layout matches the registered passenger total",
                kind: FlopsInputEvidence::SourceBacked,
                uncertainty: "Airbus uses first/tourist labels; operator cabin equipment and installed seating may differ.",
            },
        ),
        "B787-9" if seats == 290 => (
            ClassSplit {
                first: 0,
                business: 28,
                tourist: 262,
            },
            Evidence {
                document: "Boeing 787 Airplane Characteristics for Airport Planning",
                revision: "D6-58333 Rev Q, October 2025",
                location: "section 2.1.2, typical two-class 290-seat cabin",
                applicability: "787-9; typical two-class 290-seat layout matches the registered passenger total",
                kind: FlopsInputEvidence::SourceBacked,
                uncertainty: "Typical Boeing layout; operator cabin equipment and installed seating may differ.",
            },
        ),
        _ => (
            ClassSplit::all_economy(seats),
            Evidence {
                document: "Registered preset planning cabin",
                revision: "ALAS preset registry 2026-09-11",
                location: "AircraftPreset::planning_cabin_config",
                applicability: "installed passenger total mapped to FLOPS three-class inputs",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "class split remains all-economy because no source layout with the registered passenger total is available",
            },
        ),
    }
}

const HYDRAULIC_3000_PSI_PA: f64 = 3_000.0 * 6_894.757_293_168;
const HYDRAULIC_5000_PSI_PA: f64 = 5_000.0 * 6_894.757_293_168;
