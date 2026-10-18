// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Failed assertions identify broken fixtures in this test target.
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Operating-point solver verification.
//!
//! References: (a) the closed form for a loss-free chain (Drela motor with
//! i0 = 0, Rm -> 0: omega = Kv D Voc, eta = 1), (b) exact power balance,
//! (c) self-consistency of the limiter and of the thrust-demand inversion.
//! The 2814 motor row is the Hobbywing page value stored in the kit.

use alas_uav::power::from_spec::{battery_from_spec, esc_from_spec};
use alas_uav::power::{
    BatteryModel, BatteryState, BemtGeometry, Demand, Dynamics, EscModel, Flight, LimitFlag,
    Limiter, MotorModel, MotorThermal, PowerError, Powertrain, PropMapTable, PropModel,
};
use alas_uav_db::parts::{BatterySpec, EscSpec};
use alas_uav_db::{
    ComponentId, Field, IsoDate, Method, PerfKind, PropPerfRow, PropPerfSet, SourceId,
};

const T_AMB: f64 = 293.15;

fn dynamics(t_w: f64) -> Dynamics {
    Dynamics {
        battery: BatteryState::new(1.0, 298.15),
        t_winding_k: t_w,
    }
}

fn kv_si(rpm_per_v: f64) -> f64 {
    rpm_per_v * std::f64::consts::TAU / 60.0
}

fn real_chain() -> Powertrain {
    let mut motor = MotorModel::new(kv_si(1000.0), 0.029, 2.0, 14.8).unwrap();
    motor.thermal = Some(MotorThermal {
        r_th_k_per_w: 2.0,
        c_th_j_per_k: 450.0 * 0.115,
    });
    let mut battery = BatteryModel::new(5.0 * 3600.0, 4, 0.02).unwrap();
    battery.mass_kg = 0.4;
    Powertrain {
        motor,
        esc: EscModel::from_nominal_efficiency(0.96, 14.8, 60.0).unwrap(),
        battery,
        prop: PropModel::from_bemt(BemtGeometry::generic(0.254, 0.127, 2, 0.15).unwrap()),
        n_motors: 1,
    }
}

fn wide_map() -> PropMapTable {
    let mut rows = Vec::new();
    for omega in [20.0, 100.0, 200.0, 300.0, 400.0] {
        for j in [0.0, 0.25, 0.5, 0.75, 1.0] {
            rows.push(PropPerfRow {
                omega_rad_s: omega,
                j,
                re: None,
                ct: 0.12 - 0.1 * j,
                cp: 0.06 - 0.03 * j,
                eta: None,
            });
        }
    }
    let set = PropPerfSet {
        prop_id: ComponentId::new("synthetic").unwrap(),
        source_id: SourceId::new("synthetic").unwrap(),
        kind: PerfKind::Measured,
        retrieved_at: Some(IsoDate::parse("2026-10-12").unwrap()),
        rows,
    };
    PropMapTable::from_set(&set, 0.254).unwrap()
}

#[test]
fn loss_free_chain_reproduces_the_closed_form() {
    // Rm -> 0, i0 = 0, ideal ESC, near-zero pack resistance: omega = Kv D Voc.
    let kv = 25.0;
    let motor = MotorModel::new(kv, 1.0e-9, 0.0, 10.0).unwrap();
    let battery = BatteryModel::new(18_000.0, 4, 1.0e-9).unwrap();
    let chain = Powertrain {
        motor,
        esc: EscModel::ideal(),
        battery,
        prop: PropModel::from_map(wide_map(), true),
        n_motors: 1,
    };
    let fl = Flight::new(5.0, 1.225, T_AMB);
    let dy = dynamics(T_AMB);
    for d in [0.5, 0.7, 0.9] {
        let op = chain.solve_duty(&fl, &dy, d).unwrap();
        let voc = chain.battery.voc(1.0);
        assert!(
            (op.omega_rad_s / (kv * d * voc) - 1.0).abs() < 1.0e-6,
            "omega {} vs {}",
            op.omega_rad_s,
            kv * d * voc
        );
        assert!((op.eta_motor - 1.0).abs() < 1.0e-6 && (op.eta_esc - 1.0).abs() < 1.0e-6);
        // Whole chain lossless: electrical power equals shaft power; eta_total = eta_prop = J CT / CP.
        assert!((op.p_battery_w / op.p_shaft_w - 1.0).abs() < 1.0e-6);
        assert!((op.eta_total - op.eta_prop).abs() < 1.0e-6);
    }
}

