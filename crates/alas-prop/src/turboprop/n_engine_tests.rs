// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! N-engine (four-unit) behaviour of the turboprop adapter. SI units, body axes
//! x forward, y starboard, z down; moments are about the body origin.

use super::*;

fn system(positions: &[[f64; 3]]) -> Atr72TurbopropSystem {
    Atr72TurbopropSystem::new(
        Pw127m568fModel::default(),
        Vec::new(),
        PropulsionInstallation {
            unit_positions_m: positions.to_vec(),
            thrust_axes_body: vec![[1.0, 0.0, 0.0]; positions.len()],
            nacelle_wetted_area_m2: None,
            frontal_area_m2: None,
        },
    )
    .unwrap()
}

fn twin() -> Atr72TurbopropSystem {
    system(&[[2.0, -6.0, 0.3], [2.0, 6.0, 0.3]])
}

fn quad() -> Atr72TurbopropSystem {
    system(&[
        [2.0, -14.0, 0.3],
        [2.0, -6.0, 0.3],
        [2.0, 6.0, 0.3],
        [2.0, 14.0, 0.3],
    ])
}

fn request(failure: FailureState) -> PropulsionRequest {
    PropulsionRequest {
        flight: FlightCondition {
            altitude_m: 0.0,
            mach: 0.0,
            pressure_pa: 101_325.0,
            temperature_k: 288.15,
            density_kg_m3: 1.225,
            dynamic_viscosity_pa_s: 1.789e-5,
            gravity_m_s2: 9.806_65,
            gamma: 1.4,
            cp_j_kgk: 1_004.5,
            gas_constant_j_kgk: 287.05,
            speed_of_sound_m_s: 340.3,
            velocity_m_s: 0.0,
            stagnation_temperature_k: 288.15,
            stagnation_pressure_pa: 101_325.0,
        },
        demand: PropulsionDemand::Rating(PropulsionRating::TakeoffGoAround),
        mode: OperatingMode::Normal,
        failure,
        loads: Default::default(),
        state: Default::default(),
        time_step_s: None,
    }
}

#[test]
fn invalid_unit_counts_and_axes_are_rejected() {
    let bad = |positions: Vec<[f64; 3]>, axes: usize| {
        Atr72TurbopropSystem::new(
            Pw127m568fModel::default(),
            Vec::new(),
            PropulsionInstallation {
                unit_positions_m: positions,
                thrust_axes_body: vec![[1.0, 0.0, 0.0]; axes],
                nacelle_wetted_area_m2: None,
                frontal_area_m2: None,
            },
        )
        .is_err()
    };
    assert!(bad(vec![[0.0; 3]], 1));
    assert!(bad(vec![[0.0; 3]; 3], 3));
    assert!(bad(vec![[0.0; 3]; 4], 3));
    assert!(!bad(vec![[0.0; 3]; 4], 4));
    assert!(!bad(vec![[0.0; 3]; 6], 6));
}

#[test]
fn four_symmetric_engines_double_the_twin_thrust_and_fuel_flow_with_zero_net_moment() {
    let two = twin().evaluate(&request(FailureState::None)).unwrap();
    let four = quad().evaluate(&request(FailureState::None)).unwrap();
    assert_eq!(four.body_force_n[0], 2.0 * two.body_force_n[0]);
    assert_eq!(
        four.shaft_power_w.unwrap(),
        2.0 * two.shaft_power_w.unwrap()
    );
    assert_eq!(
        four.resource_flows[0].mass_flow_kg_s.unwrap(),
        2.0 * two.resource_flows[0].mass_flow_kg_s.unwrap()
    );
    // Symmetric y arms cancel; only the common z offset gives a pitching moment.
    assert!(four.body_moment_nm[2].abs() < 1.0e-6 * four.body_force_n[0]);
    assert!(four.body_moment_nm[0].abs() < 1.0e-6 * four.body_force_n[0]);
}

#[test]
fn oei_removes_the_outboard_unit_and_uses_the_reserve_rating() {
    let system = quad();
    assert_eq!(system.unit_count(), 4);
    // Outboard units are indices 0 (port, y = -14 m) and 3; ties go to index 0.
    assert_eq!(system.critical_unit_index(), 0);
    let aeo = system.evaluate(&request(FailureState::None)).unwrap();
    let oei = system
        .evaluate(&request(system.critical_engine_failure()))
        .unwrap();
    let per_unit_reserve = oei.body_force_n[0] / 3.0;
    let per_unit_normal = aeo.body_force_n[0] / 4.0;
    // Reserve rating is above the normal take-off rating, so three units give
    // more than three quarters of the four-unit thrust but less than all four.
    assert!(per_unit_reserve > per_unit_normal);
    assert!(oei.body_force_n[0] < aeo.body_force_n[0]);
    assert_eq!(oei.active_limits[0].name, "PW127M MaximumTakeoffReserve");
    // Three remaining units: yaw moment = -sum(y * Fx) with y = -6, 6, 14 m.
    // Fx > 0 along +x, N_z = x*Fy - y*Fx = -(sum y) * F_unit = -14 m * F_unit.
    let expected = -14.0 * per_unit_reserve;
    assert!((oei.body_moment_nm[2] - expected).abs() < 1.0e-9 * expected.abs());
    // Two failed units are not the single-failure reserve case.
    let two_out = system
        .evaluate(&request(FailureState::UnitsUnavailable(vec![0, 3])))
        .unwrap();
    assert_eq!(two_out.active_limits[0].name, "PW127M NormalTakeoff");
    assert_eq!(two_out.body_force_n[0], 2.0 * per_unit_normal);
}

#[test]
fn failure_indices_are_validated_against_the_unit_count() {
    let system = quad();
    for bad in [vec![4], vec![1, 1], vec![0, 1, 2, 3, 0]] {
        assert!(matches!(
            system.evaluate(&request(FailureState::UnitsUnavailable(bad))),
            Err(PropulsionError::UnsupportedFailureState)
        ));
    }
    let none = system
        .evaluate(&request(FailureState::UnitsUnavailable(vec![0, 1, 2, 3])))
        .unwrap();
    assert_eq!(none.body_force_n, [0.0; 3]);
    assert!(matches!(
        twin().evaluate(&request(FailureState::UnitsUnavailable(vec![2]))),
        Err(PropulsionError::UnsupportedFailureState)
    ));
}
