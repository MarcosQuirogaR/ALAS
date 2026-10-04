// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Numerical-error bounds of the closed-form and secant solvers against
//! converged bisection references.

// Test fixtures assert successful construction through expect.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

/// Converged reference: 200-step bisection of `P(T) = T (V + v_i(T))` on
/// `[0, T_static]` (one-dimensional momentum theory, McCormick 1967 sec. 3.1).
fn bisected_actuator_disk_thrust_n(power_w: f64, rho: f64, area: f64, speed: f64) -> f64 {
    let static_thrust = (2.0 * rho * area * power_w.powi(2)).cbrt();
    let power_for = |thrust: f64| {
        let induced = 0.5 * ((speed * speed + 2.0 * thrust / (rho * area)).sqrt() - speed);
        thrust * (speed + induced)
    };
    let (mut low, mut high) = (0.0, static_thrust);
    for _ in 0..200 {
        let middle = 0.5 * (low + high);
        if power_for(middle) > power_w {
            high = middle;
        } else {
            low = middle;
        }
    }
    0.5 * (low + high)
}

#[test]
fn closed_form_actuator_disk_matches_converged_bisection() {
    let area = std::f64::consts::PI * 0.25 * 3.93_f64.powi(2);
    let mut worst: f64 = 0.0;
    for rho in [0.30, 0.55, 0.9, 1.225] {
        for speed in [0.1, 1.0, 20.0, 60.0, 110.0, 150.0, 250.0] {
            for power_w in [1.0e3, 5.0e4, 4.0e5, 1.0e6, 1.8e6, 2.5e6] {
                let closed = actuator_disk_thrust_bound_n(power_w, rho, area, speed);
                let reference = bisected_actuator_disk_thrust_n(power_w, rho, area, speed);
                let error = (closed - reference).abs() / reference;
                worst = worst.max(error);
                // The energy balance closes; (u - V) cancels at high V and low P, so the
                // residual is only conditioned to about 1e-9 there.
                let disk_velocity = power_w / closed;
                let residual = 2.0 * rho * area * disk_velocity.powi(2) * (disk_velocity - speed);
                assert!((residual - power_w).abs() <= 1.0e-9 * power_w);
            }
        }
    }
    assert!(worst <= 1.0e-12, "worst relative error {worst:e}");
}

#[test]
fn actuator_disk_limits_are_static_bound_and_zero_power() {
    let (rho, area) = (1.225, 12.13);
    let static_thrust = actuator_disk_thrust_bound_n(1.0e6, rho, area, 0.0);
    assert_eq!(static_thrust, (2.0 * rho * area * 1.0e12_f64).cbrt());
    // A vanishing forward speed approaches the static bound continuously.
    let creeping = actuator_disk_thrust_bound_n(1.0e6, rho, area, 1.0e-6);
    assert!((creeping - static_thrust).abs() / static_thrust < 1.0e-6);
    assert_eq!(actuator_disk_thrust_bound_n(0.0, rho, area, 80.0), 0.0);
    // Thrust is bounded by P / V (propulsive efficiency below one).
    assert!(actuator_disk_thrust_bound_n(1.0e6, rho, area, 80.0) < 1.0e6 / 80.0);
}

fn atr_system() -> Atr72TurbopropSystem {
    Atr72TurbopropSystem::new(
        Pw127m568fModel::default(),
        Vec::new(),
        PropulsionInstallation {
            unit_positions_m: vec![[2.0, -6.0, 0.0], [2.0, 6.0, 0.0]],
            thrust_axes_body: vec![[1.0, 0.0, 0.0]; 2],
            nacelle_wetted_area_m2: None,
            frontal_area_m2: None,
        },
    )
    .expect("installation")
}

/// Standard-day flight condition at altitude `altitude_m` and Mach `mach`.
fn flight(altitude_m: f64, mach: f64) -> FlightCondition {
    let air = alas_atmo::us1976_compute_values(altitude_m, 0.0);
    let velocity = mach * air.speed_of_sound_m_s;
    FlightCondition {
        altitude_m,
        mach,
        pressure_pa: air.pressure_pa,
        temperature_k: air.temperature_k,
        density_kg_m3: air.density_kg_m3,
        dynamic_viscosity_pa_s: air.dynamic_viscosity_pa_s,
        gravity_m_s2: 9.806_65,
        gamma: 1.4,
        cp_j_kgk: 1_004.5,
        gas_constant_j_kgk: 287.05,
        speed_of_sound_m_s: air.speed_of_sound_m_s,
        velocity_m_s: velocity,
        stagnation_temperature_k: air.temperature_k * (1.0 + 0.2 * mach * mach),
        stagnation_pressure_pa: air.pressure_pa * (1.0 + 0.2 * mach * mach).powf(3.5),
    }
}

