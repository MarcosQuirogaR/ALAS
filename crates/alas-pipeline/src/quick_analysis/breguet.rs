// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Breguet range, achievable payload and payload-range corners for the
//! reduced analysis.
//!
//! The corner convention matches the report's payload-range figure: point A
//! carries the maximum payload with fuel to the declared MTOW, point B
//! carries the maximum fuel with the payload traded down to MTOW, point C is
//! the ferry case, and every range is a Breguet cruise at the requested
//! Mach and altitude with the analysed lift-to-drag. Reserves are not
//! deducted here, so the corners are capability envelopes, not dispatch
//! ranges.
//!
//! The maximum payload is an *estimated achievable* capacity of the fixed
//! aircraft (D07): the declared structural cap bounded by the preset's
//! MZFW-derived limit when the full analysis recorded one and by the mass
//! budget `MTOW - OEW`. Every corner therefore respects the declared MTOW,
//! and an aircraft whose empty mass reaches its MTOW gets no corners at all
//! instead of a curve that lifts negative fuel.
//!
//! The turboprop range uses the catalogue's constant maximum-cruise fuel
//! flow, which is published for the two-engine reference installation, so it
//! is scaled to the number of engines actually installed on the drawn
//! aircraft (`TurbopropEngineSpec::installed_cruise_fuel_flow_kg_h`).

use alas_atmo::Atmosphere;
use alas_config::{ActiveEngineModel, AlasConfig};
use alas_perf::performance::breguet_range_m;
use serde::{Deserialize, Serialize};

use crate::feasibility::{assess_fuel_capacity, FuelCapacityEvidence};
use crate::full_analysis::{effective_structural_payload_limit_kg, AnalysisReport};

/// A range model closed over the cruise condition and propulsion anchor.
pub struct RangeModel {
    tas_m_s: f64,
    l_over_d: f64,
    kind: RangeKind,
    /// The assumption behind the model.
    pub note: String,
}

enum RangeKind {
    Turbofan { tsfc_si: f64 },
    Turboprop { fuel_flow_kg_h: f64 },
}

impl RangeModel {
    /// The cruise range flown while the mass drops from `start_kg` to `end_kg`.
    pub fn range_m(&self, start_kg: f64, end_kg: f64) -> f64 {
        match self.kind {
            RangeKind::Turbofan { tsfc_si } => {
                breguet_range_m(self.tas_m_s, self.l_over_d, tsfc_si, start_kg, end_kg)
            }
            RangeKind::Turboprop { fuel_flow_kg_h } => {
                self.tas_m_s * 3600.0 * (start_kg - end_kg).max(0.0) / fuel_flow_kg_h
            }
        }
    }
}

/// The range model for `config` at the requested cruise condition, or `None`
/// when the propulsion model exposes no cruise fuel-flow anchor or no engine
/// is installed.
pub fn range_model(config: &AlasConfig, l_over_d: f64) -> Option<RangeModel> {
    let requirements = &config.requirements;
    let atmosphere = Atmosphere::new(requirements.cruise_altitude_m);
    let tas_m_s = requirements.cruise_mach * atmosphere.speed_of_sound();
    let gravity = requirements.gravity_m_s2;
    let installed_engines = config.geometry.engine.spanwise_positions_m.len();
    match config.geometry.engine.active_model().ok()? {
        ActiveEngineModel::Turbofan(spec) => {
            let tsfc_si = spec.cruise_tsfc_kg_kgf_hr / (gravity * 3600.0);
            (tsfc_si.is_finite() && tsfc_si > 0.0).then(|| RangeModel {
                tas_m_s,
                l_over_d,
                kind: RangeKind::Turbofan { tsfc_si },
                note: format!(
                    "catalogue cruise TSFC {:.3} kg/(kgf h)",
                    spec.cruise_tsfc_kg_kgf_hr
                ),
            })
        }
        ActiveEngineModel::Turboprop(spec) => {
            let fuel_flow_kg_h = spec.installed_cruise_fuel_flow_kg_h(installed_engines)?;
            (fuel_flow_kg_h.is_finite() && fuel_flow_kg_h > 0.0).then(|| RangeModel {
                tas_m_s,
                l_over_d,
                kind: RangeKind::Turboprop { fuel_flow_kg_h },
                note: format!(
                    "constant maximum-cruise fuel flow {fuel_flow_kg_h:.0} kg/h for {installed_engines} installed engines ({:.0} kg/h each from the two-engine catalogue anchor), no off-design deck",
                    spec.cruise_fuel_flow_per_engine_kg_h()
                ),
            })
        }
    }
}

