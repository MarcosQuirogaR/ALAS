// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission analysis model/Methods/Missions/Segments/Common/{Aerodynamics,Weights,
// Energy}.py.
// Upstream: mission analysis model 2.5.2, LGPL-2.1 (relicensed under GPL-2.0-or-later per
// LGPL-2.1 section 3; compatible with this program's AGPL-3.0-or-later).
// Reference: alas @ rust-port-baseline.

//! The rest of one iteration: the flow state, the forces the analyses report,
//! and the mass that burns away underneath them.
//!
//! Read in the order the process chain runs them, these say what a mission
//! segment *is*. Altitude comes out of the position vector; the atmosphere
//! comes out of the altitude; the freestream comes out of the atmosphere and
//! the velocity; the two analyses turn that plus the two unknowns into a
//! thrust and a drag polar; and the mass falls by the integral of the fuel
//! flow. What is left over -- the force that does not balance the
//! acceleration -- is the residual the solver drives to zero.

use alas_atmo::Us1976Values;

use super::analyses::MissionAnalyses;
use super::conditions::{Conditions, Initials};
use super::frames::compute_gravity;

/// Altitude is the negated `z` of the inertial position, nothing more.
pub fn update_altitude(conditions: &mut Conditions) {
    for point in 0..conditions.len() {
        conditions.altitude_m[point] = -conditions.position_vector_m[point][2];
    }
}

/// Evaluate the atmosphere at every control point.
///
/// Returns the raw values alongside packing them, because
/// [`update_thrust`] needs the same object rather than a second evaluation:
/// the turbofan's freestream is built from it, and rebuilding it from the
/// altitude would be re-deriving an input the segment already holds.
pub fn update_atmosphere(
    conditions: &mut Conditions,
    analyses: &MissionAnalyses,
    temperature_deviation_k: f64,
) -> Vec<Us1976Values> {
    let values: Vec<Us1976Values> = conditions
        .altitude_m
        .iter()
        .map(|&altitude| analyses.atmosphere(altitude, temperature_deviation_k))
        .collect();

    for (point, atmosphere) in values.iter().enumerate() {
        conditions.pressure_pa[point] = atmosphere.pressure_pa;
        conditions.temperature_k[point] = atmosphere.temperature_k;
        conditions.density_kg_m3[point] = atmosphere.density_kg_m3;
        conditions.speed_of_sound_m_s[point] = atmosphere.speed_of_sound_m_s;
        conditions.dynamic_viscosity_pa_s[point] = atmosphere.dynamic_viscosity_pa_s;
    }
    values
}

/// Gravity at each control point's altitude.
pub fn update_gravity(conditions: &mut Conditions) {
    for point in 0..conditions.len() {
        conditions.gravity_m_s2[point] = compute_gravity(conditions.altitude_m[point]);
    }
}

/// Speed, Mach, Reynolds number and dynamic pressure from the velocity vector.
///
/// The speed is formed as the square root of the summed squares of all three
/// components and the dynamic pressure from the *squared* magnitude before the
/// root, which is how upstream writes it -- `q` is `0.5 * rho * Vmag2`, not
/// `0.5 * rho * Vmag * Vmag`.
pub fn update_freestream(conditions: &mut Conditions) {
    for point in 0..conditions.len() {
        let velocity = conditions.velocity_vector_m_s[point];
        let magnitude_squared: f64 = velocity.iter().map(|value| value * value).sum();
        let magnitude = magnitude_squared.sqrt();

        conditions.velocity_m_s[point] = magnitude;
        conditions.dynamic_pressure_pa[point] =
            0.5 * conditions.density_kg_m3[point] * magnitude_squared;
        conditions.mach[point] = magnitude / conditions.speed_of_sound_m_s[point];
        conditions.reynolds_number_per_m[point] =
            conditions.density_kg_m3[point] * magnitude / conditions.dynamic_viscosity_pa_s[point];
    }
}

