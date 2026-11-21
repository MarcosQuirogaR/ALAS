// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The wing over the ground of a built airplane, for the ground-effect
//! reduction of the take-off lift-dependent drag
//! (`alas_perf::performance::GroundEffect`).

use alas_config::AlasConfig;
use alas_perf::performance::GroundEffect;

use super::YawAirframe;

/// Span, mean geometric chord and the MAC quarter-chord and trailing-edge
/// heights of the main wing above the static ground line
/// (`alas_mass::stations::ground_plane_z_m`, the plane the gear stands on at
/// the take-off weight), in the level attitude: the two heights of G-77's
/// `2 h_eff` and of the lift ratio. `None` without a wing or fuselage, or for
/// degenerate geometry (the field is then flown in free air, which is the
/// conservative bound).
///
/// The fallback of a field without a lift-off schedule: with one, the drag
/// reads the schedule's own wing (`crate::envelope::LiftOffLift::ground_effect()`)
/// raised by the rotation's strut extension and attitude.
pub(super) fn build(config: &AlasConfig, frame: YawAirframe<'_>) -> Option<GroundEffect> {
    let plane = frame.plane;
    let wing = plane.wings.first()?;
    let fuselage = plane.fuselages.first()?;
    // The large end of an undeclared clearance: a higher wing gains less from
    // the ground, which is the conservative side for the engine-out climb.
    let sample = alas_mass::stations::clearance_sample(
        wing,
        fuselage,
        &config.geometry,
        &config.landing_gear,
    );
    let ground_z_m = alas_mass::stations::ground_plane_z_for(
        alas_mass::stations::GroundPlaneBasis::High,
        sample,
        fuselage,
        &config.geometry,
        &config.landing_gear,
    );
    let mac = wing.carried_box();
    let height_m = mac.aerodynamic_center(0.25)[2] - ground_z_m;
    let trailing_edge_m = mac.aerodynamic_center(1.0)[2] - ground_z_m;
    GroundEffect::new(
        plane.b_ref,
        plane.s_ref / plane.b_ref,
        height_m,
        trailing_edge_m,
    )
}