/// The estimated achievable payload of the fixed aircraft and the bound
/// that sets it.
#[derive(Debug, Clone, PartialEq)]
pub struct PayloadCapacityEstimate {
    /// Achievable payload, kg; zero when the empty mass already reaches MTOW.
    pub capacity_kg: f64,
    /// Which bound is active.
    pub basis: &'static str,
}

/// The achievable payload of an aircraft with the given declared cap,
/// optional MZFW-derived limit, carried payload, declared MTOW and modeled
/// OEW.
///
/// The declared cap is the user's structural payload requirement; it is not
/// achievable when the mass budget `MTOW - OEW` or the preset's MZFW-derived
/// limit is smaller. A non-finite or non-positive declared cap contributes
/// no bound, and when no MZFW-derived limit exists either, the structural
/// maximum is unknown: the corner then carries the analysed payload, as the
/// report's payload-range figure does, still bounded by the mass budget.
pub fn payload_capacity_estimate(
    declared_cap_kg: f64,
    mzfw_limit_kg: Option<f64>,
    carried_payload_kg: f64,
    mtow_kg: f64,
    oew_kg: f64,
) -> PayloadCapacityEstimate {
    let positive = |value: f64| value.is_finite() && value > 0.0;
    let budget_kg = (mtow_kg - oew_kg).max(0.0);
    let mut estimate = PayloadCapacityEstimate {
        capacity_kg: budget_kg,
        basis: "MTOW less operating empty mass budget",
    };
    let mzfw_limit_kg = mzfw_limit_kg.filter(|value| positive(*value));
    if let Some(limit_kg) = mzfw_limit_kg {
        if limit_kg < estimate.capacity_kg {
            estimate = PayloadCapacityEstimate {
                capacity_kg: limit_kg,
                basis: "preset MZFW less modeled operating empty mass",
            };
        }
    }
    if positive(declared_cap_kg) {
        if declared_cap_kg < estimate.capacity_kg {
            estimate = PayloadCapacityEstimate {
                capacity_kg: declared_cap_kg,
                basis: "declared structural payload cap",
            };
        }
    } else if mzfw_limit_kg.is_none() && carried_payload_kg.max(0.0) < estimate.capacity_kg {
        estimate = PayloadCapacityEstimate {
            capacity_kg: carried_payload_kg.max(0.0),
            basis: "analysed carried payload (no structural payload cap declared)",
        };
    }
    estimate
}

/// Why the payload-range corners could not be produced.
#[derive(Debug, Clone, PartialEq)]
pub enum PayloadRangeUnavailable {
    /// The model has no anchor for this configuration (no fuel capacity,
    /// no cruise fuel-flow anchor, no engine).
    Unsupported(String),
    /// The fixed aircraft cannot lift any payload or fuel at its MTOW.
    Infeasible(String),
}

/// The payload-range corners of the reduced analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuickPayloadRange {
    /// `(range_m, payload_kg)` from the max-payload corner to the ferry corner.
    pub points: Vec<(f64, f64)>,
    /// Operating empty mass used for the corners.
    pub oew_kg: f64,
    /// The declared MTOW the corners are bounded by.
    pub mtow_kg: f64,
    /// Achievable maximum payload of point A, kg (bounded by `MTOW - OEW`).
    pub max_payload_kg: f64,
    /// Which bound set the maximum payload.
    pub payload_basis: String,
    /// Usable fuel capacity after the MTOW budget.
    pub fuel_capacity_kg: f64,
    /// Which evidence set the capacity.
    pub fuel_capacity_basis: String,
    /// The assumptions behind the ranges.
    pub note: String,
}

