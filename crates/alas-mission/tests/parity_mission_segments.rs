// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-mission::segments` against SUAVE's segment iterate chain,
//! via `golden/mission/mission_segments.json`.
//!
//! **This is the fixture that can localize a fault, and it is why it exists
//! separately from the end-to-end one.** A mission solve reports what the
//! segments converged to; if a residual is formed wrongly, the solver simply
//! converges somewhere else and *every* column of the answer moves at once,
//! which says nothing about which step was wrong. Here the two unknowns are
//! held at a fixed, deliberately-unsolved vector and one pass of the chain is
//! run, so each update method's output is compared on its own line: a wrong
//! atmosphere, a wrong orientation, a wrong thrust and a wrong mass integral
//! are four different failures rather than one.
//!
//! Two tiers, `exact` + `linalg`. The discrete facts: the segment schedule,
//! the analysis settings, the engine's fixed component parameters, which
//! branch of the surrogate was built, are `exact`, because they are copied
//! constants and a port that assumed a different one would agree about
//! arithmetic while flying a different aeroplane.
//!
//! **The numeric tier is `linalg` and not the `iter` the row was planned at,
//! and that is a tightening.** `iter` describes a quantity two solvers reach
//! by different paths, where the tolerance belongs to the answer rather than
//! to the arithmetic. Nothing here is solved: the unknowns are *held*, and one
//! pass of the chain is one deterministic evaluation whose loosest step is a
//! bicubic spline. `linalg` is what that construction is, and it carries an
//! absolute floor, which `iter` does not, and this comparison needs one,
//! because a segment flown at constant speed has quantities that are
//! analytically zero. `alas-mission::solve` keeps `iter`, where it belongs.
//!
//! The unknowns are held *away* from the root on purpose. At the solution
//! every force is in balance and the residuals are near zero, which is the one
//! state in which two different force models agree; the generator refuses to
//! write a fixture whose residuals fall below 1e-3.

// This file is itself a test binary, so an unwrap or expect that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod support;

use alas_mission::segments::Segment;
use alas_testkit::{Comparison, Tier};
use support::segments::{fixture, Held};

/// The bound this row's numbers are held to.
///
/// One control point is a bicubic spline evaluation, a twenty-station engine
/// walk and a full drag buildup, over an atmosphere evaluated at an altitude
/// that came out of a pseudospectral quadrature. The spline is the loosest
/// step in that, and it is `linalg`'s. Measured, 925 of the 961 values agree
/// to better than 1e-15 relative and the worst of the rest by 6.5e-14,
/// five decades inside the tier, which is therefore what the construction
/// calls for rather than what the numbers need.
const TIER: Tier = Tier::Linalg;

/// Run one pass of the chain at the unknowns the fixture held.
fn iterate(held: &Held, analyses: &alas_mission::segments::MissionAnalyses) -> Segment {
    let mut segment = Segment::new(held.spec.to_spec(), Some((&held.initials).into()))
        .expect("the fixture's segment sets up");
    segment.throttle.copy_from_slice(&held.unknowns.throttle);
    segment
        .body_angle_rad
        .copy_from_slice(&held.unknowns.body_angle_rad);
    segment.iterate(analyses);
    segment
}

