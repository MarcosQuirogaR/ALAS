// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! FLOPS architecture of the registered military transport (Airbus A400M).
//!
//! Source key as in `presets/military.rs`: S sourced, I inferred, E estimate.
//! Nothing here is a calibration to A400M results.

use super::architecture_rows::declared_haul_class;
use super::{ClassSplit, DeclaredArchitecture, Evidence};
use crate::{
    CabinEquipmentMethod, CargoHoldLoading, FlopsInputEvidence, FlopsStructureConfig,
    FlopsTurbopropConfig, PropellerConstruction, PylonMassMethod,
};

/// Hydraulic working pressure, Pa. E: 3,000 psi class value, the A400M
/// figure is not published in the sources retained.
const HYDRAULIC_3000_PSI_PA: f64 = 3_000.0 * 6_894.757_293_168;

/// FLOPS transport architecture of the A400M, `None` for any other name.
pub(super) fn declared_architecture(name: &str) -> Option<DeclaredArchitecture> {
    if name != "A400M" {
        return None;
    }
    Some(DeclaredArchitecture {
        maximum_mach: 0.72,       // S: T1 III.10 MMO
        design_range_nmi: 3_400.0, // S: B1 p024 20 t / 6300 km point
        flight_crew_count: 3,     // E: two pilots and a loadmaster
        flight_attendant_count: 0,
        galley_crew_count: 0,
        class_split: ClassSplit::all_economy(0), // no passenger seats
        hydraulic_pressure_pa: HYDRAULIC_3000_PSI_PA,
        fuselage_mounted_engine_count: 0,
        published_tank_count: Some(7), // S: T1 III.9 centre, 2 inner, 4 feed tanks
        maximum_fuel_capacity_kg: None, // taken from reference.usable_fuel_mass_kg (S: T1)
        containerized_cargo_kg: 0.0,
        cargo_loading: CargoHoldLoading::Containerized,
        containerized_baggage_fraction: None,
        // E: the LTH civil-transport relations are fitted on passenger turbofan
        // transports (and are zero at zero seats); this military cargo aircraft
        // keeps FLOPS equation 110 and the FLOPS operating items.
        cabin_equipment_method: CabinEquipmentMethod::FlopsTransportV1,
        haul_class: declared_haul_class(name),
        mission: Evidence {
            document: "Airbus A400M brochure TMMA0026/01/2025 and EASA TCDS A.169 Issue 07",
            revision: "TMMA0026/01/2025 / Issue 07, 2025-11-28",
            location: "B1 p024 range-payload table; T1 III.10",
            applicability: "military 141 t standard; 20 t at 6300 km (3400 nmi) design point, reserves not stated",
            kind: FlopsInputEvidence::UserDeclared,
            uncertainty: "design range is a declared study value; the cargo model cannot carry the 37 t and 30 t brochure points",
        },
        cabin: Evidence {
            document: "Registered preset planning cabin",
            revision: "ALAS preset registry",
            location: "AircraftPreset::planning_cabin_config",
            applicability: "no passenger seats; military cargo hold",
            kind: FlopsInputEvidence::UserDeclared,
            uncertainty: "zero installed seats by declaration",
        },
        architecture: Evidence {
            document: "EASA TCDS A.169 Issue 07 and EASA TCDS E.033 Issue 08",
            revision: "Issue 07 / Issue 08",
            location: "III.9 fuel system, engine installation",
            applicability: "A400M, fixed wing, four wing-mounted TP400-D6; crew of three and APU are estimates",
            kind: FlopsInputEvidence::UserDeclared,
            uncertainty: "crew count, APU and hydraulic pressure are estimates",
        },
    })
}

/// Structure inputs the generic declaration cannot express for the A400M.
pub(super) fn adjust_structure(name: &str, structure: &mut FlopsStructureConfig) {
    if name != "A400M" {
        return;
    }
    // S: B1 p024 military design masses, declared so no other aircraft's
    // limits supply the design landing mass.
    structure.design_gross_mass_kg = Some(141_000.0);
    structure.design_landing_mass_kg = Some(123_000.0);
    // E: FLOPS CARGF 1.0, the military cargo floor.
    structure.military_cargo_floor = 1.0;
    // E: CFRP wing skins are mentioned only by a secondary article (S1).
    structure.composite_utilization = 0.3;
    // E: the LTH box-beam pylon relation was fitted on turbofans; the nacelles
    // are treated as faired into the wing, as for the ATR 72-600.
    structure.pylon_mass_method = PylonMassMethod::None;
}

/// Shaft-power propulsion-group record of the A400M, `None` for any other name.
pub(super) fn declared_turboprop(name: &str) -> Option<FlopsTurbopropConfig> {
    if name != "A400M" {
        return None;
    }
    Some(FlopsTurbopropConfig {
        engine_dry_mass_kg: Some(1_952.0), // S: T2 mean of CW 1,938.1 / CCW 1,965.1 kg
        baseline_shaft_power_kw: Some(7_971.0), // S: T2 takeoff rating
        engine_mass_scaling_exponent: 1.0,
        gearbox_inside_engine_mass: true, // E: the PGB is listed in the engine TCDS
        propeller_blade_count: 8,         // S: T3
        propeller_activity_factor: 130.0, // E: ATR fallback value, unused with an assembly mass
        propeller_construction: PropellerConstruction::Composite, // E
        propeller_weight_coefficient: Some(170.0), // E: ATR value (NASA TM-83458 band 160-180)
        propeller_accessory_mass_kg: 0.0,
        propeller_assembly_mass_kg: Some(683.0), // S: T3 maximum per propeller
        propeller_assembly_accessories_included: None,
        // E: ATR nacelle area density 19.7 kg/m2 kept; no A400M nacelle mass.
        nacelle_area_density_kg_m2: 19.7,
        nacelle_reference_mass_kg: None,
        nacelle_reference_area_m2: None,
        pylon_coefficient: 0.0,
        // E: 0.32 x engine dry mass (ATR ratio) for four engines.
        engine_installation_mass_kg: 0.32 * 4.0 * 1_952.0,
        engine_oil_mass_kg: 150.0, // E: four engines
    })
}