/// Payload-range corners from a reduced analysis report.
pub fn payload_range_corners(
    config: &AlasConfig,
    report: &AnalysisReport,
) -> Result<QuickPayloadRange, PayloadRangeUnavailable> {
    if report.airplane.wings.is_empty() {
        return Err(PayloadRangeUnavailable::Unsupported(
            "the analysed aircraft has no wing".to_owned(),
        ));
    }
    let oew_kg = super::report_oew_kg(report);
    let mtow_kg = config.requirements.mtow_kg;
    if !oew_kg.is_finite() || !mtow_kg.is_finite() || oew_kg >= mtow_kg {
        return Err(PayloadRangeUnavailable::Infeasible(format!(
            "operating empty mass {oew_kg:.0} kg is not below the declared MTOW {mtow_kg:.0} kg, so the fixed aircraft can lift neither payload nor fuel"
        )));
    }
    let mzfw_limit_kg = report
        .geometry_summary
        .get("effective_structural_payload_limit_kg")
        .copied()
        .or_else(|| effective_structural_payload_limit_kg(config, &report.design, oew_kg));
    let carried_payload_kg = report
        .component_masses
        .get("Payload")
        .copied()
        .unwrap_or(0.0);
    let payload = payload_capacity_estimate(
        config.requirements.max_structural_payload_kg,
        mzfw_limit_kg,
        carried_payload_kg,
        mtow_kg,
        oew_kg,
    );
    let max_payload_kg = payload.capacity_kg;
    let capacity = assess_fuel_capacity(config, &report.design, report);
    let Some(capacity_kg) = capacity.capacity_kg.filter(|value| value.is_finite()) else {
        return Err(PayloadRangeUnavailable::Unsupported(
            "fuel capacity unavailable for this design".to_owned(),
        ));
    };
    let mut basis = match capacity.evidence {
        FuelCapacityEvidence::PublishedPreset => "published usable capacity",
        FuelCapacityEvidence::GeometryEstimate => "geometry-estimated wing tank capacity",
        FuelCapacityEvidence::Unavailable => "unavailable",
    }
    .to_owned();
    let structural_capacity_kg = mtow_kg - oew_kg;
    let fuel_capacity_kg = if capacity_kg <= structural_capacity_kg {
        capacity_kg
    } else {
        basis = "MTOW budget".to_owned();
        structural_capacity_kg
    };
    let l_over_d = report
        .trimmed_design_point
        .as_ref()
        .map(|point| point.l_over_d)
        .unwrap_or(report.design_point.l_over_d);
    let Some(model) = range_model(config, l_over_d) else {
        return Err(PayloadRangeUnavailable::Unsupported(
            "no cruise fuel-flow anchor for the selected propulsion model or no engine installed"
                .to_owned(),
        ));
    };

    let fuel_b = fuel_capacity_kg
        .min(mtow_kg - oew_kg - max_payload_kg)
        .max(0.0);
    let payload_c = (mtow_kg - oew_kg - fuel_capacity_kg)
        .min(max_payload_kg)
        .max(0.0);
    let tow_b = oew_kg + max_payload_kg + fuel_b;
    let tow_c = oew_kg + payload_c + fuel_capacity_kg;
    let tow_d = oew_kg + fuel_capacity_kg;
    let points = vec![
        (0.0, max_payload_kg),
        (model.range_m(tow_b, tow_b - fuel_b), max_payload_kg),
        (model.range_m(tow_c, tow_c - fuel_capacity_kg), payload_c),
        (model.range_m(tow_d, oew_kg), 0.0),
    ];
    Ok(QuickPayloadRange {
        points,
        oew_kg,
        mtow_kg,
        max_payload_kg,
        payload_basis: payload.basis.to_owned(),
        fuel_capacity_kg,
        fuel_capacity_basis: basis,
        note: format!(
            "Breguet cruise at Mach {:.3}, {:.0} m, L/D {:.1}; {}; no reserves deducted",
            config.requirements.cruise_mach,
            config.requirements.cruise_altitude_m,
            l_over_d,
            model.note
        ),
    })
}

