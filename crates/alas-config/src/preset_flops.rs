// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The declared FLOPS architecture of each registered aircraft.
//!
//! Kept beside the presets rather than inside them, for the same reason
//! [`crate::preset_fuel_tanks`] and [`crate::preset_structures`] are:
//! the aircraft registry is held at its reviewed size and does not grow a
//! field for every discipline. [`inputs_for`] is the one seam the
//! configuration loader reads.
//!
//! ## Two kinds of number live here
//!
//! **Derived.** Quantities the configuration already states, mapped into the
//! FLOPS variable that means the same thing: the usable fuel capacity from
//! `reference.usable_fuel_mass_kg`, the tank count from the registered tank
//! arrangement, the cabin class split from the planning cabin, the engine
//! count from the built geometry. Re-typing those by hand would create a
//! second copy that can drift, so they are read from the one that already
//! exists and the provenance cites the preset's own source.
//!
//! **Declared.** Quantities nothing in the configuration carries -- maximum
//! operating Mach, hydraulic working pressure, minimum flight crew, engine
//! mounting, variable-sweep architecture. Each is written out per aircraft
//! with the document, revision and locator it came from.
//!
//! ## What the mapping is *not* allowed to do
//!
//! * Cruise Mach is not maximum Mach. `VMAX` is `MMO` from the type
//!   certificate or the flight manual, and where no such figure was retrieved
//!   the aircraft is left without one rather than given its cruise number.
//! * `WCARGO` is containerised cargo, not the payload brief's
//!   `cargo_payload_kg`. The two are different quantities and the second is
//!   never substituted for the first.
//! * The passenger *total* a preset states is not a class split. Where a
//!   manufacturer publishes a typical multi-class layout the split is that
//!   layout's; where it does not, the cabin is declared all-economy and the
//!   evidence kind says who decided that.
//! * A "percent composite by structural weight" figure is not FLOPS `FCOMP`.
//!   `FCOMP` is a technology coefficient running from a metallic wing to the
//!   maximum composite benefit the fits were built around, and no
//!   manufacturer publishes it. It stays at the metallic value unless a
//!   source establishes otherwise, which none does.

use crate::{
    FlopsInputEvidence, FlopsInputProvenance, FlopsStructureConfig, FlopsTransportConfig,
    FlopsTransportProvenance, FuelTankLayoutConfig,
};

/// The FLOPS architecture registered for one aircraft.
#[derive(Debug, Clone, PartialEq)]
pub struct PresetFlopsInputs {
    /// Mission, cabin and installed-architecture inputs.
    pub transport: FlopsTransportConfig,
    /// Technology factors and declared airframe/propulsion overrides.
    pub structure: FlopsStructureConfig,
}

/// A cabin class split, in installed seats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ClassSplit {
    first: usize,
    business: usize,
    tourist: usize,
}

impl ClassSplit {
    /// Every seat in the economy cabin.
    const fn all_economy(total: usize) -> Self {
        Self {
            first: 0,
            business: 0,
            tourist: total,
        }
    }
}

/// Everything about one aircraft that the configuration does not already say.
struct DeclaredArchitecture {
    /// FLOPS `VMAX`: maximum operating Mach number, `MMO`.
    maximum_mach: f64,
    /// FLOPS `DESRNG`, nautical miles.
    design_range_nmi: f64,
    /// FLOPS `NFLCR`: minimum certified flight-deck crew.
    flight_crew_count: usize,
    /// FLOPS `NSTU`: installed cabin attendants.
    flight_attendant_count: usize,
    /// FLOPS `NGALC`: dedicated galley crew.
    galley_crew_count: usize,
    /// The installed cabin's class split.
    class_split: ClassSplit,
    /// FLOPS `HYDPR`, pascals.
    hydraulic_pressure_pa: f64,
    /// FLOPS `FNEF`: engines on the fuselage or empennage. The remainder of
    /// the built engine count is `FNEW`.
    fuselage_mounted_engine_count: usize,
    /// FLOPS `NTANK`, when the aircraft's published fuel system differs from
    /// the cell count of the registered tank arrangement.
    published_tank_count: Option<usize>,
    /// Maximum usable fuel declared for a notional preset. A `None` value is
    /// resolved from the preset's source-backed reference data; a `Some`
    /// value is explicitly scenario data and keeps that distinction in the
    /// provenance family.
    maximum_fuel_capacity_kg: Option<f64>,
    /// FLOPS `WCARGO`, kilograms of containerised cargo.
    containerized_cargo_kg: f64,
    /// Evidence for the mission family.
    mission: Evidence,
    /// Evidence for the cabin family.
    cabin: Evidence,
    /// Evidence for the installed-architecture family.
    architecture: Evidence,
}

