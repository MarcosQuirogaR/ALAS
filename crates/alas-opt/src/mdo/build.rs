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
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{
    calculate_physical_cg, run_mass_analysis_with_model_checked_product_with_gear, MassBreakdown,
    MassCoordinateModel, MassCoordinates, PayloadLayoutSummary, OEW_KEYS,
};
use alas_mass::product_stations::product_mass_coordinates;
use alas_mass::wingbox_feedback::{ReferenceWingMass, WingboxFeedback};
use alas_payload::build::build_payload_layout;
use alas_payload::layout::LayoutSummary;
use alas_payload::oew::oew_and_cg;

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

pub(crate) use alas_mass::wing_reconciliation::StructuralInventory;

/// Failure with the `geometry_build` reason, for every early exit below.
fn geometry_build_failure() -> CandidateFailure {
    CandidateFailure {
        reason: "geometry_build",
    }
}

/// Build the candidate's geometry and payload-recomputed configuration.
///
/// The production path builds through the structural reconciliation; this
/// geometry-only form remains the fixture the unit tests construct their
/// candidates with.
#[cfg(test)]
pub(crate) fn build_geometry(
    config: &AlasConfig,
    x: &[f64],
) -> Result<(AlasConfig, DesignVector, Airplane), CandidateFailure> {
    build_geometry_with_fuselage_policy(config, x, false)
}

/// Build a candidate while optionally preserving a caller-pinned fuselage
/// length.  Clean-sheet searches use the derived cabin sizing solve; a fixed
/// desktop/reference vector uses the literal coordinate it supplied so the
/// downstream report and exported geometry remain the same aircraft.
pub(crate) fn build_geometry_with_fuselage_policy(
    config: &AlasConfig,
    x: &[f64],
    preserve_explicit_fuselage_length: bool,
) -> Result<(AlasConfig, DesignVector, Airplane), CandidateFailure> {
    let mut dv = DesignVector::from_array(x).map_err(|_| geometry_build_failure())?;
    let mut candidate_config = config.clone();
    if apply_candidate_payload_load_case(&mut candidate_config, &dv).is_err() {
        return Err(geometry_build_failure());
    }
    if !preserve_explicit_fuselage_length {
        size_fuselage_from_cabin(&candidate_config, &mut dv)?;
    }
    let builder = AircraftBuilder::new(Some(candidate_config.geometry.clone()));
    // The nacelles are part of the candidate, not a reporting embellishment:
    // `alas_mass::stations` places the propulsion group at the nacelle
    // mid-length when the bodies are drawn and falls back to the wing station
    // when they are not, and `alas_aero`'s parasite buildup adds a nacelle
    // entry per drawn body. Building the search's aircraft without them made
    // the optimizer balance and trim a different aircraft from the one
    // `alas-pipeline`'s finalist report builds with `include_engines = true`.
    let plane = builder
        .build(Some(&dv), true)
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
            // Size against the seats the detailed row packer actually lays
            // out.  `max_certifiable_capacity` is a useful regulatory cap,
            // but it deliberately ignores the requested class/count load
            // case and can therefore make a bisection stop one row short
            // (for example 339 seats for a 340-seat brief).  The production
            // geometry must rebuild with every requested passenger seated.
            LayoutSummary::Passenger(summary) => Ok(summary.seated_pax),
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
    // The detailed row packer is discrete: a small body-length change can
    // move a row across a seat/exit boundary, so its *reported* seated count
    // is not strictly monotone even though the available floor is.  Retain
    // the fast bracketed solve for the usual case, but remember the capacity
    // of its selected endpoint so we never publish a vector that rebuilds one
    // seat short of the requested clean-sheet load case.
    let mut selected_capacity = capacity_at(upper)?;
    for _ in 0..36 {
        let middle = 0.5 * (lower + upper);
        let capacity = capacity_at(middle)?;
        if capacity >= target {
            upper = middle;
            selected_capacity = capacity;
        } else {
            lower = middle;
        }
    }
    if selected_capacity >= target {
        design.fuselage_length_m = upper;
        return Ok(());
    }

    // Rescue the rare non-monotone row-packing bracket.  Search the actual
    // specification interval at a bounded 0.10 m resolution and select the
    // first sampled length that seats the complete requested load.  This is
    // deliberately a fallback after bisection so normal optimizer candidates
    // retain the cheap 36-evaluation path.  The returned length is a real
    // geometry value and is re-evaluated by the ordinary builder below.
    const DISCRETE_CAPACITY_SCAN_STEP_M: f64 = 0.10;
    let span = spec.upper - spec.lower;
    let samples = (span / DISCRETE_CAPACITY_SCAN_STEP_M).ceil() as usize;
    for index in 0..=samples {
        let length = (spec.lower + index as f64 * DISCRETE_CAPACITY_SCAN_STEP_M).min(spec.upper);
        if capacity_at(length)? >= target {
            design.fuselage_length_m = length;
            return Ok(());
        }
    }

    // The upper endpoint was already proven feasible. Reaching this branch
    // means the interval or the geometry builder changed between calls; keep
    // the failure typed instead of returning a short aircraft.
    Err(geometry_build_failure())
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

include!("build_structural.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_sheet_sizing_rebuilds_with_every_requested_passenger_seated() {
        let mut config = AlasConfig::default();
        config.requirements.num_passengers = 340;
        let mut design = DesignVector::default();

        size_fuselage_from_cabin(&config, &mut design)
            .unwrap_or_else(|failure| panic!("{}", failure.reason));
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), false)
            .expect("sized clean-sheet geometry");
        let layout = build_payload_layout(&plane, &config, 0.0, 0.0)
            .expect("sized clean-sheet payload layout");

        match layout.summary {
            LayoutSummary::Passenger(summary) => {
                assert_eq!(summary.total_pax, 340);
                assert_eq!(summary.unseated_pax, 0);
                assert!(summary.seated_pax >= config.requirements.num_passengers);
            }
            LayoutSummary::Cargo(_) => panic!("clean-sheet passenger sizing built cargo"),
        }
    }
}
