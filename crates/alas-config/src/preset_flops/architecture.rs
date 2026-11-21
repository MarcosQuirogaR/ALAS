// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::{ClassSplit, DeclaredArchitecture, Evidence};
use crate::FlopsInputEvidence;

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
        "A350-900" => (
            0.89,
            8_500.0,
            2,
            HYDRAULIC_5000_PSI_PA,
            0,
            Some(3),
            Some(130_693.0),
            Evidence {
                document: "Airbus A350 Family Facts and Figures; Poll and Schumann 2020 Part 2",
                revision: "June 2026 / Table 1",
                location: "p.3 (range) and MMO column",
                applicability: "A350-941 / WV020 HGW; design range is a declared study value",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "payload and reserves of the 8,500 nmi range are not defined",
            },
            Evidence {
                document: "EASA.A.151 type-certificate data",
                revision: "Issue 33, 2026-07-01",
                location: "section III.2.2: 130,693 kg usable at 0.785 kg/L, mod 110211",
                applicability: "A350-941 / Trent XWB-84, fixed wing, two wing-mounted engines",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "hydraulic pressure, containerized cargo and FLOPS equivalent tank topology are study mappings",
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
        "C295" => (
            // I: 0.49 is VMO 200 kt at 25,000 ft, the highest Mach on the T1 III.10 line, ISA,
            // IAS read as CAS. The TCDS prints no MMO.
            0.49,
            810.0, // S: A1 about 1,500 km with the 9 t payload
            2,     // S: T1 III.18 pilot and co-pilot; J1 p.545 crew of two
            // S: J1 p.293 CN-235 207 bar; J1 p.545 C-295 systems identical to the CN-235 Srs 300.
            HYDRAULIC_3000_PSI_PA,
            0,
            Some(4), // S: T1 III.9 main and auxiliary tank in each wing
            None,
            Evidence {
                document: "Airbus C295 product page and EASA.A.186 CN-235/C-295 TCDS",
                revision: "accessed 2026-11-05 / Issue 08, 2025-12-09",
                location: "product page payload-range statement; TCDS Section 5 III.10",
                applicability: "C-295M overload weights; 9 t over 1,500 km, reserves not stated",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "advertised range as a study proxy; maximum Mach derived from the VMO line, no MMO printed",
            },
            Evidence {
                document: "EASA.A.186 CN-235/C-295 TCDS and Jane's All the World's Aircraft 2008-2009",
                revision: "Issue 08, 2025-12-09 / 2008-2009 edition",
                location: "TCDS Section 5 III.5-III.9 and III.18; Jane's p.293 and p.545",
                applicability: "C-295 / PW127G / HSD 568F-5, two wing-mounted engines, four wing tanks, no APU (TCDS III.6 N/A)",
                kind: FlopsInputEvidence::SourceBacked,
                uncertainty: "hydraulic pressure read across from the CN-235 Srs 300; a loadmaster is not part of the certified minimum crew",
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
                applicability: "DC-10-30 / CF6-50C1, two wing-mounted engines and one tail-mounted engine",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "containerized cargo is zero by study declaration; tank count follows the ACAP layout",
            },
        ),
        "B747-400" => (
            0.92,
            5_700.0,
            2,
            HYDRAULIC_3000_PSI_PA,
            0,
            Some(8),
            None,
            Evidence {
                document: "Boeing 747-400 Airplane Characteristics for Airport Planning",
                revision: "D6-58326-1 Rev F, December 2024",
                location: "section 3.2.1 payload/range corner (5,700 nmi, +-100 nmi read)",
                applicability: "747-400 / CF6-80C2B1F, 875,000 lb MTOW column; design range is the maximum-structural-payload corner; maximum Mach 0.92 and the two-pilot crew are recalled declarations, not read from the ACAP",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "MMO and crew mapping unverified; no complete source mission load case",
            },
            Evidence {
                document: "Boeing 747-400 Airplane Characteristics for Airport Planning",
                revision: "D6-58326-1 Rev F, December 2024",
                location: "the Engines table (p.1-5), sections 2.1.1 and 2.2.1",
                applicability: "747-400 / CF6-80C2B1F, four wing-mounted engines; 3,000 psi hydraulics and the eight-tank count (four mains, two reserve, centre, stabiliser) are recalled, not read from the ACAP",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "hydraulic pressure and tank-count mapping unverified; containerized cargo is zero by study declaration",
            },
        ),
        _ => super::architecture_rows::narrowbody_rows(name)?,
    };

    // The operational minimum of EASA ORO.CC.100(b): the type-certificate
    // number for the cabin configuration, never fewer than one per 50 seats
    // on a deck (`presets::minimum_cabin_crew`). This is not FLOPS:
    // NASA/TM-2017-219627 eq. 116 reads `NSTU = 1 + ceil(NPASS/40)`, a
    // heavier count for a transport this size. The deck split is not known
    // at declaration, so every seat is counted on the main deck here; the
    // payload layout re-derives the number deck by deck and
    // `cabin_synchronized_for_cabin` (`alas-opt::mdo::cabin_sync`) raises
    // the declared count to it.
    let flight_attendant_count = match name {
        // A business jet flown under TCDS III.9: two crew, no cabin crew required.
        "LJ60" => 0,
        _ if crate::systems_mass::minimum_cabin_crew(seats) == 0 => 0,
        _ => crate::presets::minimum_cabin_crew(name, seats, 0),
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
        cargo_loading: super::architecture_rows::declared_cargo_loading(name),
        containerized_baggage_fraction: None,
        cabin_equipment_method: super::architecture_rows::declared_cabin_equipment_method(preset),
        haul_class: super::architecture_rows::declared_haul_class(name),
        mission,
        cabin,
        architecture,
    })
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
        "A350-900" if seats == 325 => (
            ClassSplit {
                first: 0,
                business: 32,
                tourist: 293,
            },
            Evidence {
                document: "Registered A350-900 study cabin",
                revision: "study declaration",
                location: "preset requirements",
                applicability: "325 seats, between the 315 of the Airbus AC section 2-1-0 and the 332 of the June 2026 Facts and Figures",
                kind: FlopsInputEvidence::UserDeclared,
                uncertainty: "class split is a study value; neither Airbus layout has 325 seats.",
            },
        ),
        "B747-400" if seats == 400 => (
            ClassSplit {
                first: 24,
                business: 74,
                tourist: 302,
            },
            Evidence {
                document: "Boeing 747-400 Airplane Characteristics for Airport Planning",
                revision: "D6-58326-1 Rev F, December 2024",
                location: "section 2.1.1 typical seating capacity",
                applicability: "747-400; 24 first + 32 business + 302 economy on the main deck and 42 business on the upper deck match the registered 400 passengers",
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

pub(super) const HYDRAULIC_3000_PSI_PA: f64 = 3_000.0 * 6_894.757_293_168;
const HYDRAULIC_5000_PSI_PA: f64 = 5_000.0 * 6_894.757_293_168;
