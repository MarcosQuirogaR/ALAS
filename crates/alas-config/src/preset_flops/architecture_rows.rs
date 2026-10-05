// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! FLOPS architecture rows for the newest narrowbody presets, kept apart so
//! `architecture.rs` stays inside its source-size budget.

use super::architecture::HYDRAULIC_3000_PSI_PA;
use super::Evidence;
use crate::{CabinEquipmentMethod, CargoHoldLoading, FlopsInputEvidence, OperatingHaulClass};

/// (maximum Mach, design range nmi, flight crew, hydraulic pressure Pa,
/// fuselage engines, published tank count, maximum fuel kg, mission
/// evidence, architecture evidence), in the order `declared_architecture`
/// destructures them.
pub(super) type Row = (
    f64,
    f64,
    usize,
    f64,
    usize,
    Option<usize>,
    Option<f64>,
    Evidence,
    Evidence,
);

pub(super) fn narrowbody_rows(name: &str) -> Option<Row> {
    Some(match name {
        "E195-E2" => (
            0.82,
            3_000.0,
            2,
            HYDRAULIC_3000_PSI_PA,
            0,
            Some(2),
            None,
            Evidence {
                document: "Embraer E195-E2 specification sheet",
                revision: "April 2025",
                location: "page 1, performance (maximum cruise speed M 0.82, range 3,000 nm full passengers)",
                applicability: "ERJ 190-400 / 62,500 kg MTOW; the advertised range is a study proxy for the design range",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "design range lacks stated cabin, payload, profile and reserve definition; MMO is the sheet's maximum cruise Mach",
            },
            Evidence {
                document: "EASA.IM.A.071 ERJ-190 TCDS and Embraer E-Jets E2 Airport Planning Manual APM 5824",
                revision: "Issue 28 / Rev 8 (E190-E2 effectivity)",
                location: "Section 5 III.3-III.5 and APM section 2.2",
                applicability: "ERJ 190-400 / PW1923G, fixed wing, two wing-mounted engines, wing integral tanks",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "ESTIMATE: the 3,000 psi hydraulic pressure and the two-tank mapping are not stated in the retained documents; containerized cargo is zero for the bulk-loaded holds",
            },
        ),
        "C919" => (
            0.82,
            2_200.0,
            2,
            HYDRAULIC_3000_PSI_PA,
            0,
            Some(2),
            None,
            Evidence {
                document: "Wikipedia, Comac C919, Specifications (secondary, citing COMAC)",
                revision: "retrieved 2026-10-05",
                location: "Chinese edition table: maximum speed M 0.82, full-payload range 2,200 nmi (STD)",
                applicability: "C919-100 STD / 75,100 kg MTOW; the advertised range is a study proxy for the design range",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "secondary source; design range lacks stated cabin, payload, profile and reserve definition; MMO is the table's maximum speed",
            },
            Evidence {
                document: "Wikipedia, Comac C919 (secondary); ICAO EEDB v32 UID 08P28CM150",
                revision: "retrieved 2026-10-05",
                location: "Specifications: two wing-mounted LEAP-1C28, 19,560 kg fuel, no auxiliary tank",
                applicability: "C919-100 STD / LEAP-1C28, fixed wing, two wing-mounted engines, wing integral tanks",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "ESTIMATE: the 3,000 psi hydraulic pressure and the two-tank mapping are not stated in any retained source; containerized cargo is left at the default because the article only says a common ULD with the A320 may be possible",
            },
        ),
        _ => return None,
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
/// * **E195-E2 - bulk.** EASA.IM.A.071 Issue 28 Section 5 III.21 lists two
///   Class C underfloor compartments (14.77 m3 forward, 15.20 m3 aft) and the
///   E190-E2 APM section 2 prints one 22.63 m3 cargo volume with no unit-load
///   device position. Inferred from that absence, not a manufacturer
///   statement.
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
/// The accounting caveat is resolved in the evaluator, not here: Boeing's own OEW definition (D6-58333 Rev Q section
/// 2.1) and the FAA basic operating weight of AC 120-27F exclude unit load
/// devices, and AC 120-85B treats a ULD as tare tracked with the load.
/// `alas_mass::flops_transport::FlopsOperatingItemsBreakdown` therefore
/// computes the tare for every configuration from this declaration and reports
/// it *outside* operating empty mass, with FLOPS' own `WOPIT` convention
/// retained alongside it. The consequence for this function is that the
/// declaration below changes no operating empty mass at all: it
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
/// A shaft-power aircraft below 40 t takes
/// [`CabinEquipmentMethod::RegionalTurbopropV1`] instead: the LTH fit
/// population is four turbofans of 52-233 t and contains no turboprop, so the
/// seat clause is not evidence that the relations describe a light turboprop
/// cabin. The 72-seat ATR 72-600 is the one registered aircraft this affects.
/// The 40 t boundary is the LTH statement's own mass figure, used as an
/// engineering-estimate class boundary; a shaft-power aircraft of 40 t or more
/// keeps the LTH selection.
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
/// Effect on the operating-empty-mass error against the published references:
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
/// A220-300 130 against 140, DC-10-30 250 against 255. The seat count feeds
/// several mass terms - the LTH operating items (proportional to
/// `n_pax^1.114` on long haul), the passenger service items, the flight
/// attendants, the APU, the electrical system and the air conditioning - so
/// matching the published cabins would add at least the LTH operating-item
/// part, 3,454.8 / 2,451.7 / 372.4 / 374.7 kg respectively. That is a **configuration mismatch, not a model error**, and
/// it is not closed here: changing the FLOPS seat input would price a cabin
/// the rest of the product does not fly. It has to be closed in the cabin
/// layout, which this module does not own - and until it is, a real share of
/// the remaining operating-empty deficits is a cabin difference rather than a
/// mass method being wrong.
pub(super) fn declared_cabin_equipment_method(
    preset: &crate::AircraftPreset,
) -> CabinEquipmentMethod {
    let mtom_kg = preset
        .reference
        .mtow_kg
        .unwrap_or(preset.requirements.mtow_kg);
    let shaft_power = matches!(
        preset.geometry.engine.active_model(),
        Ok(crate::ActiveEngineModel::Turboprop(_))
    );
    CabinEquipmentMethod::for_civil_transport_size(
        Some(mtom_kg),
        Some(preset.requirements.num_passengers),
        shaft_power,
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
/// this class is load-bearing for it and not merely declarative: the
/// ATR 72-600 takes `m_opp = 32.907 n_pax^1.021`.
pub(super) fn declared_haul_class(name: &str) -> OperatingHaulClass {
    match name {
        "A340-300" | "A380-800" | "B787-9" | "DC-10" | "B747-400" | "AVE" => {
            OperatingHaulClass::LongHaul
        }
        _ => OperatingHaulClass::ShortMediumHaul,
    }
}

pub(crate) fn declared_cargo_loading(name: &str) -> CargoHoldLoading {
    match name {
        "ATR72-600" | "A220-300" | "A320-200" | "E195-E2" => CargoHoldLoading::Bulk,
        _ => CargoHoldLoading::Containerized,
    }
}
