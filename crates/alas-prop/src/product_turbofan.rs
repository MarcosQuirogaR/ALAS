// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Physically closed Level-1 separate-flow turbofan screening model.
//!
//! This kernel is deliberately independent of the legacy mission and report
//! solvers. Compressor and turbine work use temperature-dependent enthalpy;
//! nozzles and thrust use dimensional control-volume balances that remain
//! regular at zero flight speed. Fixed component pressure ratios are an
//! on-design screening assumption, not an off-design engine deck.

include!("product_turbofan_parts/part_01.rs");
include!("product_turbofan_parts/part_02.rs");
include!("product_turbofan_parts/part_03.rs");

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn representative_config() -> ProductTurbofanConfig {
        ProductTurbofanConfig {
            bypass_ratio: 5.5,
            fan_pressure_ratio: 1.60,
            core_compressor_pressure_ratio: 18.0,
            fan_isentropic_efficiency: 0.89,
            core_compressor_isentropic_efficiency: 0.88,
            high_pressure_turbine_isentropic_efficiency: 0.91,
            low_pressure_turbine_isentropic_efficiency: 0.92,
            high_spool_mechanical_efficiency: 0.99,
            low_spool_mechanical_efficiency: 0.99,
            inlet_pressure_recovery: 0.995,
            combustor_pressure_ratio: 0.95,
            combustion_efficiency: 0.995,
            turbine_inlet_temperature_k: 1_650.0,
            fuel_lower_heating_value_j_kg: 43.0e6,
            bleed_fraction_of_core_air: 0.03,
            accessory_specific_work_j_kg_core_air: 12_000.0,
            design_ambient_temperature_k: 288.15,
            design_ambient_pressure_pa: 101_325.0,
        }
    }

    fn representative_engine() -> ProductTurbofan {
        ProductTurbofan::size_from_certified_static_thrust(
            representative_config(),
            120_000.0,
            "representative certified sea-level-static datum",
        )
        .unwrap()
    }

    #[test]
    fn true_static_sizing_has_no_mach_workaround_and_closes_thrust() {
        let engine = representative_engine();
        let result = engine.evaluate(engine.design_static_point()).unwrap();
        assert_eq!(engine.design_static_point().true_airspeed_m_s, 0.0);
        assert!((result.net_thrust_n - 120_000.0).abs() < 1.0e-6);
        assert_eq!(result.validity, CycleValidity::OnDesignScreening);
    }

    #[test]
    fn mass_energy_and_spool_balances_close() {
        let result = representative_engine()
            .evaluate(TurbofanOperatingPoint {
                ambient_temperature_k: 288.15,
                ambient_pressure_pa: 101_325.0,
                true_airspeed_m_s: 0.0,
            })
            .unwrap();
        assert!(result.residuals.mass_kg_s.abs() < 1.0e-10);
        assert!(
            result.residuals.high_spool_power_w.abs() < 1.0e-3,
            "high-spool residual = {} W",
            result.residuals.high_spool_power_w
        );
        assert!(
            result.residuals.low_spool_power_w.abs() < 1.0e-3,
            "low-spool residual = {} W",
            result.residuals.low_spool_power_w
        );
        assert!(
            result.residuals.combustor_energy_w.abs() < 1.0e-2,
            "combustor residual = {} W",
            result.residuals.combustor_energy_w
        );
        assert!(
            result.residuals.nozzle_energy_w.abs() < 1.0e-3,
            "nozzle residual = {} W",
            result.residuals.nozzle_energy_w
        );
    }

    #[test]
    fn station_temperatures_follow_cold_and_hot_section_physics() {
        let stations = representative_engine()
            .evaluate(TurbofanOperatingPoint {
                ambient_temperature_k: 288.15,
                ambient_pressure_pa: 101_325.0,
                true_airspeed_m_s: 0.0,
            })
            .unwrap()
            .stations;
        assert!(stations.inlet_k < stations.fan_exit_k);
        assert!(stations.fan_exit_k < stations.compressor_exit_k);
        assert!(stations.compressor_exit_k < stations.combustor_exit_k);
        assert!(stations.combustor_exit_k > stations.high_turbine_exit_k);
        assert!(stations.high_turbine_exit_k > stations.low_turbine_exit_k);
    }

    #[test]
    fn rejects_invalid_component_domains_and_rich_combustion() {
        let mut invalid_config = representative_config();
        invalid_config.fan_isentropic_efficiency = 1.1;
        assert!(matches!(
            ProductTurbofan::size_from_certified_static_thrust(invalid_config, 100_000.0, "test"),
            Err(ProductTurbofanError::InvalidInput { .. })
        ));
        assert!(matches!(
            complete_kerosene_products(1.0, 0.1),
            Err(ProductTurbofanError::RichCombustionUnsupported { .. })
        ));
    }

    #[test]
    fn representative_transport_cruise_is_finite_and_marked_extrapolated() {
        let result = representative_engine()
            .evaluate(TurbofanOperatingPoint {
                ambient_temperature_k: 216.65,
                ambient_pressure_pa: 22_632.0,
                true_airspeed_m_s: 230.0,
            })
            .unwrap();
        assert!(result.net_thrust_n > 0.0);
        assert!(result.cycle_fuel_mass_flow_kg_s > 0.0);
        assert!((0.01..0.06).contains(&result.fuel_air_ratio));
        assert_eq!(
            result.validity,
            CycleValidity::FixedRatioVariableAreaExtrapolation
        );
        assert_eq!(
            result.nozzle_area_treatment,
            NozzleAreaTreatment::RequiredAreaSolvedPerPoint
        );
        assert!(result
            .provenance
            .off_design_basis
            .contains("no component maps"));
    }

    #[test]
    fn nonzero_speed_bleed_stream_closes_mass_and_ram_drag() {
        let point = TurbofanOperatingPoint {
            ambient_temperature_k: 250.0,
            ambient_pressure_pa: 54_000.0,
            true_airspeed_m_s: 180.0,
        };
        let result = representative_engine().evaluate(point).unwrap();
        assert!(result.bleed_air_mass_flow_kg_s > 0.0);
        assert!(result.bleed_discharge_nozzle.is_some());
        assert!(result.residuals.mass_kg_s.abs() < 1.0e-10);
        let expected_ram_drag = result.total_air_mass_flow_kg_s * point.true_airspeed_m_s;
        assert!(
            (result.ram_drag_n - expected_ram_drag).abs() < 1.0e-8,
            "ram drag = {}, expected = {} N",
            result.ram_drag_n,
            expected_ram_drag
        );
    }

    #[test]
    fn empirical_fuel_schedule_is_explicitly_separate() {
        let engine =
            representative_engine().with_part_power_fuel_evidence(FuelFlowEvidenceAnchors {
                thrust_fractions: vec![0.07, 0.30, 0.85, 1.0],
                fuel_flows_kg_s: vec![0.10, 0.25, 0.70, 0.90],
                source: "certification evidence".into(),
            });
        let interpolated = engine
            .part_power_fuel_evidence
            .as_ref()
            .unwrap()
            .interpolate(0.50)
            .unwrap();
        assert!((0.25..0.70).contains(&interpolated));
        assert!(engine
            .provenance
            .fuel_interpolation
            .unwrap()
            .contains("empirical"));
    }
}
