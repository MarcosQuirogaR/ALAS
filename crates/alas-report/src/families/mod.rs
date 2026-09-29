// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Figure generation families categorized by engineering discipline.
//!
//! Each module constructs backend-neutral [`crate::scene::Scene`] structures
//! representing engineering diagrams, polar charts, flight profiles, or 3D wireframes.

/// What a gear figure says instead of drawing a station nothing measured.
///
/// Raised when `alas_pipeline::gear_stations::resolved_gear_stations` refuses
/// the wing-mounted main-gear fallback for a layout outside its domain: the
/// aircraft registers no source gear-station anchor and its wing root sits
/// above the fuselage crown, so there is no wing-root gear bay the rule could
/// place legs in. This is a missing datum, closed by registering the
/// aircraft's published gear stations, and the figure must not stand in a
/// number for it.
pub(crate) const MAIN_GEAR_STATION_NOT_MEASURED: &str =
    "No main-gear station measured for this layout (no published gear-station anchor; wing root above fuselage crown)";

/// Build the same typed [`alas_opt::ModelCgEnvelopeAssessment`] the
/// feasibility pipeline evaluates a design against, from a figure's own
/// [`alas_pipeline::full_analysis::AnalysisReport`] and [`alas_config::AlasConfig`].
///
/// Shared by [`mass_balance::figure_cg_envelope`] and
/// [`mass_balance_layout::figure_landing_gear_planform`] so both figures
/// read the identical aft/forward physical boundaries instead of each
/// re-deriving its own simplified aerodynamic/gear formula.
///
/// Mirrors `alas_pipeline::feasibility::model_cg::model_cg_assessment`'s own
/// construction of the ten canonical mass/coordinate components, except
/// that the fuel mass used here is the report's own `FUEL` component
/// (this crate has no access to `FuelLoadingAssessment`'s separately
/// resolved analyzed-carried-fuel value); on every registered preset this
/// is the same mass the pipeline's own fuel-loading assessment analyzes,
/// so the two only diverge for a design whose usable-fuel cap actually
/// binds.
pub(crate) fn model_cg_gate_assessment(
    report: &alas_pipeline::full_analysis::AnalysisReport,
    config: &alas_config::AlasConfig,
) -> Result<alas_opt::ModelCgEnvelopeAssessment, alas_opt::ModelCgEnvelopeError> {
    use alas_mass::breakdown::{
        MassBreakdown, MassCoordinates, FUEL, FURNISHINGS, FUSELAGE, GEAR, H_STAB, PAYLOAD,
        PROPULSION, SYSTEMS, V_STAB, WING,
    };
    let mass = |name: &str| report.component_masses.get(name).copied();
    let coordinate = |name: &str| report.mass_coordinates.get(name).copied();
    let (Some(masses), Some(coordinates)) = (
        (|| {
            Some(MassBreakdown {
                wing: mass(WING)?,
                h_stab: mass(H_STAB)?,
                v_stab: mass(V_STAB)?,
                fuselage: mass(FUSELAGE)?,
                gear: mass(GEAR)?,
                propulsion: mass(PROPULSION)?,
                systems: mass(SYSTEMS)?,
                furnishings: mass(FURNISHINGS)?,
                payload: mass(PAYLOAD)?,
                fuel: mass(FUEL)?,
            })
        })(),
        (|| {
            Some(MassCoordinates {
                wing: coordinate(WING)?,
                h_stab: coordinate(H_STAB)?,
                v_stab: coordinate(V_STAB)?,
                fuselage: coordinate(FUSELAGE)?,
                gear: coordinate(GEAR)?,
                propulsion: coordinate(PROPULSION)?,
                systems: coordinate(SYSTEMS)?,
                furnishings: coordinate(FURNISHINGS)?,
                payload: coordinate(PAYLOAD)?,
                fuel: coordinate(FUEL)?,
            })
        })(),
    ) else {
        return Err(alas_opt::ModelCgEnvelopeError::InvalidInput);
    };
    let critical_x_np = report
        .neutral_point_conditions
        .as_ref()
        .map_or(report.x_neutral_point, |conditions| conditions.critical);
    alas_opt::assess_model_cg_envelope(
        &report.airplane,
        &masses,
        &coordinates,
        report.physical_cg[0],
        report.x_neutral_point,
        critical_x_np,
        report.airplane.c_ref,
        config,
    )
}

pub mod aerodynamics;
mod common;
pub mod geometry;
pub mod mass_balance;
pub mod mass_balance_layout;
pub mod mission;
pub mod optimization;
pub mod performance;
pub mod propulsion;
pub mod screening;
pub mod stability;
pub mod structures;
pub mod structures_dynamics;
