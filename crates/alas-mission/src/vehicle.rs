// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from the legacy vehicle-request integration (`build_vehicle_request`,
// `_cruise_thrust_kn_per_engine`).
// Reference: alas @ rust-port-baseline.

//! The vehicle half of the mission request.
//!
//! Where [`crate::profile`] is the route and the flown schedule, this half is
//! the aeroplane: the design vector it was optimized to, its geometry, its mass
//! breakdown, its engine cycle and the certification requirements it was sized
//! against. Most of it is read straight off a green `alas-config`; the one
//! computed quantity is the per-engine cruise thrust from mission turbofan sizing
//! wants, which is derived from this aircraft's own trimmed cruise lift-to-drag
//! and its maximum takeoff weight.
//!
//! Scope. Upstream reads five things off an `AnalysisReport` -- a P10 type this
//! crate is below -- so, following the same "take the fields you read, not the
//! type" scoping `alas-perf`'s `build_vn_diagram` and `alas-aero`'s
//! `trimmed_performance` use, this builder takes a [`ReportView`] of exactly
//! those five. Two are lift-to-drag ratios that decide the cruise thrust; the
//! other three -- the design vector, the geometry summary and the component
//! masses -- are report dictionaries with no native type until P10, so they
//! pass through as [`serde_json::Value`] unchanged, exactly as the reference
//! carries them through unread.

use alas_config::AlasConfig;
use serde::Serialize;
use serde_json::Value;

/// The five fields [`build_vehicle_request`] reads off the P10 `AnalysisReport`.
///
/// The two lift-to-drag ratios are `Some` when the corresponding design point
/// exists and carry its `l_over_d`; `None` when it does not, which is how the
/// reference's `report.trimmed_design_point or report.design_point` fallback is
/// reproduced here. The three [`Value`] fields are report dictionaries carried
/// through unchanged.
#[derive(Debug, Clone, PartialEq)]
pub struct ReportView {
    /// `trimmed_design_point.l_over_d`, or `None` if there is no trimmed point.
    pub trimmed_l_over_d: Option<f64>,
    /// `design_point.l_over_d`, or `None` if there is no design point.
    pub plain_l_over_d: Option<f64>,
    /// `report.design` as a dictionary, carried through as `design_vector`.
    pub design_vector: Value,
    /// `report.geometry_summary`, carried through unchanged.
    pub geometry_summary: Value,
    /// `report.component_masses`, carried through as `component_masses_kg`.
    pub component_masses: Value,
}

/// The engine cycle block of the vehicle request.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EngineRequest {
    /// How many engines the aircraft carries.
    pub n_engines: usize,
    /// Sea-level static rated thrust per engine, in kilonewtons.
    pub thrust_kn: f64,
    /// Per-engine thrust at the cruise design point, or `None` when no trimmed
    /// or untrimmed design point was available to derive it from.
    pub cruise_thrust_kn: Option<f64>,
    /// Engine bypass ratio.
    pub bypass_ratio: f64,
    /// Nacelle length, in metres.
    pub nacelle_length_m: f64,
    /// Nacelle maximum radius, in metres.
    pub nacelle_max_radius_m: f64,
    /// Overall pressure ratio.
    pub overall_pressure_ratio: f64,
    /// Turbine inlet temperature, in kelvin.
    pub turbine_inlet_temp_k: f64,
    /// Fan pressure ratio.
    pub fan_pressure_ratio: f64,
}

/// The certification-requirements block of the vehicle request.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RequirementsRequest {
    /// The certification aircraft type.
    pub aircraft_type: String,
    /// Design passenger count.
    pub num_passengers: i64,
    /// Design cruise Mach number.
    pub cruise_mach: f64,
    /// Design cruise altitude, in metres.
    pub cruise_altitude_m: f64,
    /// Ultimate load factor.
    pub ultimate_load_factor: f64,
}

/// The vehicle half of the request handed to the segment network.
///
/// The field names are the JSON keys the reference's subprocess boundary used.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VehicleRequest {
    /// The preset name, or `"ALAS_Design"` when the design is not a preset.
    pub name: String,
    /// The design vector, carried through from the report.
    pub design_vector: Value,
    /// The geometry summary, carried through from the report.
    pub geometry_summary: Value,
    /// The whole geometry configuration.
    pub geometry_config: alas_config::GeometryConfig,
    /// Maximum takeoff weight, in kilograms.
    pub mtow_kg: f64,
    /// The component mass breakdown, carried through from the report.
    pub component_masses_kg: Value,
    /// The engine cycle block.
    pub engine: EngineRequest,
    /// The certification-requirements block.
    pub requirements: RequirementsRequest,
}

/// Per-engine thrust at the cruise design point, in kilonewtons.
///
/// Thrust required in steady level flight is drag, which is weight over
/// lift-to-drag; sizing off maximum takeoff weight (the heaviest, most
/// demanding point) leaves the realistic margin a cruise-climb profile needs.
/// The trimmed design point is preferred and the untrimmed one is the fallback,
/// exactly as `report.trimmed_design_point or report.design_point`; a
/// non-positive lift-to-drag, or no design point at all, yields `None` so the
/// caller can fall back to the static rating.
fn cruise_thrust_kn_per_engine(report: &ReportView, config: &AlasConfig) -> Option<f64> {
    let l_over_d = report.trimmed_l_over_d.or(report.plain_l_over_d)?;
    if l_over_d <= 0.0 {
        return None;
    }
    let n_engines = config.geometry.engine.spanwise_positions_m.len();
    // 9.81 is hardcoded upstream rather than read from `requirements.gravity_m_s2`
    // (which holds the same value by default); reproduced as written.
    let thrust_required_n = config.requirements.mtow_kg * 9.81 / l_over_d;
    Some(thrust_required_n / n_engines as f64 / 1000.0)
}

