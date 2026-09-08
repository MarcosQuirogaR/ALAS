// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Technology-neutral propulsion-system contracts.
//!
//! This module is the boundary between aircraft analyses and propulsion
//! technology physics. Consumers request a system operating point and receive
//! forces, moments, resource flows, power, limits, validity and provenance;
//! they do not reconstruct thrust or fuel burn from catalogue fields.
//! Technology models retain their own internal equations. The initial
//! [`LegacyTurbofanModel`] is deliberately a lossless adapter around
//! [`crate::mission_turbofan`] so consumers can migrate before its physics is
//! replaced.

include!("system_parts/part_01.rs");
include!("system_parts/part_02.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission_turbofan::{size_turbofan, PartPowerModel};

    fn fixture() -> Result<(LegacyTurbofanModel, Freestream, ThrustOutput), PropulsionError> {
        let inputs = TurbofanInputs {
            number_of_engines: 2.0,
            bypass_ratio: 10.0,
            overall_pressure_ratio: 60.0,
            fan_pressure_ratio: 1.45,
            turbine_inlet_temperature_k: 1670.0,
            cruise_mach: 0.84,
            cruise_altitude_m: 11887.2,
            design_thrust_total_n: 197136.97010079,
        };
        let params = VehicleBuilderParams {
            part_power_model: PartPowerModel::LegacyLinear,
            ..VehicleBuilderParams::default()
        };
        let sized = size_turbofan(&inputs, &params);
        let flight = sized.sea_level_static.freestream;
        let raw = evaluate_thrust(
            &flight,
            &inputs,
            &params,
            sized.compressor_nondimensional_massflow,
            0.63,
        );
        let model = LegacyTurbofanModel::new(
            inputs,
            params,
            sized.compressor_nondimensional_massflow,
            ModelProvenance {
                model: ModelIdentity {
                    family: "legacy-turbofan".to_owned(),
                    version: "1".to_owned(),
                },
                dataset: Some("regression fixture".to_owned()),
                sources: Vec::new(),
            },
            Vec::new(),
            PropulsionInstallation {
                unit_positions_m: vec![[0.0, -5.0, 0.0], [0.0, 5.0, 0.0]],
                thrust_axes_body: vec![[1.0, 0.0, 0.0]; 2],
                nacelle_wetted_area_m2: None,
                frontal_area_m2: None,
            },
        )?;
        Ok((model, flight, raw))
    }

    #[test]
    fn legacy_adapter_is_bit_exact_for_force_and_fuel() -> Result<(), PropulsionError> {
        let (model, flight, raw) = fixture()?;
        let result = model.evaluate(&PropulsionRequest {
            flight: (&flight).into(),
            demand: PropulsionDemand::NormalizedForce(0.63),
            mode: OperatingMode::Normal,
            failure: FailureState::None,
            loads: PropulsionLoads::default(),
            state: PropulsionState::default(),
            time_step_s: None,
        })?;
        assert_eq!(result.body_force_n, [raw.thrust_n, 0.0, 0.0]);
        assert_eq!(
            result.resource_flows[0].mass_flow_kg_s,
            Some(raw.fuel_flow_rate_kg_s)
        );
        assert_eq!(
            result.state_derivatives[0].rate_per_s,
            -raw.fuel_flow_rate_kg_s
        );
        assert_eq!(result.trace, Some(TechnologyTrace::LegacyTurbofan(raw)));
        assert_eq!(
            result.achieved_demand,
            PropulsionDemand::NormalizedForce(0.63)
        );
        assert_eq!(result.torque_nm, None);
        assert_eq!(result.rotational_speed_rpm, None);
        assert_eq!(
            result.validity,
            ValidityStatus::Extrapolated {
                reason: LEGACY_VALIDITY_REASON.to_owned()
            }
        );
        Ok(())
    }

    #[test]
    fn orchestrator_preserves_capability_and_metadata() -> Result<(), PropulsionError> {
        let (model, flight, _) = fixture()?;
        let orchestrator = PropulsionOrchestrator::new(model);
        let capability = orchestrator.capability((&flight).into(), FailureState::None)?;
        assert!(capability.maximum_body_force_n[0] > capability.minimum_body_force_n[0]);
        assert_eq!(orchestrator.installation().unit_positions_m.len(), 2);
        assert_eq!(orchestrator.provenance().model.family, "legacy-turbofan");
        assert_eq!(
            capability.validity,
            ValidityStatus::Extrapolated {
                reason: LEGACY_VALIDITY_REASON.to_owned()
            }
        );
        Ok(())
    }

    #[test]
    fn unsupported_semantics_are_typed_errors() -> Result<(), PropulsionError> {
        let (model, flight, _) = fixture()?;
        let result = model.evaluate(&PropulsionRequest {
            flight: (&flight).into(),
            demand: PropulsionDemand::RequiredBodyForceN([10.0, 0.0, 0.0]),
            mode: OperatingMode::Normal,
            failure: FailureState::None,
            loads: PropulsionLoads::default(),
            state: PropulsionState::default(),
            time_step_s: None,
        });
        assert!(matches!(result, Err(PropulsionError::UnsupportedDemand(_))));

        let rating = model.evaluate(&PropulsionRequest {
            flight: (&flight).into(),
            demand: PropulsionDemand::Rating(PropulsionRating::MaximumContinuous),
            mode: OperatingMode::Normal,
            failure: FailureState::None,
            loads: PropulsionLoads::default(),
            state: PropulsionState::default(),
            time_step_s: None,
        });
        assert!(matches!(rating, Err(PropulsionError::UnsupportedDemand(_))));
        Ok(())
    }

    #[test]
    fn legacy_adapter_does_not_silently_ignore_loads_or_propeller_modes(
    ) -> Result<(), PropulsionError> {
        let (model, flight, _) = fixture()?;
        let loaded = model.evaluate(&PropulsionRequest {
            flight: (&flight).into(),
            demand: PropulsionDemand::NormalizedForce(0.5),
            mode: OperatingMode::Normal,
            failure: FailureState::None,
            loads: PropulsionLoads {
                electrical_power_w: 10_000.0,
                ..PropulsionLoads::default()
            },
            state: PropulsionState::default(),
            time_step_s: None,
        });
        assert!(matches!(loaded, Err(PropulsionError::UnsupportedDemand(_))));

        let feathered = model.evaluate(&PropulsionRequest {
            flight: (&flight).into(),
            demand: PropulsionDemand::NormalizedForce(0.0),
            mode: OperatingMode::Feathered,
            failure: FailureState::None,
            loads: PropulsionLoads::default(),
            state: PropulsionState::default(),
            time_step_s: None,
        });
        assert_eq!(
            feathered,
            Err(PropulsionError::UnsupportedMode(OperatingMode::Feathered))
        );
        Ok(())
    }

    #[test]
    fn diagnostics_make_legacy_limitations_visible() -> Result<(), PropulsionError> {
        let (model, _, _) = fixture()?;
        let diagnostics = model.diagnostics(DiagnosticsQuery::SupportedSemantics);
        assert_eq!(diagnostics.items[0].code, "legacy-turbofan-adapter");
        assert!(diagnostics.items[0]
            .message
            .contains("zero aircraft-service"));
        let validity = model.diagnostics(DiagnosticsQuery::ValidityDomain);
        assert_eq!(validity.items[0].message, LEGACY_VALIDITY_REASON);
        Ok(())
    }

    #[test]
    fn normalized_force_is_strictly_bounded() -> Result<(), PropulsionError> {
        let (model, flight, _) = fixture()?;
        for demand in [-0.01, 1.01, f64::NAN] {
            let result = model.evaluate(&PropulsionRequest {
                flight: (&flight).into(),
                demand: PropulsionDemand::NormalizedForce(demand),
                mode: OperatingMode::Normal,
                failure: FailureState::None,
                loads: PropulsionLoads::default(),
                state: PropulsionState::default(),
                time_step_s: None,
            });
            assert!(matches!(result, Err(PropulsionError::InvalidInput { .. })));
        }
        Ok(())
    }

    #[test]
    fn legacy_constructor_rejects_nonrepresentable_installations() -> Result<(), PropulsionError> {
        let inputs = TurbofanInputs {
            number_of_engines: 2.0,
            bypass_ratio: 10.0,
            overall_pressure_ratio: 60.0,
            fan_pressure_ratio: 1.45,
            turbine_inlet_temperature_k: 1670.0,
            cruise_mach: 0.84,
            cruise_altitude_m: 11887.2,
            design_thrust_total_n: 197136.97010079,
        };
        let provenance = ModelProvenance {
            model: ModelIdentity {
                family: "legacy-turbofan".to_owned(),
                version: "1".to_owned(),
            },
            dataset: None,
            sources: Vec::new(),
        };
        let asymmetric = LegacyTurbofanModel::new(
            inputs,
            VehicleBuilderParams::reference_compatibility(),
            1.0,
            provenance.clone(),
            Vec::new(),
            PropulsionInstallation {
                unit_positions_m: vec![[0.0, -4.0, 0.0], [0.0, 5.0, 0.0]],
                thrust_axes_body: vec![[1.0, 0.0, 0.0]; 2],
                nacelle_wetted_area_m2: None,
                frontal_area_m2: None,
            },
        );
        assert!(matches!(
            asymmetric,
            Err(PropulsionError::InvalidInstallation(_))
        ));

        let tilted = LegacyTurbofanModel::new(
            inputs,
            VehicleBuilderParams::reference_compatibility(),
            1.0,
            provenance,
            Vec::new(),
            PropulsionInstallation {
                unit_positions_m: vec![[0.0, -5.0, 0.0], [0.0, 5.0, 0.0]],
                thrust_axes_body: vec![[1.0, 0.0, 0.0], [0.99, 0.01, 0.0]],
                nacelle_wetted_area_m2: None,
                frontal_area_m2: None,
            },
        );
        assert!(matches!(
            tilted,
            Err(PropulsionError::InvalidInstallation(_))
        ));
        Ok(())
    }
}
