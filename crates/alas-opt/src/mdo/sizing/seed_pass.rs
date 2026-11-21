// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The seed mass pass of one candidate: the first mass analysis of its drawn
//! aircraft and, in a mode that sizes at the declared mass, the takeoff fuel
//! loading priced on it.
//!
//! The pass is a function of the aircraft it is handed. A clean-sheet
//! passenger transport derives its wing setting after the first trim and
//! rebuilds the aircraft rotated (`wing_setting`); the pass is repeated on the
//! rotated aircraft, because that is the one the report rebuilds from
//! `TailSizing::wing_incidence_deg`. A loading priced on the wing before its
//! rotation differs from the report's mass ledger by the rotation's own
//! effect on the wing's planform (0.05 to 5 kg on the transports of the
//! preset grid), which the design-loading identity of the model CG
//! assessment rejects.

use alas_config::{AlasConfig, DesignVector, MtowPlan};
use alas_geom::aircraft::airplane::Airplane;
use alas_mass::breakdown::{MassBreakdown, MassCoordinates};
use alas_mass::loading::{apply_mtow_fuel_loading, MtowFuelLoading};

use super::super::build::{first_mass_pass, StructuralReference};
use super::super::types::{CandidateFailure, PayloadCapacity};

/// The seed pass of one candidate.
pub(super) struct SeedPass {
    pub(super) masses: MassBreakdown,
    pub(super) coords: MassCoordinates,
    pub(super) cg: [f64; 3],
    pub(super) capacity: PayloadCapacity,
    pub(super) structural_reference: StructuralReference,
    /// The takeoff loading a mode that sizes at the declared mass flies;
    /// `None` in every other mode.
    pub(super) takeoff_loading: Option<MtowFuelLoading>,
    /// The mass the first trim and the closure's state start at, kg.
    pub(super) analysis_mass_kg: f64,
}

/// The seed pass of `config`'s aircraft `plane`, drawn from `dv`, under the
/// sizing `plan`.
pub(super) fn seed_pass(
    config: &AlasConfig,
    plan: &MtowPlan,
    dv: &DesignVector,
    plane: &Airplane,
) -> Result<SeedPass, CandidateFailure> {
    // A mission-sized mode reads the first pass at its seed mass, a fixed
    // requirement at the declared one.
    let seeded = plan
        .requires_mission_sized_evaluation()
        .then(|| config.at_sized_closure_mass(plan.seed_kg));
    let (mut masses, mut coords, mut cg, _summary, capacity, structural_reference) =
        first_mass_pass(seeded.as_ref().unwrap_or(config), dv, plane)?;
    // The structural pass stays at the declared design weights. Only the
    // usable fuel and its physical moment follow the loadable takeoff case.
    let takeoff_loading = if plan.mode.sizes_at_declared_mass() {
        let (loading, loaded_cg) =
            apply_mtow_fuel_loading(config, dv, plane, &mut masses, &mut coords)
                .map_err(|_| CandidateFailure::new("mass_coordinates"))?;
        cg = loaded_cg;
        Some(loading)
    } else {
        None
    };
    let analysis_mass_kg = takeoff_loading.map_or(plan.seed_kg, |load| load.takeoff_mass_kg);
    Ok(SeedPass {
        masses,
        coords,
        cg,
        capacity,
        structural_reference,
        takeoff_loading,
        analysis_mass_kg,
    })
}