fn request(flight: FlightCondition, demand: PropulsionDemand) -> PropulsionRequest {
    PropulsionRequest {
        flight,
        demand,
        mode: OperatingMode::Normal,
        failure: FailureState::None,
        loads: Default::default(),
        state: Default::default(),
        time_step_s: None,
    }
}

/// Converged reference for the normalized-force inverse: the original
/// 1/1000 linear scan for the lowest governed fraction followed by a
/// 200-step bisection on thrust. `Err(())` when no solution exists.
fn bisected_force_fraction(
    unit: Pw127m568fModel,
    condition: TurbopropCondition,
    rating: Pw127mRating,
    requested: f64,
) -> Result<f64, ()> {
    let thrust_at = |power_fraction: f64| {
        unit.evaluate(
            condition,
            TurbopropCommand {
                rating,
                power_fraction,
                mode: TurbopropMode::Governed,
                propeller_speed_rpm: unit.governed_propeller_speed_rpm,
            },
        )
        .map(|output| output.total_thrust_n)
    };
    let target = requested * thrust_at(1.0).map_err(|_| ())?;
    let (mut low, minimum) = (1..=1_000)
        .find_map(|step| {
            let fraction = f64::from(step) / 1_000.0;
            thrust_at(fraction).ok().map(|thrust| (fraction, thrust))
        })
        .ok_or(())?;
    if target < minimum {
        return Err(());
    }
    let mut high = 1.0;
    for _ in 0..200 {
        let middle = 0.5 * (low + high);
        if thrust_at(middle).map_err(|_| ())? < target {
            low = middle;
        } else {
            high = middle;
        }
    }
    Ok(0.5 * (low + high))
}

#[test]
fn secant_force_solve_matches_converged_bisection_on_a_flight_grid() {
    let unit = Pw127m568fModel::default();
    let mut solved = 0;
    let mut worst: f64 = 0.0;
    for rating in [Pw127mRating::NormalTakeoff, Pw127mRating::MaximumClimb] {
        for altitude_m in [0.0, 1_500.0, 3_000.0, 4_500.0, 6_000.0, 7_600.0] {
            for mach in [0.1, 0.25, 0.4] {
                let air = flight(altitude_m, mach);
                let condition = TurbopropCondition {
                    density_kg_m3: air.density_kg_m3,
                    true_airspeed_m_s: air.velocity_m_s,
                };
                for requested in [0.05, 0.15, 0.3, 0.5, 0.7, 0.9, 1.0] {
                    let reference = bisected_force_fraction(unit, condition, rating, requested);
                    let result = Atr72TurbopropSystem::solve_normalized_force_fraction(
                        unit,
                        condition,
                        rating,
                        TurbopropMode::Governed,
                        requested,
                    );
                    match (reference, result) {
                        (Ok(expected), Ok(actual)) => {
                            solved += 1;
                            worst = worst.max((actual - expected).abs() / expected);
                        }
                        (Err(()), Err(PropulsionError::OutsideModelDomain(_))) => {}
                        (reference, result) => panic!(
                            "solvers disagree on solvability at {altitude_m} m M{mach} f{requested}: {reference:?} vs {result:?}"
                        ),
                    }
                }
            }
        }
    }
    assert!(solved > 50, "grid exercises too few solvable points");
    assert!(worst <= 1.0e-9, "worst relative fraction error {worst:e}");
}

/// Every power fraction the normalized-force inverse returns delivers the
/// requested thrust within its stated relative tolerance (1e-12), across the
/// thrust drop at the governor/fallback boundary; a force no fraction
/// delivers (above full thrust) is an error, not the nearest bracket end.
#[test]
fn force_inverse_returns_only_force_converged_fractions() {
    let unit = Pw127m568fModel::default();
    let mut solved = 0;
    for altitude_m in [0.0, 3_000.0, 7_600.0] {
        for mach in [0.15, 0.25, 0.4] {
            let air = flight(altitude_m, mach);
            let condition = TurbopropCondition {
                density_kg_m3: air.density_kg_m3,
                true_airspeed_m_s: air.velocity_m_s,
            };
            let thrust_at = |power_fraction: f64| {
                unit.evaluate(
                    condition,
                    TurbopropCommand {
                        rating: Pw127mRating::MaximumClimb,
                        power_fraction,
                        mode: TurbopropMode::Governed,
                        propeller_speed_rpm: unit.governed_propeller_speed_rpm,
                    },
                )
                .unwrap()
                .total_thrust_n
            };
            let maximum = thrust_at(1.0);
            let solve = |requested: f64| {
                Atr72TurbopropSystem::solve_normalized_force_fraction(
                    unit,
                    condition,
                    Pw127mRating::MaximumClimb,
                    TurbopropMode::Governed,
                    requested,
                )
            };
            for step in 1..=200 {
                let requested = f64::from(step) / 200.0;
                if let Ok(fraction) = solve(requested) {
                    solved += 1;
                    let target = requested * maximum;
                    let residual = (thrust_at(fraction) - target).abs();
                    assert!(
                        residual <= inverse::SOLVER_RELATIVE_TOLERANCE * target,
                        "{altitude_m} m M{mach} f{requested}: residual {residual} N"
                    );
                }
            }
            assert!(
                matches!(solve(1.05), Err(PropulsionError::OutsideModelDomain(_))),
                "{altitude_m} m M{mach}: 105 % of full thrust was returned as converged"
            );
        }
    }
    assert!(solved > 1_000, "grid exercises too few solvable points");
}

