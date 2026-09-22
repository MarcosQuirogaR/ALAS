// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission analysis model/Methods/Missions/Segments/Common/Frames.py and
// mission analysis model/Methods/Geometry/Three_Dimensional/{angles_to_dcms,orientation_product,
// orientation_transpose}.py.
// Upstream: mission analysis model 2.5.2, LGPL-2.1 (relicensed under GPL-2.0-or-later per
// LGPL-2.1 section 3; compatible with this program's AGPL-3.0-or-later).
// Reference: alas @ rust-port-baseline.

//! The reference frames a segment carries, and the moves between them.
//!
//! Four of these run once per iteration before anything else
//! ([`initialize_time`], [`initialize_weights`] in [`super::common`],
//! [`initialize_inertial_position`], [`initialize_planet_position`]) and
//! reconcile the segment with the one flown before it. The rest turn the
//! solver's two unknowns into forces: [`update_orientations`] builds the body
//! and wind transforms and reads the angle of attack back out of them, and
//! [`update_forces`] rotates lift, drag and thrust into the inertial frame and
//! adds the weight.
//!
//! The rotation machinery is translated rather than simplified. Every
//! rotation this mission actually performs is about the pitch axis alone:
//! roll and yaw are identically zero, so `T0` and `T2` are the identity and
//! the products collapse, but writing the collapsed form would make the
//! sequence a comment instead of code, and the sequence (`T0 * T1 * T2`, from
//! `angles_to_dcms(rotations, (2, 1, 0))` walking its sequence backwards) is
//! exactly the sort of thing a port gets subtly wrong.

use super::conditions::{Conditions, Initials, Matrix3, Vector3};

/// Radians per degree: mission analysis model's `Units.deg`, used here only where upstream
/// divides an angle by it.
const RADIANS_PER_DEGREE: f64 = std::f64::consts::PI / 180.0;

/// Earth's mean radius, m. `Attributes.Planets.Earth.mean_radius`.
pub const EARTH_MEAN_RADIUS_M: f64 = 6.371e6;

/// Standard sea-level gravity, m/s^2. `Attributes.Planets.Earth.sea_level_gravity`.
pub const SEA_LEVEL_GRAVITY_M_S2: f64 = 9.806_65;

/// `Earth.compute_gravity`: `g0 * (Re / (Re + H))^2`.
pub fn compute_gravity(altitude_m: f64) -> f64 {
    SEA_LEVEL_GRAVITY_M_S2 * (EARTH_MEAN_RADIUS_M / (EARTH_MEAN_RADIUS_M + altitude_m)).powi(2)
}

/// `T0`: rotation about the first axis.
fn t0(angle_rad: f64) -> Matrix3 {
    let (sin, cos) = angle_rad.sin_cos();
    [[1.0, 0.0, 0.0], [0.0, cos, sin], [0.0, -sin, cos]]
}

/// `T1`: rotation about the second axis.
fn t1(angle_rad: f64) -> Matrix3 {
    let (sin, cos) = angle_rad.sin_cos();
    [[cos, 0.0, -sin], [0.0, 1.0, 0.0], [sin, 0.0, cos]]
}

/// `T2`: rotation about the third axis.
fn t2(angle_rad: f64) -> Matrix3 {
    let (sin, cos) = angle_rad.sin_cos();
    [[cos, sin, 0.0], [-sin, cos, 0.0], [0.0, 0.0, 1.0]]
}