/// Build the vehicle half of the request from a design's analysis report.
pub fn build_vehicle_request(report: &ReportView, config: &AlasConfig) -> VehicleRequest {
    let req = &config.requirements;
    let engine = &config.geometry.engine;
    let n_engines = engine.spanwise_positions_m.len();
    let cruise_thrust_kn = cruise_thrust_kn_per_engine(report, config);

    let name = if config.preset.is_empty() {
        "ALAS_Design".to_owned()
    } else {
        config.preset.clone()
    };

    VehicleRequest {
        name,
        design_vector: report.design_vector.clone(),
        geometry_summary: report.geometry_summary.clone(),
        geometry_config: config.geometry.clone(),
        mtow_kg: req.mtow_kg,
        component_masses_kg: report.component_masses.clone(),
        engine: EngineRequest {
            n_engines,
            thrust_kn: engine.thrust_kn,
            cruise_thrust_kn,
            bypass_ratio: engine.bypass_ratio,
            nacelle_length_m: engine.nacelle_length_m(),
            nacelle_max_radius_m: engine.radius_scale_m,
            overall_pressure_ratio: engine.overall_pressure_ratio,
            turbine_inlet_temp_k: engine.turbine_inlet_temp_k,
            fan_pressure_ratio: engine.fan_pressure_ratio,
        },
        requirements: RequirementsRequest {
            aircraft_type: req.aircraft_type.clone(),
            num_passengers: req.num_passengers,
            cruise_mach: req.cruise_mach,
            cruise_altitude_m: req.cruise_altitude_m,
            ultimate_load_factor: req.ultimate_load_factor,
        },
    }
}

/// Build the vehicle request with the frozen reference geometry schema.
///
/// The current product configuration carries optional transport-wing stations
/// for the side-of-body and Yehudi kink. The historical mission subprocess
/// request predates those fields, so its compatibility document omits them
/// while preserving every legacy geometry value. Product callers should use
/// [`build_vehicle_request`] and retain the configured stations.
pub fn build_vehicle_request_reference_compatibility(
    report: &ReportView,
    config: &AlasConfig,
) -> VehicleRequest {
    let mut compatibility_config = config.clone();
    compatibility_config
        .geometry
        .wing
        .side_of_body_span_fraction = None;
    compatibility_config.geometry.wing.kink_span_fraction = None;
    build_vehicle_request(report, &compatibility_config)
}

// A test asserts on values it constructed here, so a failed unwrap is the
// assertion failing rather than a library invariant being broken.
#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn report(trimmed: Option<f64>, plain: Option<f64>) -> ReportView {
        ReportView {
            trimmed_l_over_d: trimmed,
            plain_l_over_d: plain,
            design_vector: json!({}),
            geometry_summary: json!({}),
            component_masses: json!({}),
        }
    }

    // The trimmed point is preferred and the untrimmed one is the fallback --
    // and a present-but-nonpositive trimmed ratio does *not* fall through to the
    // untrimmed one, matching `trimmed or plain` on the point rather than on its
    // ratio.
    #[test]
    fn cruise_thrust_prefers_the_trimmed_ratio_without_falling_through_on_a_bad_one() {
        let config = AlasConfig::default();

        let with_trim = cruise_thrust_kn_per_engine(&report(Some(18.0), Some(20.0)), &config);
        let without_trim = cruise_thrust_kn_per_engine(&report(None, Some(20.0)), &config);
        assert!(with_trim.unwrap() > without_trim.unwrap());

        // A non-positive trimmed ratio returns None even though the untrimmed
        // ratio is healthy.
        assert_eq!(
            cruise_thrust_kn_per_engine(&report(Some(0.0), Some(20.0)), &config),
            None
        );
        assert_eq!(
            cruise_thrust_kn_per_engine(&report(None, None), &config),
            None
        );
    }

    // A design with no preset is named ALAS_Design; the passthrough dictionaries
    // are carried through byte for byte.
    #[test]
    fn nameless_design_and_passthrough_dictionaries() {
        let config = AlasConfig::default();
        let view = ReportView {
            trimmed_l_over_d: Some(19.0),
            plain_l_over_d: Some(18.0),
            design_vector: json!({"sweep_deg": 25.0}),
            geometry_summary: json!({"span_m": 60.0}),
            component_masses: json!({"wing": 12000.0}),
        };
        let request = build_vehicle_request(&view, &config);
        assert_eq!(request.name, "ALAS_Design");
        assert_eq!(request.design_vector, json!({"sweep_deg": 25.0}));
        assert_eq!(request.component_masses_kg, json!({"wing": 12000.0}));
        assert_eq!(
            request.engine.n_engines,
            config.geometry.engine.spanwise_positions_m.len()
        );
    }

    #[test]
    fn reference_vehicle_request_omits_new_transport_station_fields() {
        let config = AlasConfig::default();
        let view = report(Some(19.0), Some(18.0));

        let product = build_vehicle_request(&view, &config);
        let reference = build_vehicle_request_reference_compatibility(&view, &config);

        assert!(product
            .geometry_config
            .wing
            .side_of_body_span_fraction
            .is_some());
        assert!(product.geometry_config.wing.kink_span_fraction.is_some());
        assert!(reference
            .geometry_config
            .wing
            .side_of_body_span_fraction
            .is_none());
        assert!(reference.geometry_config.wing.kink_span_fraction.is_none());
    }
}
