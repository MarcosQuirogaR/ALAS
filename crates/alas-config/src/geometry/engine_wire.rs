// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Serialization bridge for legacy flat engine thrust documents.

use super::*;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EngineConfigWire {
    engine_name: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "is_default_propulsion_technology")]
    propulsion_technology: PropulsionTechnology,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    turbofan: Option<TurbofanEngineSpec>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    turboprop: Option<TurbopropEngineSpec>,
    nacelle_profile: Vec<(f64, f64)>,
    radius_scale_m: f64,
    spanwise_positions_m: Vec<f64>,
    z_m: f64,
    inlet_x_offset_m: f64,
    #[serde(default)]
    thrust_kn: Option<f64>,
    bypass_ratio: f64,
    overall_pressure_ratio: f64,
    fan_pressure_ratio: f64,
    turbine_inlet_temp_k: f64,
    cruise_tsfc_kg_kgf_hr: f64,
    fan_diameter_m: f64,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    part_power_fuel_flow_ratios: Vec<f64>,
    #[serde(default)]
    #[serde(skip_serializing_if = "String::is_empty")]
    part_power_source: String,
}

impl From<EngineConfig> for EngineConfigWire {
    fn from(value: EngineConfig) -> Self {
        let thrust_kn = Some(value.thrust_kn());
        Self {
            engine_name: value.engine_name,
            propulsion_technology: value.propulsion_technology,
            turbofan: value.turbofan,
            turboprop: value.turboprop,
            nacelle_profile: value.nacelle_profile,
            radius_scale_m: value.radius_scale_m,
            spanwise_positions_m: value.spanwise_positions_m,
            z_m: value.z_m,
            inlet_x_offset_m: value.inlet_x_offset_m,
            thrust_kn,
            bypass_ratio: value.bypass_ratio,
            overall_pressure_ratio: value.overall_pressure_ratio,
            fan_pressure_ratio: value.fan_pressure_ratio,
            turbine_inlet_temp_k: value.turbine_inlet_temp_k,
            cruise_tsfc_kg_kgf_hr: value.cruise_tsfc_kg_kgf_hr,
            fan_diameter_m: value.fan_diameter_m,
            part_power_fuel_flow_ratios: value.part_power_fuel_flow_ratios,
            part_power_source: value.part_power_source,
        }
    }
}

impl TryFrom<EngineConfigWire> for EngineConfig {
    type Error = String;

    fn try_from(value: EngineConfigWire) -> Result<Self, Self::Error> {
        let legacy_thrust = value.thrust_kn;
        let has_typed_payload = value.turbofan.is_some() || value.turboprop.is_some();
        let mut config = Self {
            engine_name: value.engine_name,
            propulsion_technology: value.propulsion_technology,
            turbofan: value.turbofan,
            turboprop: value.turboprop,
            nacelle_profile: value.nacelle_profile,
            radius_scale_m: value.radius_scale_m,
            spanwise_positions_m: value.spanwise_positions_m,
            z_m: value.z_m,
            inlet_x_offset_m: value.inlet_x_offset_m,
            bypass_ratio: value.bypass_ratio,
            overall_pressure_ratio: value.overall_pressure_ratio,
            fan_pressure_ratio: value.fan_pressure_ratio,
            turbine_inlet_temp_k: value.turbine_inlet_temp_k,
            cruise_tsfc_kg_kgf_hr: value.cruise_tsfc_kg_kgf_hr,
            fan_diameter_m: value.fan_diameter_m,
            part_power_fuel_flow_ratios: value.part_power_fuel_flow_ratios,
            part_power_source: value.part_power_source,
        };
        // Typed saved configurations already have an authoritative rating;
        // their deprecated flat mirror may be stale. Only legacy-only files
        // migrate the flat rating into a freshly resolved typed payload.
        if !has_typed_payload {
            if let Ok(spec) = crate::engines::get(&config.engine_name) {
                match config.propulsion_technology {
                    PropulsionTechnology::Turbofan => {
                        config.turbofan = spec.turbofan_spec();
                        if let Some(payload) = config.turbofan.as_mut() {
                            if let Some(thrust) = legacy_thrust {
                                payload.rated_thrust_kn = thrust;
                            }
                        }
                    }
                    PropulsionTechnology::Turboprop => config.turboprop = spec.turboprop.clone(),
                }
            }
        }
        Ok(config)
    }
}

// Serialization tests construct known-good local fixtures.
#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_rating_is_authoritative_when_legacy_mirror_disagrees() {
        let mut original = EngineConfig::default();
        original.set_thrust_kn(401.0).unwrap();
        let mut value = serde_json::to_value(&original).unwrap();
        value["thrust_kn"] = serde_json::json!(467.0);
        let restored: EngineConfig = serde_json::from_value(value).unwrap();
        assert_eq!(restored.thrust_kn(), 401.0);
        assert_eq!(serde_json::to_value(&restored).unwrap()["thrust_kn"], 401.0);
    }

    #[test]
    fn legacy_only_rating_is_migrated_to_typed_payload() {
        let mut value = serde_json::to_value(EngineConfig::default()).unwrap();
        value.as_object_mut().unwrap().remove("turbofan");
        value["thrust_kn"] = serde_json::json!(399.0);
        let restored: EngineConfig = serde_json::from_value(value).unwrap();
        assert_eq!(restored.thrust_kn(), 399.0);
        assert_eq!(restored.turbofan.unwrap().rated_thrust_kn, 399.0);
    }

    #[test]
    fn setter_rejects_invalid_ratings_without_mutating_engine() {
        let mut engine = EngineConfig::default();
        let original = engine.clone();
        for value in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(engine.set_thrust_kn(value).is_err());
            assert_eq!(engine, original);
        }
    }

    #[test]
    fn saved_configs_reject_unknown_fields() {
        let mut value = serde_json::to_value(EngineConfig::default()).unwrap();
        value["unknown_rating"] = serde_json::json!(399.0);
        assert!(serde_json::from_value::<EngineConfig>(value).is_err());
    }
}