/// `orientation_product` for two tensors: an ordinary matrix product.
fn matrix_product(left: &Matrix3, right: &Matrix3) -> Matrix3 {
    let mut out = [[0.0; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (k, value) in row.iter_mut().enumerate() {
            *value = (0..3).map(|j| left[i][j] * right[j][k]).sum();
        }
    }
    out
}

/// `orientation_product` for a tensor and a vector.
fn matrix_vector(matrix: &Matrix3, vector: &Vector3) -> Vector3 {
    let mut out = [0.0; 3];
    for (i, value) in out.iter_mut().enumerate() {
        *value = (0..3).map(|j| matrix[i][j] * vector[j]).sum();
    }
    out
}

/// `orientation_transpose`.
fn transpose(matrix: &Matrix3) -> Matrix3 {
    let mut out = [[0.0; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, value) in row.iter_mut().enumerate() {
            *value = matrix[j][i];
        }
    }
    out
}

/// `angles_to_dcms(rotations, (2, 1, 0))`.
///
/// The sequence is walked in reverse and each factor multiplied on the right
/// of the running product, which starts at the identity, so the result is
/// `T0(r0) * T1(r1) * T2(r2)`.
fn angles_to_dcm(rotations: &Vector3) -> Matrix3 {
    let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let mut transform = matrix_product(&identity, &t0(rotations[0]));
    transform = matrix_product(&transform, &t1(rotations[1]));
    matrix_product(&transform, &t2(rotations[2]))
}

/// Shift the whole time array so it begins where the previous segment ended.
///
/// Upstream also copies the planet's `start_time` across; nothing in the
/// reached chain reads it, and `mission_setup` sets no `start_time` on any
/// segment, so there is no value to carry.
pub fn initialize_time(conditions: &mut Conditions, initials: Option<&Initials>) {
    let Some(initials) = initials else {
        return;
    };
    let Some(&first) = conditions.time_s.first() else {
        return;
    };
    let offset = initials.time_s - first;
    for time in &mut conditions.time_s {
        *time += offset;
    }
}

/// Shift position and ground range so they begin where the previous segment
/// ended.
///
/// Upstream first tries to overwrite the *previous* segment's last altitude
/// with this segment's own, which is a real mutation of a shared object, but
/// it is guarded on `segment.altitude` or `segment.altitude_start` being set,
/// and every segment in this mission that has a predecessor leaves both at
/// `None`. Its `else` branch is `assert('Altitude not set')`, which asserts a
/// non-empty string and therefore never fires. So on this program's inputs the
/// method is exactly the shift below, and the guarded mutation is recorded
/// here rather than translated into a branch nothing can take.
pub fn initialize_inertial_position(conditions: &mut Conditions, initials: Option<&Initials>) {
    let Some(initials) = initials else {
        return;
    };
    let Some(&first_position) = conditions.position_vector_m.first() else {
        return;
    };
    let first_range = conditions.aircraft_range_m[0];

    let offset = [
        initials.position_vector_m[0] - first_position[0],
        initials.position_vector_m[1] - first_position[1],
        initials.position_vector_m[2] - first_position[2],
    ];
    for position in &mut conditions.position_vector_m {
        for (component, shift) in position.iter_mut().zip(offset) {
            *component += shift;
        }
    }

    let range_offset = initials.aircraft_range_m - first_range;
    for range in &mut conditions.aircraft_range_m {
        *range += range_offset;
    }
}

/// Set every latitude and longitude to the previous segment's last value.
///
/// Not a shift: upstream assigns the scalar across the whole column, which is
/// what makes [`update_planet_position`]'s reading of row zero well defined.
/// With no previous segment and no `latitude` on the segment, which is every
/// case `mission_setup` builds, the initial position is the equator on the
/// prime meridian.
pub fn initialize_planet_position(conditions: &mut Conditions, initials: Option<&Initials>) {
    let (latitude, longitude) = match initials {
        Some(initials) => (initials.latitude_deg, initials.longitude_deg),
        None => (0.0, 0.0),
    };
    conditions.latitude_deg.fill(latitude);
    conditions.longitude_deg.fill(longitude);
}

/// Differentiate the inertial velocity to get the inertial acceleration.
///
/// Runs in a climb or descent segment and *not* in a cruise segment, whose
/// iterate chain omits it, so a cruise acceleration stays at the zero
/// `expand_rows` left it at, and its residual is formed without one.
pub fn update_acceleration(conditions: &mut Conditions, differentiate: &[Vec<f64>]) {
    let velocity = conditions.velocity_vector_m_s.clone();
    for (row, acceleration) in conditions.acceleration_vector_m_s2.iter_mut().enumerate() {
        for (axis, value) in acceleration.iter_mut().enumerate() {
            *value = differentiate[row]
                .iter()
                .zip(&velocity)
                .map(|(&weight, v)| weight * v[axis])
                .sum();
        }
    }
}

/// Build the body and wind transforms, and read the flow angles out of them.
///
/// The angle of attack is *not* an input here: it falls out of the body pitch
/// (an unknown) and the inertial velocity (fixed by the segment's speed and
/// climb rate) as the angle between the flight path and the body axis.
pub fn update_orientations(conditions: &mut Conditions) {
    for point in 0..conditions.len() {
        let rotations = conditions.body_inertial_rotations_rad[point];
        let velocity = conditions.velocity_vector_m_s[point];

        let inertial_to_body = angles_to_dcm(&rotations);
        let body_to_inertial = transpose(&inertial_to_body);
        let body_velocity = matrix_vector(&inertial_to_body, &velocity);

        // The inertial velocity projected into the body x-z plane.
        let stability_velocity = [body_velocity[0], 0.0, body_velocity[2]];
        let stability_magnitude = (stability_velocity
            .iter()
            .map(|value| value * value)
            .sum::<f64>())
        .sqrt();

        let alpha = stability_velocity[2].atan2(stability_velocity[0]);
        let beta = body_velocity[1].atan2(stability_magnitude);

        conditions.angle_of_attack_rad[point] = alpha;
        conditions.side_slip_angle_rad[point] = -beta;
        conditions.roll_angle_rad[point] = rotations[0];
        conditions.transform_body_to_inertial[point] = body_to_inertial;

        // Upstream builds `T_wind2body`, transposes it into `T_body2wind`,
        // and then forms the wind-to-inertial transform from `T_wind2body`,
        // so the transpose is computed and never read, the same dead
        // arithmetic `alas-struct::mesh` and `alas-aero::vorlax` each dropped
        // in their own rows. It is dropped here too, and the product is the
        // one upstream actually takes.
        //
        // Worth stating because the difference is invisible in the numbers
        // until it reaches a force: with the transpose, the wind frame is
        // rotated by `theta + alpha` instead of `theta - alpha`, which on a
        // four-degree climb is a tenth of a degree. Every coordinate, angle
        // and coefficient still agrees; only the resolved forces move.
        let wind_body_rotations = [0.0, alpha, beta];
        let wind_to_body = angles_to_dcm(&wind_body_rotations);
        conditions.transform_wind_to_inertial[point] =
            matrix_product(&wind_to_body, &body_to_inertial);
    }
}

/// Sum lift, drag, thrust and weight in the inertial frame.
pub fn update_forces(conditions: &mut Conditions) {
    for point in 0..conditions.len() {
        let wind_to_inertial = conditions.transform_wind_to_inertial[point];
        let body_to_inertial = conditions.transform_body_to_inertial[point];

        let lift = matrix_vector(
            &wind_to_inertial,
            &conditions.wind_lift_force_vector_n[point],
        );
        let drag = matrix_vector(
            &wind_to_inertial,
            &conditions.wind_drag_force_vector_n[point],
        );
        let thrust = matrix_vector(&body_to_inertial, &conditions.thrust_force_vector_n[point]);
        let weight = conditions.gravity_force_vector_n[point];

        for axis in 0..3 {
            conditions.total_force_vector_n[point][axis] =
                lift[axis] + drag[axis] + thrust[axis] + weight[axis];
        }
    }
}

/// Integrate the ground track and the latitude/longitude along it.
///
/// Two upstream quirks are reproduced rather than corrected, both
/// `deviation-candidate`s and neither observable in what this mission
/// reports. The integrated latitude is divided by the degree factor and then
/// *its cosine is taken*, so a degree-valued number is fed to a trigonometric
/// function expecting radians; and the result is added to a latitude that was
/// initialized in radians. Nothing downstream reads either column (no force,
/// no residual and no exported quantity depends on them) which is why the
/// mismatch has survived.
pub fn update_planet_position(
    conditions: &mut Conditions,
    integrate: &[Vec<f64>],
    true_course_rad: f64,
) {
    let points = conditions.len();
    let mut latitude_rate = vec![0.0; points];
    for (point, rate) in latitude_rate.iter_mut().enumerate() {
        let flight_path = conditions.body_inertial_rotations_rad[point][1]
            - conditions.angle_of_attack_rad[point];
        let radius = conditions.altitude_m[point] + EARTH_MEAN_RADIUS_M;
        *rate =
            (conditions.velocity_m_s[point] / radius) * flight_path.cos() * true_course_rad.cos();
    }

    let latitude: Vec<f64> = (0..points)
        .map(|row| {
            integrate[row]
                .iter()
                .zip(&latitude_rate)
                .map(|(&weight, &rate)| weight * rate)
                .sum::<f64>()
                / RADIANS_PER_DEGREE
        })
        .collect();

    let mut longitude_rate = vec![0.0; points];
    for (point, rate) in longitude_rate.iter_mut().enumerate() {
        let flight_path = conditions.body_inertial_rotations_rad[point][1]
            - conditions.angle_of_attack_rad[point];
        let radius = conditions.altitude_m[point] + EARTH_MEAN_RADIUS_M;
        *rate =
            (conditions.velocity_m_s[point] / radius) * flight_path.cos() * true_course_rad.sin()
                / latitude[point].cos();
    }

    let longitude: Vec<f64> = (0..points)
        .map(|row| {
            integrate[row]
                .iter()
                .zip(&longitude_rate)
                .map(|(&weight, &rate)| weight * rate)
                .sum::<f64>()
                / RADIANS_PER_DEGREE
        })
        .collect();

    let latitude_start = conditions.latitude_deg[0];
    let longitude_start = conditions.longitude_deg[0];
    for (point, (&north, &east)) in latitude.iter().zip(&longitude).enumerate() {
        conditions.latitude_deg[point] = latitude_start + north;
        conditions.longitude_deg[point] = longitude_start + east;
    }
}

/// Integrate the horizontal velocity into a ground track.
///
/// Runs once, after the segment has converged. Upstream integrates only the
/// `x` component and then copies it into the `y` slot before projecting both
/// onto the course heading, so the two horizontal components share one
/// integral; the ground range takes the `x` integral alone, unprojected.
pub fn integrate_inertial_horizontal_position(
    conditions: &mut Conditions,
    integrate: &[Vec<f64>],
    true_course_rad: f64,
) {
    let points = conditions.len();
    if points == 0 {
        return;
    }
    let start_x = conditions.position_vector_m[0][0];
    let start_y = conditions.position_vector_m[0][1];
    let start_range = conditions.aircraft_range_m[0];
    let velocity_x: Vec<f64> = conditions
        .velocity_vector_m_s
        .iter()
        .map(|velocity| velocity[0])
        .collect();

    let distance: Vec<f64> = (0..points)
        .map(|row| {
            integrate[row]
                .iter()
                .zip(&velocity_x)
                .map(|(&weight, &v)| weight * v)
                .sum()
        })
        .collect();

    let (sin_course, cos_course) = true_course_rad.sin_cos();
    for (point, &flown) in distance.iter().enumerate() {
        conditions.position_vector_m[point][0] = start_x + flown * cos_course;
        conditions.position_vector_m[point][1] = start_y + flown * sin_course;
        conditions.aircraft_range_m[point] = start_range + flown;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-12,
            "{actual} is not {expected}"
        );
    }

    // The three elementary rotations and their sequence are the part of this
    // module a port gets wrong silently, so they are pinned against a case
    // whose answer is known by inspection rather than only through parity.
    #[test]
    fn a_pure_pitch_rotation_tilts_the_body_x_axis_by_the_pitch_angle() {
        let pitch = 0.25;
        let dcm = angles_to_dcm(&[0.0, pitch, 0.0]);
        // `angles_to_dcms` builds the inertial-to-body transform, so the body
        // x axis is its first *row* read back through the transpose.
        let body_to_inertial = transpose(&dcm);
        approx(body_to_inertial[0][0], pitch.cos());
        approx(body_to_inertial[2][0], -pitch.sin());
    }

    #[test]
    fn the_rotation_sequence_is_roll_then_pitch_then_yaw() {
        let rotations = [0.1, 0.2, 0.3];
        let expected = matrix_product(
            &matrix_product(&t0(rotations[0]), &t1(rotations[1])),
            &t2(rotations[2]),
        );
        assert_eq!(angles_to_dcm(&rotations), expected);
    }

    // A level flight path with the nose up by theta must read back an angle
    // of attack of exactly theta, which is the property `update_orientations`
    // exists to provide and the one the residual is built on.
    #[test]
    fn level_flight_reads_the_angle_of_attack_back_as_the_body_angle() {
        let mut conditions = Conditions::expanded(1);
        conditions.velocity_vector_m_s[0] = [200.0, 0.0, 0.0];
        conditions.body_inertial_rotations_rad[0] = [0.0, 0.05, 0.0];
        update_orientations(&mut conditions);
        approx(conditions.angle_of_attack_rad[0], 0.05);
        approx(conditions.side_slip_angle_rad[0], 0.0);
    }

    // Climbing at the same body angle as the flight path angle is zero-alpha
    // flight: the check that the sign convention (z down) survived.
    #[test]
    fn a_climb_at_the_flight_path_angle_has_no_angle_of_attack() {
        let mut conditions = Conditions::expanded(1);
        let climb_angle: f64 = 0.08;
        conditions.velocity_vector_m_s[0] =
            [200.0 * climb_angle.cos(), 0.0, -200.0 * climb_angle.sin()];
        conditions.body_inertial_rotations_rad[0] = [0.0, climb_angle, 0.0];
        update_orientations(&mut conditions);
        approx(conditions.angle_of_attack_rad[0], 0.0);
    }

    #[test]
    fn initializing_time_shifts_the_array_onto_the_previous_segment() {
        let mut conditions = Conditions::expanded(3);
        conditions.time_s = vec![0.0, 10.0, 20.0];
        let initials = Initials {
            time_s: 500.0,
            ..Initials::default()
        };
        initialize_time(&mut conditions, Some(&initials));
        assert_eq!(conditions.time_s, vec![500.0, 510.0, 520.0]);
        // Shifting twice is a no-op, which is what makes it safe to run at
        // the head of every iteration.
        initialize_time(&mut conditions, Some(&initials));
        assert_eq!(conditions.time_s, vec![500.0, 510.0, 520.0]);
    }

    #[test]
    fn without_a_predecessor_the_planet_position_is_the_origin() {
        let mut conditions = Conditions::expanded(2);
        conditions.latitude_deg = vec![7.0, 9.0];
        initialize_planet_position(&mut conditions, None);
        assert_eq!(conditions.latitude_deg, vec![0.0, 0.0]);
        assert_eq!(conditions.longitude_deg, vec![0.0, 0.0]);
    }

    #[test]
    fn gravity_falls_off_with_altitude_from_the_sea_level_value() {
        approx(compute_gravity(0.0), SEA_LEVEL_GRAVITY_M_S2);
        assert!(compute_gravity(11_000.0) < SEA_LEVEL_GRAVITY_M_S2);
    }
}