/// Run the energy network and pack the thrust force and the fuel flow.
///
/// The `vehicle_additional_fuel_rate` branch is unreachable: a `Turbofan`
/// network's results carry no such key, so `has_additional_fuel` stays false
/// and [`update_weights`] takes its plain branch. Recorded rather than
/// translated as a branch nothing can enter.
pub fn update_thrust(
    conditions: &mut Conditions,
    analyses: &MissionAnalyses,
    atmosphere: &[Us1976Values],
) {
    let mut thrusts = Vec::with_capacity(conditions.len());
    for (point, values) in atmosphere.iter().enumerate() {
        let output = analyses.thrust(
            values,
            conditions.altitude_m[point],
            conditions.velocity_m_s[point],
            conditions.mach[point],
            conditions.gravity_m_s2[point],
            conditions.throttle[point],
        );
        conditions.thrust_force_vector_n[point] = [output.thrust_n, 0.0, 0.0];
        conditions.vehicle_mass_rate_kg_s[point] = output.fuel_flow_rate_kg_s;
        thrusts.push(output);
    }
    conditions.thrust = thrusts;
}

/// Run the aerodynamics analysis and dimensionalize its two coefficients.
///
/// The lift and drag coefficients are zeroed wherever the dynamic pressure is
/// not positive, and clamped to `+/- maximum_lift_coefficient`. Both are
/// translated; neither fires on this program's inputs, because a segment
/// always has airspeed and the clamp is `np.inf`.
pub fn update_aerodynamics(conditions: &mut Conditions, analyses: &MissionAnalyses) {
    let mut breakdowns = Vec::with_capacity(conditions.len());
    for point in 0..conditions.len() {
        let solution = analyses.aerodynamics(
            conditions.angle_of_attack_rad[point],
            conditions.mach[point],
            conditions.temperature_k[point],
            conditions.reynolds_number_per_m[point],
        );

        let dynamic_pressure = conditions.dynamic_pressure_pa[point];
        let mut lift_coefficient = solution.lift_coefficient;
        let mut drag_coefficient = solution.drag.total;
        if dynamic_pressure <= 0.0 {
            lift_coefficient = 0.0;
            drag_coefficient = 0.0;
        }
        if let Some(maximum) = analyses.maximum_lift_coefficient {
            lift_coefficient = lift_coefficient.clamp(-maximum, maximum);
        }

        let reference_area = analyses.reference_area_m2;
        conditions.lift_coefficient[point] = lift_coefficient;
        conditions.drag_coefficient[point] = drag_coefficient;
        conditions.wind_lift_force_vector_n[point] = [
            0.0,
            0.0,
            -lift_coefficient * dynamic_pressure * reference_area,
        ];
        conditions.wind_drag_force_vector_n[point] = [
            -drag_coefficient * dynamic_pressure * reference_area,
            0.0,
            0.0,
        ];
        conditions.wing_lift_coefficient[point] = solution.wing_lift_coefficient;
        conditions.wing_induced_drag_coefficient[point] = solution.wing_induced_drag_coefficient;
        conditions.surrogate_domain[point] = solution.surrogate_domain;
        breakdowns.push(solution.drag);
    }
    conditions.drag_breakdown = breakdowns;
}

/// Shift the mass array so it begins where the previous segment ended.
///
/// With no predecessor the segment starts at `takeoff_mass_kg`, which is the
/// one number the weights analysis is reached for from inside a segment --
/// taken as that number rather than as the analysis, the same "take the
/// fields you read" scoping [`crate::numerics`] already uses for the segment
/// state it hangs off.
pub fn initialize_weights(
    conditions: &mut Conditions,
    takeoff_mass_kg: f64,
    initials: Option<&Initials>,
) {
    let initial_mass = match initials {
        Some(initials) => initials.total_mass_kg,
        None => takeoff_mass_kg,
    };
    let Some(&first) = conditions.total_mass_kg.first() else {
        return;
    };
    let offset = initial_mass - first;
    for mass in &mut conditions.total_mass_kg {
        *mass += offset;
    }
}

