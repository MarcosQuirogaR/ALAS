// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-mission::solve` against a whole SUAVE mission, via
//! `golden/mission/mission.json`.
//!
//! Twelve segments, sixteen control points each, flown end to end: the default
//! AVE aircraft from Madrid to Nairobi at 6500 km. Nothing here is held fixed,
//! each segment's thirty-two unknowns are searched for by MINPACK exactly
//! as `converge_root` searches for them, and each segment starts from the mass,
//! time, position and ground range the one before it ended at. This is the
//! only test in the crate that exercises that chain, and the only one in which
//! a wrong answer can come from the *solver* rather than from the model.
//!
//! **What is compared is the flight, not only the fuel burn.** A mission that
//! agrees on block fuel while flying a different throttle schedule has not
//! agreed about anything; so the solved unknowns are compared per segment and
//! per point, before any derived column is. The columns after them are every
//! one `export_data.py` writes, which is what the rest of the program reads a
//! mission through.
//!
//! Two tiers. The discrete facts, which segments were flown, in what order,
//! how many points each carries, and that every one of them converged, are
//! `exact`. Everything numeric is `iter`, and here that tier is the right one
//! rather than a fallback: MINPACK stops on its trust region and not on its
//! residual, so two implementations that agree about every force can still
//! stop at throttles that differ in the last few digits, and the segment after
//! them starts from the mass that difference produced. The bound is stated at
//! [`TIER`] with the measured margin.

// This file is itself a test binary, so an unwrap or expect that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod support;

use alas_mission::solve::Mission;
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;
use support::{SurrogateTraining, Vehicle};

/// The bound the flown mission is held to.
///
/// Every quantity here is downstream of twelve trust-region searches, each
/// handing its endpoint to the next as a starting condition, so a difference
/// in one segment's stopping point is carried by every segment after it. The
/// `linalg`-tier evaluation underneath is not what sets this bound:
/// `alas-mission::segments` agrees to 6.5e-14 at a held unknown vector: the
/// solver is: the same forces stop the trust region at slightly different
/// points, four decades looser than the arithmetic that produced them.
///
/// Measured, the worst disagreement anywhere in the mission is 3.0e-9
/// relative, on the last descent segment's solved body angle, and the worst
/// in any *derived* column is 9.1e-10, on the second cruise leg's throttle and
/// the thrust and fuel flow that follow from it. Both are solved unknowns and
/// their consequences, which is what this tier exists to describe; the flow
/// state, the coefficients and the masses all sit at 1e-13 and below.
const TIER: Tier = Tier::Iter { relative: 1e-7 };

#[derive(Debug, Deserialize)]
struct Fixture {
    inputs: Inputs,
    summary: Summary,
    segments: Vec<SolvedSegment>,
}

#[derive(Debug, Deserialize)]
struct Inputs {
    vehicle: Vehicle,
    surrogate_training: SurrogateTraining,
}

#[derive(Debug, Deserialize)]
struct Summary {
    initial_mass_kg: f64,
    final_mass_kg: f64,
    fuel_burned_kg: f64,
    block_time_s: f64,
    n_segments: usize,
}

#[derive(Debug, Deserialize)]
struct SolvedSegment {
    tag: String,
    spec: support::Spec,
    converged: bool,
    throttle: Vec<f64>,
    body_angle_rad: Vec<f64>,
    points: Vec<Point>,
}

/// One control point, as `export_data.py` writes it.
#[derive(Debug, Deserialize)]
struct Point {
    altitude_m: f64,
    tas_m_s: f64,
    mach: f64,
    density_kg_m3: f64,
    range_m: f64,
    pitch_deg: f64,
    aoa_deg: f64,
    cl: f64,
    cd: f64,
    throttle: f64,
    lift_n: f64,
    drag_n: f64,
    thrust_n: f64,
    cd_parasite: f64,
    cd_induced: f64,
    cd_compressible: f64,
    cd_miscellaneous: f64,
    cd_total: f64,
    mass_kg: f64,
    mass_flow_rate_kg_s: f64,
}

/// Radians per degree, for the two columns the export reports in degrees.
const RADIANS_PER_DEGREE: f64 = std::f64::consts::PI / 180.0;

fn fixture() -> Fixture {
    alas_testkit::load("mission", "mission")
}

/// Fly the whole mission once. Twelve trust-region searches is not free, and
/// three tests want the same answer.
fn flown() -> (Fixture, alas_mission::solve::MissionResult) {
    let fixture = fixture();
    let analyses = support::analyses(&fixture.inputs.vehicle, &fixture.inputs.surrogate_training);
    let mission = Mission {
        schedule: fixture
            .segments
            .iter()
            .map(|segment| segment.spec.to_spec())
            .collect(),
    };
    let result = mission.evaluate(&analyses).expect("the mission flies");
    (fixture, result)
}