#[cfg(test)]
mod tests {
    // Tests assert on values they construct here, so a failed expect is the
    // assertion failing, not a library invariant being broken.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn the_achievable_payload_is_the_tightest_of_cap_mzfw_and_mass_budget() {
        let cap = payload_capacity_estimate(30.0, None, 10.0, 100.0, 60.0);
        assert_eq!(cap.capacity_kg, 30.0);
        assert_eq!(cap.basis, "declared structural payload cap");
        let budget = payload_capacity_estimate(30.0, None, 10.0, 100.0, 80.0);
        assert_eq!(budget.capacity_kg, 20.0);
        assert_eq!(budget.basis, "MTOW less operating empty mass budget");
        let mzfw = payload_capacity_estimate(30.0, Some(25.0), 10.0, 100.0, 60.0);
        assert_eq!(mzfw.capacity_kg, 25.0);
        assert_eq!(mzfw.basis, "preset MZFW less modeled operating empty mass");
        let impossible = payload_capacity_estimate(30.0, Some(25.0), 10.0, 100.0, 120.0);
        assert_eq!(impossible.capacity_kg, 0.0);
        assert_eq!(impossible.basis, "MTOW less operating empty mass budget");
        // No cap and no MZFW limit: the structural maximum is unknown, so the
        // analysed payload stands in, bounded by the budget.
        let no_cap = payload_capacity_estimate(f64::NAN, None, 10.0, 100.0, 60.0);
        assert_eq!(no_cap.capacity_kg, 10.0);
        assert!(no_cap.basis.starts_with("analysed carried payload"));
        let heavy_load = payload_capacity_estimate(0.0, None, 55.0, 100.0, 60.0);
        assert_eq!(heavy_load.capacity_kg, 40.0);
        assert_eq!(heavy_load.basis, "MTOW less operating empty mass budget");
        // A non-finite MZFW limit is no limit, but a zero cap with a known
        // MZFW limit keeps the MZFW bound rather than the carried payload.
        let zero_cap = payload_capacity_estimate(0.0, Some(f64::INFINITY), 10.0, 100.0, 60.0);
        assert_eq!(zero_cap.capacity_kg, 10.0);
        let mzfw_only = payload_capacity_estimate(0.0, Some(35.0), 10.0, 100.0, 60.0);
        assert_eq!(mzfw_only.capacity_kg, 35.0);
    }

    fn atr_config(installed_engines: usize) -> AlasConfig {
        let mut config = AlasConfig::from_value(&serde_json::json!({"preset": "ATR72-600"}))
            .expect("the ATR preset loads");
        let engine = &mut config.geometry.engine;
        assert!(matches!(
            engine.active_model().expect("bound"),
            ActiveEngineModel::Turboprop(_)
        ));
        engine.spanwise_positions_m = (0..installed_engines)
            .map(|index| 4.0 + index as f64 * 2.5)
            .collect();
        config
    }

    #[test]
    fn the_turboprop_range_scales_inversely_with_the_installed_engine_count() {
        let two = range_model(&atr_config(2), 15.0).expect("two-engine anchor");
        let four = range_model(&atr_config(4), 15.0).expect("four-engine anchor");
        let range_two = two.range_m(22_000.0, 20_000.0);
        let range_four = four.range_m(22_000.0, 20_000.0);
        assert!(range_two > 0.0);
        assert!(
            (range_four - range_two / 2.0).abs() < 1e-9 * range_two,
            "four engines burn twice the flow: {range_four} vs {range_two}"
        );
        assert!(two.note.contains("2 installed engines"));
        assert!(four.note.contains("4 installed engines"));
        let spec = match atr_config(2).geometry.engine.active_model().unwrap() {
            ActiveEngineModel::Turboprop(spec) => spec.maximum_cruise_fuel_flow_kg_h,
            ActiveEngineModel::Turbofan(_) => unreachable!(),
        };
        let expected_two = two.tas_m_s * 3600.0 * 2000.0 / spec;
        assert!((range_two - expected_two).abs() < 1e-9 * range_two);
    }

    #[test]
    fn an_aircraft_without_engines_has_no_turboprop_range_model() {
        assert!(range_model(&atr_config(0), 15.0).is_none());
    }
}