#[test]
fn fuel_flow_and_thrust_do_not_decrease_with_commanded_thrust() {
    let system = atr_system();
    for altitude_m in [0.0, 3_000.0, 6_000.0] {
        for mach in [0.15, 0.3, 0.4] {
            let condition = flight(altitude_m, mach);
            let mut previous = (f64::NEG_INFINITY, f64::NEG_INFINITY);
            for step in 0..=100 {
                let demand = PropulsionDemand::NormalizedForce(f64::from(step) / 100.0);
                let result = system.evaluate(&request(condition, demand)).expect("point");
                // Demands below the lowest governed point return the neutral
                // flight-idle surrogate, whose fuel flow is a separate branch.
                if result
                    .active_limits
                    .iter()
                    .any(|limit| limit.name == "flight-idle-thrust")
                {
                    continue;
                }
                let thrust = result.body_force_n[0];
                let fuel = result.resource_flows[0].mass_flow_kg_s.expect("fuel");
                assert!(
                    thrust >= previous.0 - 1.0e-6 && fuel >= previous.1 - 1.0e-12,
                    "non-monotone at {altitude_m} m M{mach} step {step}: thrust {thrust} fuel {fuel} after {previous:?}"
                );
                previous = (thrust, fuel);
            }
        }
    }
}

#[test]
fn active_limit_labels_match_the_rating_names() {
    let system = atr_system();
    for rating in [
        PropulsionRating::TakeoffGoAround,
        PropulsionRating::MaximumContinuous,
        PropulsionRating::MaximumClimb,
        PropulsionRating::Cruise,
        PropulsionRating::FlightIdle,
    ] {
        let result = system
            .evaluate(&request(
                flight(1_000.0, 0.2),
                PropulsionDemand::Rating(rating),
            ))
            .expect("rated point");
        let name = &result.active_limits[0].name;
        assert!(name.starts_with("PW127M "));
        assert!([
            "NormalTakeoff",
            "MaximumContinuous",
            "MaximumClimb",
            "MaximumCruise",
            "FlightIdleSurrogate"
        ]
        .contains(&&name["PW127M ".len()..]));
    }
    for rating in [
        Pw127mRating::NormalTakeoff,
        Pw127mRating::MaximumTakeoffReserve,
        Pw127mRating::MaximumContinuous,
        Pw127mRating::MaximumClimb,
        Pw127mRating::MaximumCruise,
        Pw127mRating::FlightIdleSurrogate,
    ] {
        assert_eq!(
            system::rating_limit_name(rating),
            format!("PW127M {rating:?}")
        );
    }
}

#[test]
fn identical_units_are_evaluated_once_with_the_per_unit_sum_preserved() {
    let system = atr_system();
    let unit = Pw127m568fModel::default();
    for altitude_m in [0.0, 1_500.0, 4_500.0, 7_000.0] {
        for mach in [0.0, 0.15, 0.3, 0.45] {
            let condition = flight(altitude_m, mach);
            let result = system
                .evaluate(&request(
                    condition,
                    PropulsionDemand::Rating(PropulsionRating::TakeoffGoAround),
                ))
                .expect("two-engine result");
            let output = unit
                .evaluate_at_temperature(
                    TurbopropCondition {
                        density_kg_m3: condition.density_kg_m3,
                        true_airspeed_m_s: condition.velocity_m_s,
                    },
                    condition.temperature_k,
                    TurbopropCommand {
                        rating: Pw127mRating::NormalTakeoff,
                        power_fraction: 1.0,
                        mode: TurbopropMode::Governed,
                        propeller_speed_rpm: unit.governed_propeller_speed_rpm,
                    },
                )
                .expect("unit result");
            // The original loop accumulated `0 + x + x` over the two units.
            assert_eq!(
                result.body_force_n[0].to_bits(),
                (output.total_thrust_n + output.total_thrust_n).to_bits()
            );
            let fuel = result.resource_flows[0].mass_flow_kg_s.expect("fuel");
            assert_eq!(
                fuel.to_bits(),
                (output.fuel_flow_kg_s + output.fuel_flow_kg_s).to_bits()
            );
        }
    }
}
