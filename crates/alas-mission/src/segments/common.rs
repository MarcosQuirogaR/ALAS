// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission analysis model/Methods/Missions/Segments/Common/{Aerodynamics,Weights,
// Energy}.py.
// Upstream: mission analysis model 2.5.2, LGPL-2.1 (relicensed under GPL-2.0-or-later per
// LGPL-2.1 section 3; compatible with this program's AGPL-3.0-or-later).

//! The rest of one iteration: the flow state, the forces the analyses report,
//! and the mass that burns away underneath them.
//!
//! Read in the order the process chain runs them, these say what a mission
//! segment *is*. Altitude comes out of the position vector; the atmosphere
//! comes out of the altitude; the freestream comes out of the atmosphere and
//! the velocity; the two analyses turn that plus the two unknowns into a
//! thrust and a drag polar; and the mass falls by the integral of the fuel
//! flow. What is left over: the force that does not balance the
//! acceleration, is the residual the solver drives to zero.

use alas_aero::drag_buildup::DragBreakdown;
use alas_atmo::Us1976Values;
use alas_prop::system::{PropulsionDemand, PropulsionRating};

use super::analyses::MissionAnalyses;
use super::conditions::{Conditions, Initials};
use super::frames::compute_gravity;
use crate::numerics::{integrate_row, Differentials};
use crate::point_memo::PointMemo;

/// The throttle share and the body angle (radians) a take-off-rated leg's unknowns start at. The
/// root find is local, and the default start (half the maximum-climb force, 3 degrees) left the
/// root of a climb at the safety speed out of reach above about 1,700 ft/min on the A220-300 (the
/// solver stalled with the command on its bound, though 87 % of the rating flies 2,200 ft/min): a
/// take-off-rated leg operates at 70-90 % of the rating and at a high-lift angle of attack of the
/// order of 10 degrees, so it starts there.
pub const TAKEOFF_RATED_START: (f64, f64) = (0.85, 10.0 * std::f64::consts::PI / 180.0);

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

/// `update_differentials_altitude`: how long a ramp discretized in altitude takes.
///
/// The overall time is the dimensionless integration operator's last row, scaled by the altitude
/// change, contracted against the reciprocal of the vertical velocity (a quadrature of
/// `dz / vz`) and the node grid is then that time. Written exactly as upstream contracts it,
/// because the factor `dz` is folded into the operator row before the contraction rather than
/// after.
pub fn update_differentials_altitude(conditions: &mut Conditions, numerics: &Differentials) {
    let points = conditions.len();
    if points == 0 {
        return;
    }
    let dz = conditions.position_vector_m[points - 1][2] - conditions.position_vector_m[0][2];
    let span: f64 = numerics.integrate[points - 1]
        .iter()
        .zip(&conditions.velocity_vector_m_s)
        .map(|(&weight, velocity)| (weight * dz) * (1.0 / velocity[2]))
        .sum();
    let initial_time = conditions.time_s[0];
    for (time, &node) in conditions.time_s.iter_mut().zip(&numerics.control_points) {
        *time = initial_time + node * span;
    }
}