#[test]
fn power_balance_closes_for_map_and_bemt_chains() {
    let fl = Flight::new(12.0, 1.1, 288.0);
    let dy = dynamics(320.0);
    let map_chain = Powertrain {
        prop: PropModel::from_map(wide_map(), true),
        ..real_chain()
    };
    for chain in [real_chain(), map_chain] {
        for d in [0.4, 0.7, 1.0] {
            let Ok(op) = chain.solve_duty(&fl, &dy, d) else {
                continue;
            };
            assert!(
                op.balance_residual.abs() < 1.0e-8,
                "residual {} at D = {d}",
                op.balance_residual
            );
            assert!(
                (op.p_battery_w - (op.p_shaft_w + op.p_copper_w + op.p_iron_w + op.p_esc_loss_w))
                    .abs()
                    < 1.0e-6 * op.p_battery_w
            );
            assert!((op.v_pack_v - (op.v_oc_v - op.sag_v)).abs() < 1.0e-12);
        }
    }
}

#[test]
fn part_throttle_has_lower_battery_than_motor_current_and_monotone_speed() {
    let chain = real_chain();
    let fl = Flight::new(10.0, 1.225, T_AMB);
    let dy = dynamics(T_AMB);
    let mut last_rpm = 0.0;
    let mut last_thrust = -1.0;
    for d in [0.45, 0.6, 0.8, 1.0] {
        let op = chain.solve_duty(&fl, &dy, d).unwrap();
        assert!(op.rpm > last_rpm && op.thrust_n > last_thrust);
        (last_rpm, last_thrust) = (op.rpm, op.thrust_n);
        if d < 0.9 {
            assert!(
                op.i_battery_a < op.i_motor_a,
                "I_b {} !< I_m {}",
                op.i_battery_a,
                op.i_motor_a
            );
        }
        assert!(op.sag_v > 0.0 && op.v_pack_v < op.v_oc_v);
        assert!(op.eta_total > 0.0 && op.eta_total < 1.0);
    }
}

#[test]
fn hot_winding_loses_thrust_through_resistance_and_kv() {
    let chain = real_chain();
    let fl = Flight::new(10.0, 1.225, T_AMB);
    let cold = chain.solve_duty(&fl, &dynamics(293.15), 0.9).unwrap();
    let hot = chain.solve_duty(&fl, &dynamics(393.15), 0.9).unwrap();
    assert!(hot.p_copper_w / hot.i_motor_a.powi(2) > cold.p_copper_w / cold.i_motor_a.powi(2)); // Rm(T)
    assert!(hot.eta_motor < cold.eta_motor);
}

#[test]
fn thrust_demand_round_trips_and_unreachable_demand_is_reported() {
    let chain = real_chain();
    let fl = Flight::new(15.0, 1.225, T_AMB);
    let dy = dynamics(T_AMB);
    let full = chain
        .solve(&fl, &dy, Demand::Throttle(1.0), &Limiter::none())
        .unwrap();
    let want = 0.5 * full.thrust_n;
    let op = chain
        .solve(&fl, &dy, Demand::Thrust(want), &Limiter::none())
        .unwrap();
    assert!((op.thrust_n / want - 1.0).abs() < 1.0e-6);
    assert!(op.duty < full.duty && op.i_battery_a < full.i_battery_a);
    match chain.solve(
        &fl,
        &dy,
        Demand::Thrust(2.0 * full.thrust_n),
        &Limiter::none(),
    ) {
        Err(PowerError::DemandUnreachable { available_n, .. }) => {
            assert!((available_n / full.thrust_n - 1.0).abs() < 1.0e-9)
        }
        other => panic!("expected DemandUnreachable, got {other:?}"),
    }
}