/// One pass of the chain, method by method.
#[test]
fn every_update_method_agrees_at_a_held_unknown_vector() {
    let fixture = fixture();
    let analyses = support::analyses(&fixture.inputs.vehicle, &fixture.inputs.surrogate_training);

    for held in &fixture.segments {
        let segment = iterate(held, &analyses);
        let actual = &segment.conditions;
        let expected = &held.conditions;
        let mut c = Comparison::new(format!("alas-mission::segments {}", held.tag), TIER);

        // The unknowns went in unchanged: if they had not, everything below
        // would be a comparison of two different flight conditions.
        c.slice("unknowns/throttle", &actual.throttle, &expected.throttle);
        c.slice(
            "unknowns/body_angle_rad",
            &column(&actual.body_inertial_rotations_rad, 1),
            &expected.body_angle_rad,
        );

        // initialize_conditions, update_differentials_altitude, initialize_time
        c.slice("initials/time_s", &actual.time_s, &expected.time_s);
        c.slice(
            "initials/position_x_m",
            &column(&actual.position_vector_m, 0),
            &expected.position_vector_x_m,
        );
        c.slice(
            "initials/position_y_m",
            &column(&actual.position_vector_m, 1),
            &expected.position_vector_y_m,
        );
        c.slice(
            "initials/position_z_m",
            &column(&actual.position_vector_m, 2),
            &expected.position_vector_z_m,
        );
        c.slice(
            "initials/aircraft_range_m",
            &actual.aircraft_range_m,
            &expected.aircraft_range_m,
        );
        c.slice(
            "initialize_conditions/velocity_x_m_s",
            &column(&actual.velocity_vector_m_s, 0),
            &expected.velocity_vector_x_m_s,
        );
        c.slice(
            "initialize_conditions/velocity_z_m_s",
            &column(&actual.velocity_vector_m_s, 2),
            &expected.velocity_vector_z_m_s,
        );

        // `update_acceleration` differentiates a velocity vector that is
        // constant along every segment this mission flies (constant
        // airspeed, constant rate) so its output is analytically zero, and
        // what both implementations produce is the rounding noise of a dense
        // 16x16 differentiation operator applied to a constant: about 1e-14
        // m/s^2, against the 5 m/s^2 the residual is built from. There is no
        // quantity here to hold the port to; comparing the noise would be
        // comparing summation order and nothing else. What is checked is that
        // both sides are zero to the operator's precision, and the residual
        // below, which is the only place the acceleration is consumed,
        // is compared in full. `docs/PORTING.md` records this.
        c.exact(
            "update_acceleration is zero to operator precision on both sides",
            &(negligible(&column(&actual.acceleration_vector_m_s2, 0))
                && negligible(&column(&actual.acceleration_vector_m_s2, 2))
                && negligible(&expected.acceleration_vector_x_m_s2)
                && negligible(&expected.acceleration_vector_z_m_s2)),
            &true,
        );

        let residuals = segment.pack_residuals();
        let points = residuals.len() / 2;

        // Every array the chain writes, in the order the chain writes it and
        // named for the method that wrote it, so a failure says which step
        // diverged rather than which column showed it.
        for (name, port, reference) in [
            // The unknowns went in unchanged: if they had not, everything
            // below would compare two different flight conditions.
            (
                "unknowns/throttle",
                actual.throttle.clone(),
                &expected.throttle,
            ),
            (
                "unknowns/body_angle_rad",
                column(&actual.body_inertial_rotations_rad, 1),
                &expected.body_angle_rad,
            ),
            // initialize_conditions, update_differentials_altitude, initialize_time
            ("initials/time_s", actual.time_s.clone(), &expected.time_s),
            (
                "initials/position_x_m",
                column(&actual.position_vector_m, 0),
                &expected.position_vector_x_m,
            ),
            (
                "initials/position_y_m",
                column(&actual.position_vector_m, 1),
                &expected.position_vector_y_m,
            ),
            (
                "initials/position_z_m",
                column(&actual.position_vector_m, 2),
                &expected.position_vector_z_m,
            ),
            (
                "initials/aircraft_range_m",
                actual.aircraft_range_m.clone(),
                &expected.aircraft_range_m,
            ),
            (
                "initialize_conditions/velocity_x_m_s",
                column(&actual.velocity_vector_m_s, 0),
                &expected.velocity_vector_x_m_s,
            ),
            (
                "initialize_conditions/velocity_z_m_s",
                column(&actual.velocity_vector_m_s, 2),
                &expected.velocity_vector_z_m_s,
            ),
            // update_altitude, update_atmosphere, update_gravity
            (
                "update_altitude/m",
                actual.altitude_m.clone(),
                &expected.altitude_m,
            ),
            (
                "update_atmosphere/pressure_pa",
                actual.pressure_pa.clone(),
                &expected.pressure_pa,
            ),
            (
                "update_atmosphere/temperature_k",
                actual.temperature_k.clone(),
                &expected.temperature_k,
            ),
            (
                "update_atmosphere/density_kg_m3",
                actual.density_kg_m3.clone(),
                &expected.density_kg_m3,
            ),
            (
                "update_atmosphere/speed_of_sound_m_s",
                actual.speed_of_sound_m_s.clone(),
                &expected.speed_of_sound_m_s,
            ),
            (
                "update_atmosphere/dynamic_viscosity_pa_s",
                actual.dynamic_viscosity_pa_s.clone(),
                &expected.dynamic_viscosity_pa_s,
            ),
            (
                "update_gravity/m_s2",
                actual.gravity_m_s2.clone(),
                &expected.gravity_m_s2,
            ),
            // update_freestream
            (
                "update_freestream/velocity_m_s",
                actual.velocity_m_s.clone(),
                &expected.velocity_m_s,
            ),
            (
                "update_freestream/mach",
                actual.mach.clone(),
                &expected.mach,
            ),
            (
                "update_freestream/reynolds_per_m",
                actual.reynolds_number_per_m.clone(),
                &expected.reynolds_number_per_m,
            ),
            (
                "update_freestream/dynamic_pressure_pa",
                actual.dynamic_pressure_pa.clone(),
                &expected.dynamic_pressure_pa,
            ),
            // update_orientations. The angle of attack falls out of the body
            // angle and the flight path rather than being given, and both
            // transforms are compared entry by entry because a wrong rotation
            // order produces a plausible force from a wrong tensor, which is
            // the fault this comparison actually caught.
            (
                "update_orientations/angle_of_attack_rad",
                actual.angle_of_attack_rad.clone(),
                &expected.angle_of_attack_rad,
            ),
            (
                "update_orientations/side_slip_angle_rad",
                actual.side_slip_angle_rad.clone(),
                &expected.side_slip_angle_rad,
            ),
            (
                "update_orientations/transform_body_to_inertial",
                flatten(&actual.transform_body_to_inertial),
                &flatten_rows(&expected.transform_body_to_inertial),
            ),
            (
                "update_orientations/transform_wind_to_inertial",
                flatten(&actual.transform_wind_to_inertial),
                &flatten_rows(&expected.transform_wind_to_inertial),
            ),
            // update_thrust
            (
                "update_thrust/thrust_force_x_n",
                column(&actual.thrust_force_vector_n, 0),
                &expected.thrust_force_x_n,
            ),
            (
                "update_thrust/vehicle_mass_rate_kg_s",
                actual.vehicle_mass_rate_kg_s.clone(),
                &expected.vehicle_mass_rate_kg_s,
            ),
            // update_aerodynamics, with the buildup broken out so a wrong
            // drag component prints as itself rather than inside the total.
            (
                "update_aerodynamics/lift_coefficient",
                actual.lift_coefficient.clone(),
                &expected.lift_coefficient,
            ),
            (
                "update_aerodynamics/drag_coefficient",
                actual.drag_coefficient.clone(),
                &expected.drag_coefficient,
            ),
            (
                "update_aerodynamics/lift_force_z_n",
                column(&actual.wind_lift_force_vector_n, 2),
                &expected.lift_force_z_n,
            ),
            (
                "update_aerodynamics/drag_force_x_n",
                column(&actual.wind_drag_force_vector_n, 0),
                &expected.drag_force_x_n,
            ),
            (
                "update_aerodynamics/drag_parasite",
                breakdown(&segment, |drag| drag.parasite_total),
                &expected.drag_parasite,
            ),
            (
                "update_aerodynamics/drag_induced",
                breakdown(&segment, |drag| drag.induced_total),
                &expected.drag_induced,
            ),
            (
                "update_aerodynamics/drag_compressible",
                breakdown(&segment, |drag| drag.compressible_total),
                &expected.drag_compressible,
            ),
            (
                "update_aerodynamics/drag_miscellaneous",
                breakdown(&segment, |drag| drag.miscellaneous_total),
                &expected.drag_miscellaneous,
            ),
            (
                "update_aerodynamics/drag_untrimmed",
                breakdown(&segment, |drag| drag.untrimmed),
                &expected.drag_untrimmed,
            ),
            // update_weights
            (
                "update_weights/total_mass_kg",
                actual.total_mass_kg.clone(),
                &expected.total_mass_kg,
            ),
            (
                "update_weights/gravity_force_z_n",
                column(&actual.gravity_force_vector_n, 2),
                &expected.gravity_force_z_n,
            ),
            // update_forces
            (
                "update_forces/total_force_x_n",
                column(&actual.total_force_vector_n, 0),
                &expected.total_force_x_n,
            ),
            (
                "update_forces/total_force_y_n",
                column(&actual.total_force_vector_n, 1),
                &expected.total_force_y_n,
            ),
            (
                "update_forces/total_force_z_n",
                column(&actual.total_force_vector_n, 2),
                &expected.total_force_z_n,
            ),
            // update_planet_position, carrying the unit mismatch the module
            // documents rather than a corrected version of it.
            (
                "update_planet_position/latitude",
                actual.latitude_deg.clone(),
                &expected.latitude_deg,
            ),
            (
                "update_planet_position/longitude",
                actual.longitude_deg.clone(),
                &expected.longitude_deg,
            ),
            // residual_total_forces: what the solver is handed.
            (
                "residual_total_forces/horizontal",
                residuals[..points].to_vec(),
                &expected.residual_horizontal,
            ),
            (
                "residual_total_forces/vertical",
                residuals[points..].to_vec(),
                &expected.residual_vertical,
            ),
        ] {
            c.slice(name, &port, reference);
        }

        c.finish();
    }
}
/// Whether an array is zero to within a differentiation operator's rounding.
///
/// The bound is nine orders below the smallest acceleration any residual in
/// this fixture is formed from, so it separates "the operator returned zero"
/// from "the trajectory actually accelerates" with a wide margin either way.
fn negligible(values: &[f64]) -> bool {
    values.iter().all(|value| value.abs() < 1e-9)
}

/// One column of a vector of fixed-size arrays.
fn column<const N: usize>(rows: &[[f64; N]], index: usize) -> Vec<f64> {
    rows.iter().map(|row| row[index]).collect()
}

/// The port's transforms, flattened row-major, one control point after another.
fn flatten(transforms: &[[[f64; 3]; 3]]) -> Vec<f64> {
    transforms
        .iter()
        .flat_map(|matrix| matrix.iter().flatten().copied())
        .collect()
}

/// The fixture's transforms, already flattened per control point.
fn flatten_rows(transforms: &[Vec<f64>]) -> Vec<f64> {
    transforms.iter().flatten().copied().collect()
}

/// One quantity out of the drag buildup at every control point.
fn breakdown(
    segment: &Segment,
    read: impl Fn(&alas_aero::drag_buildup::DragBreakdown) -> f64,
) -> Vec<f64> {
    segment.conditions.drag_breakdown.iter().map(read).collect()
}