/// One input family's document, revision, locator, applicability and kind.
struct Evidence {
    document: &'static str,
    revision: &'static str,
    location: &'static str,
    applicability: &'static str,
    kind: FlopsInputEvidence,
    uncertainty: &'static str,
}

impl Evidence {
    fn into_provenance(self) -> FlopsInputProvenance {
        FlopsInputProvenance {
            document: self.document.to_owned(),
            revision: self.revision.to_owned(),
            location: self.location.to_owned(),
            applicability: self.applicability.to_owned(),
            evidence: self.kind,
            uncertainty: self.uncertainty.to_owned(),
        }
    }
}

/// FLOPS `NTANK` implied by a registered tank arrangement.
///
/// A wing cell is one tank per side, because that is what the aircraft has
/// and what the arrangement's own symmetric volume describes; the centre,
/// trim and auxiliary cells are one each. Where the aircraft's published fuel
/// system has a finer split than the arrangement models -- the A380's four
/// feed tanks are carried inside its inner and mid cells here -- the preset
/// declares the published count instead and this derivation is not used.
fn tank_count_from_layout(layout: &FuelTankLayoutConfig) -> usize {
    let wing_cells = [&layout.inner_wing, &layout.mid_wing, &layout.outer_wing]
        .into_iter()
        .filter(|cell| cell.enabled)
        .count();
    2 * wing_cells
        + usize::from(layout.center.enabled)
        + usize::from(layout.trim.enabled)
        + usize::from(layout.auxiliary.enabled)
}

/// The FLOPS architecture registered for the named preset, if it has one.
///
/// `None` means no architecture has been declared for that aircraft, and the
/// FLOPS evaluation will report every missing input by name rather than being
/// handed a value nobody stands behind.
pub fn inputs_for(preset_name: &str) -> Option<PresetFlopsInputs> {
    let preset = crate::presets::get(preset_name).ok()?;
    let declared = declared_architecture(preset_name)?;

    let engine_count = preset.geometry.engine.spanwise_positions_m.len();
    let wing_mounted = engine_count.checked_sub(declared.fuselage_mounted_engine_count)?;

    let fuel_tank_count = declared.published_tank_count.or_else(|| {
        crate::preset_fuel_tanks::layout_for(preset_name)
            .as_ref()
            .map(tank_count_from_layout)
    })?;

    let transport = FlopsTransportConfig {
        maximum_mach: Some(declared.maximum_mach),
        design_range_nmi: Some(declared.design_range_nmi),
        flight_crew_count: Some(declared.flight_crew_count),
        flight_attendant_count: Some(declared.flight_attendant_count),
        galley_crew_count: Some(declared.galley_crew_count),
        first_class_passenger_count: Some(declared.class_split.first),
        business_class_passenger_count: Some(declared.class_split.business),
        tourist_class_passenger_count: Some(declared.class_split.tourist),
        hydraulic_pressure_pa: Some(declared.hydraulic_pressure_pa),
        // Every registered aircraft is a fixed-geometry wing. This is a
        // declared architecture fact, not an absence treated as zero: FLOPS
        // reports a missing `VARSWP` rather than assuming one.
        variable_sweep_penalty: Some(0.0),
        wing_mounted_engine_count: Some(wing_mounted),
        fuselage_mounted_engine_count: Some(declared.fuselage_mounted_engine_count),
        fuel_tank_count: Some(fuel_tank_count),
        maximum_fuel_capacity_kg: declared
            .maximum_fuel_capacity_kg
            .or(preset.reference.usable_fuel_mass_kg),
        containerized_cargo_kg: Some(declared.containerized_cargo_kg),
        provenance: FlopsTransportProvenance {
            mission: declared.mission.into_provenance(),
            cabin: declared.cabin.into_provenance(),
            architecture: declared.architecture.into_provenance(),
        },
    };

    Some(PresetFlopsInputs {
        transport,
        structure: declared_structure(preset_name),
    })
}

#[path = "preset_flops/architecture.rs"]
mod architecture;
#[path = "preset_flops/structure.rs"]
mod structure;
use architecture::declared_architecture;
use structure::declared_structure;

#[cfg(test)]
#[path = "preset_flops/tests.rs"]
mod tests;