/// Speed, Mach, Reynolds number and dynamic pressure from the velocity vector.
///
/// The speed is formed as the square root of the summed squares of all three
/// components and the dynamic pressure from the *squared* magnitude before the
/// root, which is how upstream writes it: `q` is `0.5 * rho * Vmag2`, not
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
    memo: &mut PointMemo<'_>,
    atmosphere: &[Us1976Values],
    leg_takeoff_rated: bool,
) {
    let analyses = memo.analyses();
    let mut thrusts = Vec::with_capacity(conditions.len());
    for (point, values) in atmosphere.iter().enumerate() {
        let commanded = conditions.throttle[point];
        let altitude_m = conditions.altitude_m[point];
        let velocity_m_s = conditions.velocity_m_s[point];
        let mach = conditions.mach[point];
        let gravity_m_s2 = conditions.gravity_m_s2[point];
        let mut deck = |demand| {
            memo.thrust(
                point,
                values,
                altitude_m,
                velocity_m_s,
                mach,
                gravity_m_s2,
                demand,
            )
        };
        // The deck is only ever asked for commands it defines. Outside that
        // interval the *force* follows the deck's own normalized-force
        // relation instead: `NormalizedForce(f)` delivers `f` times the force
        // available at `f = 1`, so `f * F(1)` is that relation carried past
        // the bound rather than a curve invented here. No coefficient is
        // fitted and no tolerance is introduced.
        //
        // This exists because the root find needs a gradient outside the
        // deck's domain. Where the command was clamped, every command outside
        // produced the identical force, so the residual was flat, its
        // Jacobian column was zero, and `hybrd` stalled with
        // `NoProgressSinceIterations` instead of reporting a shortfall.
        //
        // **Nothing inside the deck's domain changes**: the branch is taken
        // only where the deck itself says the answer is not the one it was
        // asked for, so a segment that already converged sees the identical
        // force it saw before. A point that needs the continuation is out of
        // envelope by construction and is refused by `converge_root`; the
        // continuation only lets the solver find that out by descending a
        // slope instead of wandering a plateau.
        let evaluated = if analyses.enforce_throttle_envelope {
            commanded.clamp(0.0, 1.0)
        } else {
            commanded
        };
        let takeoff_rated = analyses.enforce_throttle_envelope && leg_takeoff_rated;
        let (output, idle_floor_limited) = if takeoff_rated {
            // A take-off-rated leg: the command is the share of the take-off rating's force at
            // this very point, so no published point can ask more than the rating gives.
            (
                deck(PropulsionDemand::RatedFraction {
                    rating: PropulsionRating::TakeoffGoAround,
                    fraction: evaluated,
                })
                .0,
                false,
            )
        } else {
            deck(PropulsionDemand::NormalizedForce(evaluated))
        };
        let mut throttle_floor = 0.0;
        let mut thrust_n = output.thrust_n;
        if analyses.enforce_throttle_envelope {
            if takeoff_rated {
                // Outside `[0, 1]` (a trial the root find may probe, refused by `converge_root`)
                // the force follows `f F_TO`, the fuel stays the nearest rating point's.
                if commanded != evaluated {
                    let rated = deck(PropulsionDemand::RatedFraction {
                        rating: PropulsionRating::TakeoffGoAround,
                        fraction: 1.0,
                    })
                    .0;
                    if rated.thrust_n.is_finite() {
                        thrust_n = commanded * rated.thrust_n;
                    }
                }
            } else if commanded > 1.0 {
                // Above the maximum-climb rating `output` is already `F(1)`.
                thrust_n = output.thrust_n * commanded;
            } else if idle_floor_limited {
                // The deck answered from its own flight-idle floor, so the
                // command it was handed is *below* the lowest force it will
                // deliver, and every smaller command produced this same
                // force. The measured band is wide: on the product turbofan
                // deck the floor sits at 0.075-0.105 of the maximum-climb
                // force over an A320-200 descent, so the whole of `[0, 0.105]`
                // was flat while the mission's envelope still called it
                // available. A root find that wandered in could not get out
                // again - measured, the A320-200's `descent_1` stalled
                // `NoProgressSinceIterations` after 215 residual evaluations
                // at a request of -6.761 while a root existed at 0.737-0.934,
                // entirely inside the envelope.
                //
                // The deck is never asked to extrapolate through the floor: it
                // is evaluated at the floor here and at the rating below, the
                // fuel flow stays the idle value, and no sub-idle point can
                // reach a published mission because `converge_root` refuses
                // every segment that asks for one. Only the residual's force
                // follows the deck's own relation across the band, and only
                // so the root find can leave.
                //
                // **Bounding the continuation at zero force was tried and is
                // worse.** It is the more conservative-looking choice - an
                // engine has no reverse in flight - but it puts the plateau
                // back immediately below zero command, and the measured
                // A320-200 `descent_1` then stalled again
                // (`NoProgressSinceIterations`, 176 evaluations, request
                // -7.333, 48.9 kN of longitudinal force still unbalanced)
                // instead of recovering the root at 0.737-0.934 that the
                // unbounded relation finds in 43. The continuation is a
                // numerical escape from a flat region, not a propulsion
                // model, and truncating it only hides roots that exist.
                let rated = deck(PropulsionDemand::NormalizedForce(1.0)).0.thrust_n;
                if rated.is_finite() && rated > 0.0 && output.thrust_n.is_finite() {
                    throttle_floor = output.thrust_n / rated;
                    thrust_n = commanded * rated;
                }
            } else if commanded < 0.0 {
                // The deck's command interval is `[0, 1]` and its zero is zero net force
                // wherever the flight-idle net force is negative (ram drag above the idle
                // gross thrust: a high-speed descent). The engine can run lower: the
                // `Rating(FlightIdle)` point is a net drag. The mission command continues the
                // deck's own relation `f F(1)` below zero down to that point, so a descent can
                // use the drag the engine really has, and the lowest command the engine can
                // hold is `F_idle / F(1)`, reported as the (negative) available throttle floor.
                // Below it the same relation is carried on as a gradient for the root find and
                // `converge_root` refuses the point; fuel flow is the idle value throughout.
                let rated = deck(PropulsionDemand::NormalizedForce(1.0)).0.thrust_n;
                if rated.is_finite() && rated > 0.0 {
                    thrust_n = commanded * rated;
                    let idle = deck(PropulsionDemand::RatedFraction {
                        rating: PropulsionRating::FlightIdle,
                        fraction: 1.0,
                    })
                    .0;
                    if idle.thrust_n.is_finite() && idle.thrust_n < 0.0 {
                        throttle_floor = idle.thrust_n / rated;
                    }
                }
            }
        }
        // The force is what the deck delivers at the (possibly continued) command; the fuel
        // flow is the deck's at the point it was evaluated: the maximum-climb deck, the
        // take-off rating at its share on a take-off-rated leg, the idle flow for a flown
        // command below zero. A continued point past the envelope is refused by
        // `converge_root`, so its fuel is never booked; extrapolating a burn the engine cannot
        // produce would feed the mass ODE a mass it never lost.
        conditions.available_throttle_floor[point] = throttle_floor;
        conditions.thrust_force_vector_n[point] = [thrust_n, 0.0, 0.0];
        conditions.vehicle_mass_rate_kg_s[point] = output.fuel_flow_rate_kg_s;
        thrusts.push(output);
    }
    conditions.thrust = thrusts;
}

