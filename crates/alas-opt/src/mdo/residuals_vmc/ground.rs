// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The ground-run data of the `VMCG` model
//! (`alas_perf::performance::yaw_balance::GroundRunGeometry`): mass, yaw
//! inertia, gear arms and the ground-run lift.

use alas_perf::performance::takeoff_field::GROUND_LIFT_COEFFICIENT;
use alas_perf::performance::yaw_balance::GroundRunGeometry;

use super::VmcInputs;

/// Nondimensional yaw radius of gyration `R_z` of Raymer, *Aircraft Design:
/// A Conceptual Approach*, 6th ed., Table 16.1, p. 648, by layout: twin
/// turboprop transport 0.38, jet transport with fuselage-mounted engines
/// 0.44, with two wing-mounted engines 0.46, with four 0.45.
fn yaw_radius_of_gyration(inputs: &VmcInputs<'_>, turboprop: bool, fuselage_width_m: f64) -> f64 {
    let positions = &inputs.config.geometry.engine.spanwise_positions_m;
    if turboprop {
        0.38
    } else if positions.len() >= 4 {
        0.45
    } else if positions.iter().all(|y| y.abs() <= fuselage_width_m) {
        0.44
    } else {
        0.46
    }
}

/// Mass, Raymer Eq. (16.54) yaw inertia `((b + L) / 2)^2 m R_z^2 / 4`, the
/// main- and nose-gear arms about the centre of gravity, and the ground-run
/// lift coefficient (Raymer p. 672, as the take-off field model). `None`
/// when the airplane has no fuselage or main wing, or the gear does not
/// straddle the centre of gravity.
pub(super) fn ground_run_geometry(
    inputs: &VmcInputs<'_>,
    turboprop: bool,
    ground_drag_coefficient: f64,
) -> Option<GroundRunGeometry> {
    let plane = inputs.plane;
    let fuselage = plane.fuselages.first()?;
    let frame = plane.mac_frame()?;
    let start_x_m = fuselage.xsecs.first()?.xyz_c[0];
    let end_x_m = fuselage.xsecs.last()?.xyz_c[0];
    let length_m = end_x_m - start_x_m;
    let width_m = fuselage
        .xsecs
        .iter()
        .map(|section| section.width)
        .fold(0.0_f64, f64::max);
    let config = inputs.config;
    // The same fallbacks the envelope resolves its stations with.
    let stations = config.landing_gear.resolved_station_positions(
        start_x_m + length_m * config.mass_model.nlg_x_fraction,
        frame.x_lemac_m + config.mass_model.mlg_x_fraction_mac * frame.chord_m,
        start_x_m,
        length_m,
    );
    let main_gear_arm_m = stations.x_mlg_m - inputs.cg_x_m;
    let nose_gear_arm_m = inputs.cg_x_m - stations.x_nlg_m;
    let span_m = plane.b_ref;
    let r_z = yaw_radius_of_gyration(inputs, turboprop, width_m);
    let reach_m = 0.5 * (span_m + length_m);
    let yaw_inertia_kg_m2 = reach_m * reach_m * inputs.takeoff_mass_kg * r_z * r_z / 4.0;
    let usable = [
        main_gear_arm_m,
        nose_gear_arm_m,
        yaw_inertia_kg_m2,
        length_m,
    ]
    .iter()
    .all(|v| v.is_finite() && *v > 0.0);
    usable.then_some(GroundRunGeometry {
        mass_kg: inputs.takeoff_mass_kg,
        yaw_inertia_kg_m2,
        main_gear_arm_m,
        nose_gear_arm_m,
        ground_lift_coefficient: GROUND_LIFT_COEFFICIENT,
        ground_drag_coefficient,
    })
}
