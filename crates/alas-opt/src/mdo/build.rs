// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The candidate's geometry, mass and trimmed aerodynamic operating point --
//! the part of the legacy evaluation that does not depend on the takeoff
//! mass, so it is built exactly once per candidate.
//!
//! This reuses the same public crate calls, in the same order, as
//! `crate::objective_evaluate`'s legacy path: geometry build, candidate
//! payload load case, a two-pass mass analysis with the payload layout
//! summary, the cruise stall guard, and `stability_and_trim` followed by
//! `AeroAnalysis::trimmed_performance`. A design vector that fails any of
//! these is not a physically evaluable aircraft, independent of which
//! mission quantity the search is minimising.

use alas_config::design_variables::{DesignVector, SPECS};
use alas_config::optimizer::DesignMode;
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::Wing;
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{
    calculate_physical_cg, run_mass_analysis_with_model_checked_product_with_gear, MassBreakdown,
    MassCoordinateModel, MassCoordinates, PayloadLayoutSummary, OEW_KEYS,
};
use alas_mass::flops_transport::structure::{FlopsWingInputs, WingBendingFactor};
use alas_mass::torenbeek::{
    mass_wing_with_control_surface_area, wing_secondary_mass_breakdown_with_control_surface_area,
    WingSecondaryMassBreakdown,
};
use alas_mass::wing_inventory::{
    build_wing_inventory, FixedNonBoxStructure, MovableSurface, TorenbeekWingGroup,
    WingInventoryInputs, WingMovableSurfaces, WingNonBoxInventory,
};
use alas_mass::wingbox_feedback::{
    reconcile_clean_sheet_wing, reconcile_reference_wing, ReferenceWingMass, SizedWingboxMass,
    WingboxFeedback,
};
use alas_payload::build::build_payload_layout;
use alas_payload::layout::LayoutSummary;
use alas_payload::oew::oew_and_cg;
use alas_struct::sizing::{size_wingbox, WingboxSizing};

use crate::objective::apply_candidate_payload_load_case;

use super::types::{CandidateFailure, PayloadCapacity};

/// Structural result retained between the initial mass pass and later MDA
/// passes. Reference adaptation freezes the empirical total and its secondary
/// first moment once per candidate; clean-sheet runs deliberately carry no
/// reference fallback.
#[derive(Debug, Clone)]
pub(crate) struct StructuralReference {
    pub reference: Option<ReferenceWingMass>,
    pub inventory_complete: bool,
    pub feedback: WingboxFeedback,
    /// Provenance of the non-box wing inventory behind `inventory_complete`,
    /// including the enumerated clean-sheet items and their plausibility
    /// diagnostics.
    ///
    /// Nothing downstream reads it yet: surfacing the item list and the
    /// empirical-group ratios through `SizingOutcome`/`SizedCandidate` is a
    /// reporting change in `mdo/sizing.rs` and `mdo/types.rs`.
    #[expect(
        dead_code,
        reason = "diagnostics awaiting a reporting seam in mdo::sizing and mdo::types"
    )]
    pub inventory: StructuralInventory,
}

/// Where the non-box part of the reconciled wing comes from.
///
/// Reference adaptation and the baseline sandbox freeze a measured empirical
/// wing, so their non-box inventory is complete by construction and carries no
/// item list. Clean-sheet runs build the enumerated
/// [`alas_mass::wing_inventory`] list and are complete only when that list is.
#[derive(Debug, Clone)]
pub(crate) enum StructuralInventory {
    /// Frozen empirical remainder of a registered reference aircraft.
    FrozenReference,
    /// Enumerated, sourced clean-sheet non-box inventory.
    CleanSheet(Box<WingNonBoxInventory>),
}

impl StructuralInventory {
    /// Whether the wing inventory may be presented as complete.
    pub(crate) fn is_complete(&self) -> bool {
        match self {
            Self::FrozenReference => true,
            Self::CleanSheet(inventory) => inventory.status().is_complete(),
        }
    }
}

/// Failure with the `geometry_build` reason, for every early exit below.
fn geometry_build_failure() -> CandidateFailure {
    CandidateFailure {
        reason: "geometry_build",
    }
}

/// Build the candidate's geometry and payload-recomputed configuration.
pub(crate) fn build_geometry(
    config: &AlasConfig,
    x: &[f64],
) -> Result<(AlasConfig, DesignVector, Airplane), CandidateFailure> {
    let mut dv = DesignVector::from_array(x).map_err(|_| geometry_build_failure())?;
    let mut candidate_config = config.clone();
    if apply_candidate_payload_load_case(&mut candidate_config, &dv).is_err() {
        return Err(geometry_build_failure());
    }
    size_fuselage_from_cabin(&candidate_config, &mut dv)?;
    let builder = AircraftBuilder::new(Some(candidate_config.geometry.clone()));
    let plane = builder
        .build(Some(&dv), false)
        .map_err(|_| geometry_build_failure())?;
    if plane.s_ref <= 0.0 || plane.c_ref <= 0.0 {
        return Err(geometry_build_failure());
    }
    Ok((candidate_config, dv, plane))
}