/// The mission that was flown is the mission that was recorded, and all of it
/// converged.
///
/// Checked first and on its own: a mission that dropped a segment, reordered
/// two, or quietly stopped converging would still produce a plausible fuel
/// burn, and every comparison below would be reporting on a different flight.
#[test]
fn the_same_twelve_segments_were_flown_and_every_one_converged() {
    let (fixture, result) = flown();

    let mut c = Comparison::new("alas-mission::solve schedule", Tier::Exact);
    c.exact(
        "segment count",
        &result.segments.len(),
        &fixture.summary.n_segments,
    );
    c.exact(
        "segment order",
        &result
            .segments
            .iter()
            .map(|segment| segment.spec.tag.as_str())
            .collect::<Vec<_>>(),
        &fixture
            .segments
            .iter()
            .map(|segment| segment.tag.as_str())
            .collect::<Vec<_>>(),
    );
    for (solved, recorded) in result.solutions.iter().zip(&fixture.segments) {
        c.exact(
            &format!("{} converged", recorded.tag),
            &solved.converged,
            &recorded.converged,
        );
    }
    for (segment, recorded) in result.segments.iter().zip(&fixture.segments) {
        c.exact(
            &format!("{} control points", recorded.tag),
            &segment.conditions.len(),
            &recorded.points.len(),
        );
    }
    c.finish();
}

/// The solved unknowns themselves, per segment and per control point.
///
/// This is the comparison that means the two implementations flew the *same*
/// aeroplane the *same* way rather than arriving at the same place. The
/// throttle is what sets the fuel flow this whole phase exists to integrate;
/// the body angle is what sets the angle of attack the lift comes from.
#[test]
fn every_segment_was_solved_to_the_same_throttle_and_body_angle() {
    let (fixture, result) = flown();
    let mut c = Comparison::new("alas-mission::solve unknowns", TIER);
    for (segment, recorded) in result.segments.iter().zip(&fixture.segments) {
        c.slice(
            &format!("{}/throttle", recorded.tag),
            &segment.throttle,
            &recorded.throttle,
        );
        c.slice(
            &format!("{}/body_angle_rad", recorded.tag),
            &segment.body_angle_rad,
            &recorded.body_angle_rad,
        );
    }
    c.finish();
}

