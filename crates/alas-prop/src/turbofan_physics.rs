// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Candidate physical primitives for the unified product turbofan.
//!
//! These routines deliberately do not depend on either legacy turbofan kernel.
//! They implement dimensional control-volume balances, so the static limit is
//! regular. Thermodynamic properties use NASA seven-coefficient ideal-gas data
//! documented by McBride et al., NASA/TP-2002-211556; nozzle and thrust
//! balances follow the one-dimensional equations used by Mattingly.
//!
//! The implemented temperature domain is deliberately restricted to
//! 200--2000 K, although the source species fits extend to 6000 K. That upper
//! limit covers the intended transport-engine cycle range without implying a
//! dissociation or equilibrium-combustion model.

include!("turbofan_physics_parts/part_01.rs");
include!("turbofan_physics_parts/part_02.rs");

#[cfg(test)]
// Test fixtures assert successful construction through unwrap.
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn dry_air_properties_change_physically_with_temperature() {
        let cold = gas_properties(288.15, GasComposition::DRY_AIR).unwrap();
        let hot = gas_properties(1_800.0, GasComposition::DRY_AIR).unwrap();
        assert!((1_000.0..1_010.0).contains(&cold.specific_heat_cp_j_kg_k));
        assert!(hot.specific_heat_cp_j_kg_k > cold.specific_heat_cp_j_kg_k);
        assert!(hot.gamma < cold.gamma);
        assert!((286.0..289.0).contains(&cold.gas_constant_j_kg_k));
    }

    #[test]
    fn nasa_property_branches_are_continuous_at_one_thousand_kelvin() {
        for composition in [
            GasComposition::DRY_AIR,
            GasComposition::LEAN_COMBUSTION_PRODUCTS,
        ] {
            let below = gas_properties(1_000.0 - 1.0e-6, composition).unwrap();
            let above = gas_properties(1_000.0, composition).unwrap();
            assert!((above.specific_heat_cp_j_kg_k - below.specific_heat_cp_j_kg_k).abs() < 1.0e-3);
            assert!((above.specific_enthalpy_j_kg - below.specific_enthalpy_j_kg).abs() < 1.0e-2);
        }
    }

    #[test]
    fn inlet_pressure_loss_does_not_create_total_temperature_change() {
        let inlet = adiabatic_inlet(
            TotalState {
                temperature_k: 310.0,
                pressure_pa: 160_000.0,
            },
            0.98,
        )
        .unwrap();
        assert_eq!(inlet.temperature_k, 310.0);
        assert_eq!(inlet.pressure_pa, 156_800.0);
    }

    #[test]
    fn nozzle_closes_pressure_when_unchoked() {
        let nozzle = convergent_nozzle(
            TotalState {
                temperature_k: 500.0,
                pressure_pa: 120_000.0,
            },
            101_325.0,
            10.0,
            GasComposition::DRY_AIR,
        )
        .unwrap();
        assert_eq!(nozzle.regime, NozzleRegime::Unchoked);
        assert!((nozzle.exit_pressure_pa - 101_325.0).abs() < 1.0e-8);
        assert!(nozzle.exit_mach < 1.0);
        assert!(nozzle.pressure_thrust_n.abs() < 1.0e-8);
        assert!(nozzle.energy_residual_w.abs() < 1.0e-7);
        assert!(nozzle.mass_residual_kg_s.abs() < 1.0e-12);
    }

    #[test]
    fn convergent_nozzle_chokes_and_retains_pressure_thrust() {
        let nozzle = convergent_nozzle(
            TotalState {
                temperature_k: 1_200.0,
                pressure_pa: 400_000.0,
            },
            101_325.0,
            25.0,
            GasComposition::LEAN_COMBUSTION_PRODUCTS,
        )
        .unwrap();
        assert_eq!(nozzle.regime, NozzleRegime::Choked);
        assert!((nozzle.exit_mach - 1.0).abs() < 1.0e-8);
        assert!(nozzle.exit_pressure_pa > 101_325.0);
        assert!(nozzle.pressure_thrust_n > 0.0);
        assert!(nozzle.energy_residual_w.abs() < 1.0e-6);
    }

    #[test]
    fn static_thrust_is_finite_and_includes_fuel_mass() {
        let balance = net_thrust(
            0.0,
            1.0,
            &[StreamThrust {
                inlet_air_mass_flow_kg_s: 100.0,
                exit_mass_flow_kg_s: 101.0,
                exit_velocity_m_s: 500.0,
                exit_pressure_pa: 0.0,
                ambient_pressure_pa: 101_325.0,
                exit_area_m2: 1.0,
            }],
        );
        assert!(matches!(balance, Err(PhysicsError::OutOfRange { .. })));

        let balance = net_thrust(
            0.0,
            1.0,
            &[StreamThrust {
                inlet_air_mass_flow_kg_s: 100.0,
                exit_mass_flow_kg_s: 101.0,
                exit_velocity_m_s: 500.0,
                exit_pressure_pa: 101_325.0,
                ambient_pressure_pa: 101_325.0,
                exit_area_m2: 1.0,
            }],
        )
        .unwrap();
        assert_eq!(balance.ram_drag_n, 0.0);
        assert_eq!(balance.mass_residual_kg_s, 0.0);
        assert!(balance.net_thrust_n.is_finite());
    }

    #[test]
    fn rejects_out_of_domain_hot_gas_temperature() {
        assert!(matches!(
            gas_properties(2_100.0, GasComposition::DRY_AIR),
            Err(PhysicsError::OutOfRange { .. })
        ));
    }

    #[test]
    fn combustor_balance_returns_a_transport_engine_order_fuel_ratio() {
        let ratio = combustor_fuel_air_ratio(
            800.0,
            1_600.0,
            0.99,
            43.0e6,
            GasComposition::DRY_AIR,
            GasComposition::LEAN_COMBUSTION_PRODUCTS,
        )
        .unwrap();
        assert!((0.015..0.050).contains(&ratio), "fuel/air ratio = {ratio}");
    }
}