/// Integrate the fuel flow into the mass, and turn the mass into a weight.
///
/// Row zero of the mass is deliberately left alone -- upstream writes
/// `total_mass[1:, 0]`, because row zero is what
/// [`initialize_weights`] pinned to the previous segment's final mass and the
/// integral is defined relative to it. The weight, by contrast, is formed from
/// the *integrated* array at every row including the first.
pub fn update_weights(conditions: &mut Conditions, integrate: &[Vec<f64>]) {
    let points = conditions.len();
    if points == 0 {
        return;
    }
    let initial_mass = conditions.total_mass_kg[0];
    let mass: Vec<f64> = (0..points)
        .map(|row| {
            initial_mass
                - integrate[row]
                    .iter()
                    .zip(&conditions.vehicle_mass_rate_kg_s)
                    .map(|(&weight, &rate)| weight * rate)
                    .sum::<f64>()
        })
        .collect();

    conditions.total_mass_kg[1..points].copy_from_slice(&mass[1..points]);
    for (point, &integrated) in mass.iter().enumerate() {
        conditions.gravity_force_vector_n[point][2] = integrated * conditions.gravity_m_s2[point];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn altitude_is_the_negated_down_coordinate() {
        let mut conditions = Conditions::expanded(2);
        conditions.position_vector_m[0] = [100.0, 0.0, -3000.0];
        conditions.position_vector_m[1] = [200.0, 0.0, -3500.0];
        update_altitude(&mut conditions);
        assert_eq!(conditions.altitude_m, vec![3000.0, 3500.0]);
    }

    #[test]
    fn the_freestream_speed_is_the_magnitude_of_the_whole_velocity_vector() {
        let mut conditions = Conditions::expanded(1);
        conditions.velocity_vector_m_s[0] = [3.0, 0.0, -4.0];
        conditions.density_kg_m3[0] = 2.0;
        conditions.speed_of_sound_m_s[0] = 10.0;
        conditions.dynamic_viscosity_pa_s[0] = 0.5;
        update_freestream(&mut conditions);
        assert_eq!(conditions.velocity_m_s[0], 5.0);
        assert_eq!(conditions.mach[0], 0.5);
        assert_eq!(conditions.dynamic_pressure_pa[0], 25.0);
        assert_eq!(conditions.reynolds_number_per_m[0], 20.0);
    }

    // The mass integral leaves row zero alone. Stated on its own because the
    // whole coupling between segments runs through that one value: an
    // implementation that overwrote it would restart every segment at the
    // mass its own first integration step produced.
    #[test]
    fn the_mass_integral_leaves_the_first_row_pinned() {
        let mut conditions = Conditions::expanded(3);
        conditions.total_mass_kg = vec![1000.0, 1000.0, 1000.0];
        conditions.vehicle_mass_rate_kg_s = vec![1.0, 1.0, 1.0];
        conditions.gravity_m_s2 = vec![10.0, 10.0, 10.0];
        // A trapezoid-like operator whose first row is zero, as a Chebyshev
        // integration operator's is.
        let integrate = vec![
            vec![0.0, 0.0, 0.0],
            vec![5.0, 5.0, 0.0],
            vec![5.0, 10.0, 5.0],
        ];
        update_weights(&mut conditions, &integrate);
        assert_eq!(conditions.total_mass_kg, vec![1000.0, 990.0, 980.0]);
        assert_eq!(conditions.gravity_force_vector_n[0][2], 10_000.0);
        assert_eq!(conditions.gravity_force_vector_n[2][2], 9_800.0);
    }

    // The shift preserves the shape of the array it is given rather than
    // flattening it, which is what lets a re-entered iteration keep the mass
    // profile the previous one integrated.
    #[test]
    fn initializing_weights_shifts_the_profile_onto_its_starting_mass() {
        let mut conditions = Conditions::expanded(3);
        conditions.total_mass_kg = vec![1000.0, 990.0, 980.0];
        initialize_weights(&mut conditions, 70_000.0, None);
        assert_eq!(conditions.total_mass_kg, vec![70_000.0, 69_990.0, 69_980.0]);

        let initials = Initials {
            total_mass_kg: 65_000.0,
            ..Initials::default()
        };
        initialize_weights(&mut conditions, 70_000.0, Some(&initials));
        assert_eq!(conditions.total_mass_kg, vec![65_000.0, 64_990.0, 64_980.0]);
    }
}