/// Every column the export writes, at every control point of every segment.
#[test]
fn every_exported_column_agrees_at_every_control_point() {
    let (fixture, result) = flown();
    let mut c = Comparison::new("alas-mission::solve columns", TIER);

    for (segment, recorded) in result.segments.iter().zip(&fixture.segments) {
        let tag = &recorded.tag;
        let conditions = &segment.conditions;
        let expected = &recorded.points;

        c.slice(
            &format!("{tag}/altitude_m"),
            &conditions.altitude_m,
            &read(expected, |point| point.altitude_m),
        );
        c.slice(
            &format!("{tag}/tas_m_s"),
            &conditions.velocity_m_s,
            &read(expected, |point| point.tas_m_s),
        );
        c.slice(
            &format!("{tag}/mach"),
            &conditions.mach,
            &read(expected, |point| point.mach),
        );
        c.slice(
            &format!("{tag}/density_kg_m3"),
            &conditions.density_kg_m3,
            &read(expected, |point| point.density_kg_m3),
        );
        // The ground range is the one column produced *after* convergence,
        // by the finalize pass, so it is also the check that finalize ran.
        c.slice(
            &format!("{tag}/range_m"),
            &conditions.aircraft_range_m,
            &read(expected, |point| point.range_m),
        );
        c.slice(
            &format!("{tag}/pitch_deg"),
            &conditions
                .body_inertial_rotations_rad
                .iter()
                .map(|rotations| rotations[1] / RADIANS_PER_DEGREE)
                .collect::<Vec<_>>(),
            &read(expected, |point| point.pitch_deg),
        );
        c.slice(
            &format!("{tag}/aoa_deg"),
            &conditions
                .angle_of_attack_rad
                .iter()
                .map(|angle| angle / RADIANS_PER_DEGREE)
                .collect::<Vec<_>>(),
            &read(expected, |point| point.aoa_deg),
        );
        c.slice(
            &format!("{tag}/cl"),
            &conditions.lift_coefficient,
            &read(expected, |point| point.cl),
        );
        c.slice(
            &format!("{tag}/cd"),
            &conditions.drag_coefficient,
            &read(expected, |point| point.cd),
        );
        c.slice(
            &format!("{tag}/throttle"),
            &conditions.throttle,
            &read(expected, |point| point.throttle),
        );
        // The export reports lift and drag as positive magnitudes; the wind
        // frame carries them along `-z` and `-x`.
        c.slice(
            &format!("{tag}/lift_n"),
            &conditions
                .wind_lift_force_vector_n
                .iter()
                .map(|force| -force[2])
                .collect::<Vec<_>>(),
            &read(expected, |point| point.lift_n),
        );
        c.slice(
            &format!("{tag}/drag_n"),
            &conditions
                .wind_drag_force_vector_n
                .iter()
                .map(|force| -force[0])
                .collect::<Vec<_>>(),
            &read(expected, |point| point.drag_n),
        );
        c.slice(
            &format!("{tag}/thrust_n"),
            &conditions
                .thrust_force_vector_n
                .iter()
                .map(|force| force[0])
                .collect::<Vec<_>>(),
            &read(expected, |point| point.thrust_n),
        );
        // The drag buildup, broken out so a wrong component prints as itself.
        c.slice(
            &format!("{tag}/cd_parasite"),
            &drag(segment, |breakdown| breakdown.parasite_total),
            &read(expected, |point| point.cd_parasite),
        );
        c.slice(
            &format!("{tag}/cd_induced"),
            &drag(segment, |breakdown| breakdown.induced_total),
            &read(expected, |point| point.cd_induced),
        );
        c.slice(
            &format!("{tag}/cd_compressible"),
            &drag(segment, |breakdown| breakdown.compressible_total),
            &read(expected, |point| point.cd_compressible),
        );
        c.slice(
            &format!("{tag}/cd_miscellaneous"),
            &drag(segment, |breakdown| breakdown.miscellaneous_total),
            &read(expected, |point| point.cd_miscellaneous),
        );
        c.slice(
            &format!("{tag}/cd_total"),
            &drag(segment, |breakdown| breakdown.total),
            &read(expected, |point| point.cd_total),
        );
        c.slice(
            &format!("{tag}/mass_kg"),
            &conditions.total_mass_kg,
            &read(expected, |point| point.mass_kg),
        );
        c.slice(
            &format!("{tag}/mass_flow_rate_kg_s"),
            &conditions.vehicle_mass_rate_kg_s,
            &read(expected, |point| point.mass_flow_rate_kg_s),
        );
    }
    c.finish();
}

/// The four numbers the rest of the program reads a mission through.
#[test]
fn the_mission_summary_agrees() {
    let (fixture, result) = flown();
    let mut c = Comparison::new("alas-mission::solve summary", TIER);
    c.scalar(
        "initial_mass_kg",
        result.initial_mass_kg(),
        fixture.summary.initial_mass_kg,
    );
    c.scalar(
        "final_mass_kg",
        result.final_mass_kg(),
        fixture.summary.final_mass_kg,
    );
    c.scalar(
        "fuel_burned_kg",
        result.fuel_burned_kg(),
        fixture.summary.fuel_burned_kg,
    );
    c.scalar(
        "block_time_s",
        result.block_time_s(),
        fixture.summary.block_time_s,
    );
    c.finish();
}

#[test]
fn mass_and_range_are_conserved_across_segment_boundaries() {
    let (_fixture, result) = flown();
    assert!(result.fuel_burned_kg() > 0.0);
    assert!(result.block_time_s() > 0.0);

    for pair in result.segments.windows(2) {
        let previous_end = pair[0]
            .conditions
            .total_mass_kg
            .last()
            .copied()
            .expect("a solved segment has a final mass");
        let next_start = pair[1]
            .conditions
            .total_mass_kg
            .first()
            .copied()
            .expect("a solved segment has an initial mass");
        assert!((previous_end - next_start).abs() < 1e-6);
    }

    for segment in &result.segments {
        let ranges = &segment.conditions.aircraft_range_m;
        assert!(ranges.windows(2).all(|pair| pair[1] >= pair[0]));
    }
}

/// One field out of every recorded point.
fn read(points: &[Point], field: impl Fn(&Point) -> f64) -> Vec<f64> {
    points.iter().map(field).collect()
}

/// One field out of the drag buildup at every control point.
fn drag(
    segment: &alas_mission::segments::Segment,
    field: impl Fn(&alas_aero::drag_buildup::DragBreakdown) -> f64,
) -> Vec<f64> {
    segment
        .conditions
        .drag_breakdown
        .iter()
        .map(field)
        .collect()
}