#[test]
fn battery_current_limiter_binds_and_lowers_duty() {
    let chain = real_chain();
    let fl = Flight::new(8.0, 1.225, T_AMB);
    let dy = dynamics(T_AMB);
    let free = chain
        .solve(&fl, &dy, Demand::Throttle(1.0), &Limiter::none())
        .unwrap();
    let cap = 0.6 * free.i_battery_a;
    let lim = Limiter {
        battery_current_cap_a: Some(cap),
        ..Limiter::none()
    };
    let op = chain.solve(&fl, &dy, Demand::Throttle(1.0), &lim).unwrap();
    assert!(op.limited_by.contains(&LimitFlag::BatteryCurrentCap));
    assert!(
        (op.i_battery_a / cap - 1.0).abs() < 1.0e-6,
        "I_b {} vs cap {cap}",
        op.i_battery_a
    );
    assert!(op.duty < free.duty && op.duty_requested == free.duty);
    assert!(op.thrust_n < free.thrust_n);
    // A cap above the unconstrained draw is inactive.
    let loose = chain
        .solve(
            &fl,
            &dy,
            Demand::Throttle(1.0),
            &Limiter {
                battery_current_cap_a: Some(2.0 * free.i_battery_a),
                ..Limiter::none()
            },
        )
        .unwrap();
    assert!(loose.limited_by.is_empty() && (loose.thrust_n - free.thrust_n).abs() < 1.0e-12);
    // A thrust demand above what the cap allows reports the shortfall.
    let want = 0.95 * free.thrust_n;
    let short = chain.solve(&fl, &dy, Demand::Thrust(want), &lim).unwrap();
    assert!(short.limited_by.contains(&LimitFlag::ThrustShortfall));
}

#[test]
fn ratings_are_enforced_or_only_warned() {
    let mut chain = real_chain();
    chain.motor.limits.i_cont_a = Some(20.0);
    let fl = Flight::new(8.0, 1.225, T_AMB);
    let dy = dynamics(T_AMB);
    let warn = chain
        .solve(&fl, &dy, Demand::Throttle(1.0), &Limiter::none())
        .unwrap();
    assert!(warn.warnings.contains(&LimitFlag::MotorCurrent) && warn.limited_by.is_empty());
    let hard = chain
        .solve(
            &fl,
            &dy,
            Demand::Throttle(1.0),
            &Limiter {
                enforce_ratings: true,
                ..Limiter::none()
            },
        )
        .unwrap();
    assert!(hard.limited_by.contains(&LimitFlag::MotorCurrent));
    assert!((hard.i_motor_a / 20.0 - 1.0).abs() < 1.0e-6);
}

#[test]
fn steady_thermal_solution_is_self_consistent() {
    // 0.8 K/W: a motor in the propeller slipstream.
    let mut chain = real_chain();
    chain.motor.thermal = Some(MotorThermal {
        r_th_k_per_w: 0.8,
        c_th_j_per_k: 450.0 * 0.115,
    });
    let fl = Flight::new(10.0, 1.225, T_AMB);
    let op = chain
        .solve_steady_thermal(
            &fl,
            &dynamics(T_AMB),
            Demand::Throttle(0.9),
            &Limiter::none(),
        )
        .unwrap();
    let heat = (op.p_copper_w + op.p_iron_w) / f64::from(chain.n_motors);
    assert!((op.t_winding_k - (T_AMB + 0.8 * heat)).abs() < 1.0e-3);
    assert!(
        op.t_winding_k > T_AMB + 20.0,
        "winding rise {}",
        op.t_winding_k - T_AMB
    );
    assert!(op.dt_winding_dt_k_s.abs() < 1.0e-3);
    // Cold start: positive heating rate and a finite time to a limit halfway to the
    // (constant-power) cold-start steady value.
    let cold = chain
        .solve(
            &fl,
            &dynamics(T_AMB),
            Demand::Throttle(0.9),
            &Limiter::none(),
        )
        .unwrap();
    let t_limit = T_AMB + 0.5 * (cold.t_winding_steady_k - T_AMB);
    let mut limited = chain.clone();
    limited.motor.limits.t_winding_max_k = Some(t_limit);
    let start = limited
        .solve(
            &fl,
            &dynamics(T_AMB),
            Demand::Throttle(0.9),
            &Limiter::none(),
        )
        .unwrap();
    assert!(start.dt_winding_dt_k_s > 0.0);
    let t_hit = start.time_to_t_max_s.unwrap();
    let th = chain.motor.thermal.unwrap();
    let tau = th.r_th_k_per_w * th.c_th_j_per_k;
    // First-order law: T(t) = Tss + (T0 - Tss) exp(-t / tau) reaches t_limit at t_hit.
    let t_at = cold.t_winding_steady_k + (T_AMB - cold.t_winding_steady_k) * (-t_hit / tau).exp();
    assert!((t_at - t_limit).abs() < 1.0e-9);
    // At 2 K/W the copper-resistance feedback has no stable steady state at this power: runaway is reported.
    let runaway = real_chain().solve_steady_thermal(
        &fl,
        &dynamics(T_AMB),
        Demand::Throttle(0.9),
        &Limiter::none(),
    );
    assert!(
        matches!(runaway, Err(PowerError::NoSolution(_))),
        "{runaway:?}"
    );
}