/// Derive the shortest clean-sheet body that can carry the requested
/// passenger load case under the configured cabin/exit rules.
///
/// The fuselage coordinate is a fixed, derived variable in this mode.  The
/// optimizer therefore receives the derived value as its nominal and bounds
/// it to one point; the evaluator repeats this deterministic solve so a
/// returned [`DesignVector`] rebuilds the same geometry.  Cargo layouts keep
/// their explicit hold sizing and do not use the passenger cabin relation.
pub(crate) fn size_fuselage_from_cabin(
    config: &AlasConfig,
    design: &mut DesignVector,
) -> Result<(), CandidateFailure> {
    if !config.optimizer.design_space.sizes_fuselage_from_cabin()
        || config.requirements.aircraft_type == "cargo"
    {
        return Ok(());
    }
    let target = config.requirements.num_passengers.max(0);
    let Some(spec) = SPECS.iter().find(|spec| spec.name == "fuselage_length_m") else {
        return Err(geometry_build_failure());
    };
    let mut lower = spec.lower;
    let mut upper = spec.upper;

    let capacity_at = |length_m: f64| -> Result<i64, CandidateFailure> {
        let mut trial = *design;
        trial.fuselage_length_m = length_m;
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&trial), false)
            .map_err(|_| geometry_build_failure())?;
        let layout =
            build_payload_layout(&plane, config, 0.0, 0.0).map_err(|_| CandidateFailure {
                reason: "payload_layout",
            })?;
        match layout.summary {
            LayoutSummary::Passenger(summary) => Ok(summary.max_certifiable_capacity),
            LayoutSummary::Cargo(_) => Err(geometry_build_failure()),
        }
    };

    if capacity_at(lower)? >= target {
        design.fuselage_length_m = lower;
        return Ok(());
    }
    if capacity_at(upper)? < target {
        return Err(geometry_build_failure());
    }
    // Bisection is sufficient because available cabin floor is monotone in
    // body length for a fixed planform/class mix; the final value is kept in
    // full precision rather than rounded to the display decimals.
    for _ in 0..36 {
        let middle = 0.5 * (lower + upper);
        if capacity_at(middle)? >= target {
            upper = middle;
        } else {
            lower = middle;
        }
    }
    design.fuselage_length_m = upper;
    Ok(())
}

/// Run the two-pass mass analysis at `config.requirements.mtow_kg`, the
/// ceiling every sizing pass starts from, and return the masses, coordinates,
/// physical CG and the payload-layout summary later passes reuse.
pub(crate) fn first_mass_pass(
    config: &AlasConfig,
    dv: &DesignVector,
    plane: &Airplane,
) -> Result<FirstMassPassOutput, CandidateFailure> {
    let (m1, c1, _cg1, _initial_feedback, reference, inventory) =
        mass_analysis_with_structural_feedback(config, dv, plane, None, None)?;
    let (oew, x_oew) = oew_and_cg(&m1, &c1);
    let payload_layout =
        build_payload_layout(plane, config, oew, x_oew).map_err(|_| CandidateFailure {
            reason: "payload_layout",
        })?;
    let summary = PayloadLayoutSummary {
        total_mass: payload_layout.total_mass,
        cg_x: payload_layout.cg_x,
        cg_y: payload_layout.cg_y,
    };
    let capacity = match &payload_layout.summary {
        LayoutSummary::Passenger(passenger) => PayloadCapacity {
            passenger_capacity: passenger.max_certifiable_capacity,
            carried_passengers: passenger.seated_pax,
            cargo_capacity_kg: 0.0,
            carried_cargo_payload_kg: 0.0,
        },
        LayoutSummary::Cargo(cargo) => PayloadCapacity {
            passenger_capacity: 0,
            carried_passengers: 0,
            cargo_capacity_kg: cargo.capacity_t * 1_000.0,
            carried_cargo_payload_kg: cargo.loaded_net_payload_t * 1_000.0,
        },
    };
    let (masses, coords, cg, feedback, _, _) =
        mass_analysis_with_structural_feedback(config, dv, plane, Some(&summary), reference)?;
    Ok((
        masses,
        coords,
        cg,
        summary,
        capacity,
        StructuralReference {
            reference,
            inventory_complete: inventory.is_complete(),
            feedback,
            inventory,
        },
    ))
}

type FirstMassPassOutput = (
    MassBreakdown,
    MassCoordinates,
    [f64; 3],
    PayloadLayoutSummary,
    PayloadCapacity,
    StructuralReference,
);

type StructuralMassAnalysis = (
    MassBreakdown,
    MassCoordinates,
    [f64; 3],
    WingboxFeedback,
    Option<ReferenceWingMass>,
    StructuralInventory,
);

fn reclose_mass(
    mut masses: MassBreakdown,
    mut coords: MassCoordinates,
    requirements: &alas_config::DesignRequirements,
    payload_summary: Option<&PayloadLayoutSummary>,
) -> (MassBreakdown, MassCoordinates, [f64; 3]) {
    if let Some(layout) = payload_summary.filter(|layout| layout.total_mass > 0.0) {
        masses.payload = layout.total_mass;
        coords.payload = [layout.cg_x, layout.cg_y, coords.payload[2]];
    }
    let oew: f64 = OEW_KEYS
        .iter()
        .map(|&key| masses.get(key).unwrap_or(0.0))
        .sum();
    masses.fuel = requirements.mtow_kg - oew - masses.payload;
    let cg = calculate_physical_cg(&masses, &coords);
    (masses, coords, cg)
}

include!("build_wing_geometry.rs");
include!("build_structural.rs");