/// Add the flap, slat and gear increment of a leg flown out of the clean
/// configuration to a clean drag breakdown. The increment is a miscellaneous
/// (configuration) drag, so it is recorded in `miscellaneous_total` and in the
/// sums that contain it (`untrimmed`, `trim_corrected`, `total`): the plotted
/// components still add up to the total.
fn add_configuration_drag(breakdown: &mut DragBreakdown, configuration_delta_cd: f64) {
    breakdown.miscellaneous_total += configuration_delta_cd;
    breakdown.untrimmed += configuration_delta_cd;
    breakdown.trim_corrected += configuration_delta_cd;
    breakdown.total += configuration_delta_cd;
}

/// Run the aerodynamics analysis and dimensionalize its two coefficients.
///
/// The lift and drag coefficients are zeroed wherever the dynamic pressure is
/// not positive, and clamped to `+/- maximum_lift_coefficient`. Both are
/// translated; neither fires on this program's inputs, because a segment
/// always has airspeed and the clamp is `np.inf`.
pub fn update_aerodynamics(
    conditions: &mut Conditions,
    memo: &mut PointMemo<'_>,
    configuration_delta_cd: f64,
) {
    let analyses = memo.analyses();
    let mut breakdowns = Vec::with_capacity(conditions.len());
    for point in 0..conditions.len() {
        let solution = memo.aerodynamics(
            point,
            conditions.angle_of_attack_rad[point],
            conditions.mach[point],
            conditions.temperature_k[point],
            conditions.reynolds_number_per_m[point],
        );

        let dynamic_pressure = conditions.dynamic_pressure_pa[point];
        let mut lift_coefficient = solution.lift_coefficient;
        let mut breakdown = solution.drag;
        add_configuration_drag(&mut breakdown, configuration_delta_cd);
        let mut drag_coefficient = breakdown.total;
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
        breakdowns.push(breakdown);
    }
    conditions.drag_breakdown = breakdowns;
}

/// Shift the mass array so it begins where the previous segment ended.
///
/// With no predecessor the segment starts at `takeoff_mass_kg`, which is the
/// one number the weights analysis is reached for from inside a segment:
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
/// Row zero of the mass is deliberately left alone: upstream writes
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
            initial_mass - integrate_row(&integrate[row], &conditions.vehicle_mass_rate_kg_s)
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
    fn the_configuration_increment_is_a_component_of_the_total() {
        let mut breakdown = super::super::drag::MissionDragCoefficients {
            parasite: 0.020,
            induced: 0.100,
            wave: 0.001,
        }
        .breakdown();
        add_configuration_drag(&mut breakdown, 0.0114);
        let sum = breakdown.parasite_total
            + breakdown.induced_total
            + breakdown.compressible_total
            + breakdown.miscellaneous_total;
        assert!((breakdown.total - 0.1324).abs() < 1e-15);
        assert!((breakdown.total - sum).abs() < 1e-15);
        assert_eq!(breakdown.miscellaneous_total, 0.0114);
        assert_eq!(breakdown.untrimmed, breakdown.total);
        assert_eq!(breakdown.trim_corrected, breakdown.total);
    }

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