#[test]
fn advance_integrates_soc_and_winding_temperature() {
    let chain = real_chain();
    let fl = Flight::new(10.0, 1.225, T_AMB);
    let mut dy = dynamics(T_AMB);
    let op = chain.solve_duty(&fl, &dy, 0.8).unwrap();
    let step = chain.advance(&mut dy, &op, &fl, 10.0);
    assert!(!step.cutoff);
    assert!(
        (1.0 - dy.battery.soc - op.i_battery_a * 10.0 / chain.battery.capacity_c()).abs() < 1.0e-12
    );
    assert!(dy.t_winding_k > T_AMB && dy.battery.t_k > 298.15);
    assert!((op.dsoc_dt * 10.0 + (1.0 - dy.battery.soc)).abs() < 1.0e-12);
}

#[test]
fn invalid_requests_are_errors_not_numbers() {
    let chain = real_chain();
    let fl = Flight::new(10.0, 1.225, T_AMB);
    let dy = dynamics(T_AMB);
    assert!(chain
        .solve(&fl, &dy, Demand::Throttle(1.3), &Limiter::none())
        .is_err());
    assert!(chain
        .solve(&fl, &dy, Demand::Throttle(f64::NAN), &Limiter::none())
        .is_err());
    assert!(chain
        .solve(&fl, &dy, Demand::Throttle(0.0), &Limiter::none())
        .is_err());
    assert!(chain
        .solve(
            &Flight::new(-1.0, 1.225, T_AMB),
            &dy,
            Demand::Throttle(0.5),
            &Limiter::none()
        )
        .is_err());
    assert!(chain
        .solve(
            &Flight::new(5.0, 0.0, T_AMB),
            &dy,
            Demand::Throttle(0.5),
            &Limiter::none()
        )
        .is_err());
    // An outside-the-map condition is reported, not extrapolated.
    let map_chain = Powertrain {
        prop: PropModel::from_map(wide_map(), true),
        ..real_chain()
    };
    assert!(map_chain
        .solve_duty(&Flight::new(60.0, 1.225, T_AMB), &dy, 0.5)
        .is_err());
}

#[test]
fn two_motors_share_the_pack() {
    let one = real_chain();
    let two = Powertrain {
        n_motors: 2,
        ..real_chain()
    };
    let fl = Flight::new(10.0, 1.225, T_AMB);
    let dy = dynamics(T_AMB);
    let a = one.solve_duty(&fl, &dy, 0.8).unwrap();
    let b = two.solve_duty(&fl, &dy, 0.8).unwrap();
    assert!(b.thrust_n > 1.5 * a.thrust_n && b.thrust_n < 2.0 * a.thrust_n); // more sag
    assert!(b.i_battery_a > 1.5 * a.i_battery_a);
    assert!(b.balance_residual.abs() < 1.0e-8);
}

fn text<T>(v: T, unit: &str) -> Field<T> {
    Field::new(v, unit, SourceId::new("test").unwrap(), None, Method::User)
}

#[test]
fn battery_and_esc_build_from_database_rows() {
    let spec = BatterySpec {
        capacity_c: Some(text(5.0 * 3600.0, "C")),
        cells_s: Some(text(4, "1")),
        r_dc_ohm: Some(text(0.012, "Ohm")),
        v_min_v: Some(text(12.8, "V")),
        mass_kg: Some(text(0.42, "kg")),
        i_cont_a: Some(text(125.0, "A")),
        ..BatterySpec::default()
    };
    let b = battery_from_spec(&spec, "pack").unwrap();
    assert!((b.v_cutoff_v - 12.8).abs() < 1.0e-12 && b.i_cont_a == Some(125.0));
    assert!((b.voc(1.0) - 16.8).abs() < 1.0e-12);
    assert!(battery_from_spec(&BatterySpec::default(), "pack").is_err());
    let esc = esc_from_spec(
        &EscSpec {
            i_cont_a: Some(text(60.0, "A")),
            ..EscSpec::default()
        },
        "esc",
        0.97,
        14.8,
    )
    .unwrap();
    assert!((esc.efficiency(1.0, 14.8, 60.0) - 0.97).abs() < 1.0e-12 && esc.i_cont_a == Some(60.0));
    assert!(esc_from_spec(&EscSpec::default(), "esc", 0.97, 14.8).is_err());
}
