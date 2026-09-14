// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Breguet range and payload-range corners for the reduced analysis.
//!
//! The corner convention matches the report's payload-range figure: point A
//! carries the maximum payload with fuel to the declared MTOW, point B
//! carries the maximum fuel with the payload traded down to MTOW, point C is
//! the ferry case, and every range is a Breguet cruise at the requested
//! Mach and altitude with the analysed lift-to-drag. Reserves are not
//! deducted here, so the corners are capability envelopes, not dispatch
//! ranges.

use alas_atmo::Atmosphere;
use alas_config::{ActiveEngineModel, AlasConfig};
use alas_perf::performance::breguet_range_m;
use serde::{Deserialize, Serialize};

use crate::feasibility::{assess_fuel_capacity, FuelCapacityEvidence};
use crate::full_analysis::AnalysisReport;

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
/// when the propulsion model exposes no cruise fuel-flow anchor.
pub fn range_model(config: &AlasConfig, l_over_d: f64) -> Option<RangeModel> {
    let requirements = &config.requirements;
    let atmosphere = Atmosphere::new(requirements.cruise_altitude_m);
    let tas_m_s = requirements.cruise_mach * atmosphere.speed_of_sound();
    let gravity = requirements.gravity_m_s2;
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
            let fuel_flow_kg_h = spec.maximum_cruise_fuel_flow_kg_h;
            (fuel_flow_kg_h.is_finite() && fuel_flow_kg_h > 0.0).then(|| RangeModel {
                tas_m_s,
                l_over_d,
                kind: RangeKind::Turboprop { fuel_flow_kg_h },
                note: format!(
                    "constant maximum-cruise fuel flow {fuel_flow_kg_h:.0} kg/h, no off-design deck"
                ),
            })
        }
    }
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
) -> Option<QuickPayloadRange> {
    report.airplane.wings.first()?;
    let oew_kg = super::report_oew_kg(report);
    let mtow_kg = config.requirements.mtow_kg;
    let effective_payload_limit = report
        .geometry_summary
        .get("effective_structural_payload_limit_kg")
        .copied()
        .filter(|value| value.is_finite() && *value > 0.0);
    let max_payload_kg = effective_payload_limit
        .map(|limit| limit.min(config.requirements.max_structural_payload_kg))
        .unwrap_or(config.requirements.max_structural_payload_kg)
        .max(0.0);
    let capacity = assess_fuel_capacity(config, &report.design, report);
    let capacity_kg = capacity.capacity_kg.filter(|value| value.is_finite())?;
    let mut basis = match capacity.evidence {
        FuelCapacityEvidence::PublishedPreset => "published usable capacity",
        FuelCapacityEvidence::GeometryEstimate => "geometry-estimated wing tank capacity",
        FuelCapacityEvidence::Unavailable => "unavailable",
    }
    .to_owned();
    let structural_capacity_kg = (mtow_kg - oew_kg).max(0.0);
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
    let model = range_model(config, l_over_d)?;

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
    Some(QuickPayloadRange {
        points,
        oew_kg,
        mtow_kg,
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
