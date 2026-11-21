// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The most aft CG the aircraft can reach on the ground, for the tip-back
//! check of a gear layout.

use alas_opt::envelope::ModelCgEnvelopeAssessment;

use super::operational_envelope::OperationalEnvelopeAssessment;
use crate::full_analysis::AnalysisReport;

/// The most aft CG the aircraft can reach on the ground and where it comes
/// from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReachableGroundCg {
    /// Station, m.
    pub x_m: f64,
    /// The same station in percent of the model MAC frame (`plane.mac_frame()`).
    pub pct_mac: f64,
    /// Which candidate governs.
    pub kind: &'static str,
    /// Aircraft mass of the governing point, kg, when it has one.
    pub mass_kg: Option<f64>,
    /// The CG height above the ground plane the tip-back angle is judged with,
    /// m: the analysed takeoff state's `h_cg_m` of the CG envelope assessment
    /// (the ground-referenced basis of the envelope's own tip-back limits).
    pub cg_height_m: Option<f64>,
}

/// The most aft CG the aircraft can reach on the ground: the largest of the
/// analysed takeoff CG, every operational-envelope point (potato extreme,
/// fuel vector, landing) and, whenever the potato is empty (no loading
/// sequence for the preset) or no envelope exists, the model's own worst aft
/// limit, so that payload loading no sequence can exercise is bounded
/// conservatively. Tip-back is judged here; the per-state limits of
/// [`assess_operational_envelope`] use the same functions
/// (`alas_perf::landing_gear::geometry::{required_tip_back_deg,
/// tip_back_limit_x_m}`) and the same height basis.
#[must_use]
pub fn most_aft_reachable_ground_cg(
    envelope: Option<&OperationalEnvelopeAssessment>,
    model_cg: &ModelCgEnvelopeAssessment,
    mac_frame: alas_geom::aircraft::mac_frame::MacFrame,
    analysed_cg_x_m: f64,
) -> ReachableGroundCg {
    use alas_opt::envelope::ModelCgLoadingState;
    let takeoff = model_cg
        .loading_states
        .iter()
        .find(|state| state.state == ModelCgLoadingState::AnalyzedTakeoff);
    let mut best = (
        analysed_cg_x_m,
        "analysed takeoff CG",
        takeoff.map(|s| s.mass_kg),
    );
    let mut consider = |x: f64, kind: &'static str, mass: Option<f64>| {
        if x.is_finite() && x > best.0 {
            best = (x, kind, mass);
        }
    };
    if let Some(envelope) = envelope {
        for (kind, points) in [
            ("potato extreme", envelope.potato_checks.as_slice()),
            ("fuel-vector point", envelope.fuel_vector_checks.as_slice()),
            ("landing point", envelope.landing_check.as_slice()),
        ] {
            for point in points {
                consider(point.x_m, kind, Some(point.mass_kg));
            }
        }
    }
    if envelope.is_none_or(|envelope| envelope.potato_checks.is_empty())
        && model_cg.worst_aft_limit_pct_mac.is_finite()
    {
        consider(
            mac_frame.x_lemac_m + model_cg.worst_aft_limit_pct_mac / 100.0 * mac_frame.chord_m,
            "model worst aft limit (no loading sequence)",
            None,
        );
    }
    ReachableGroundCg {
        x_m: best.0,
        pct_mac: mac_frame.pct_mac(best.0),
        kind: best.1,
        mass_kg: best.2,
        cg_height_m: takeoff.map(|state| state.h_cg_m).filter(|h| h.is_finite()),
    }
}

/// The tip-back basis of a gear layout from the CG envelope assessment: the
/// reachable ground CG, the analysed takeoff state's ground-referenced CG
/// height and the tail-scrape angle, plus where the CG comes from. `None` when
/// the aircraft has no MAC frame.
#[must_use]
pub fn tip_back_basis(
    report: &AnalysisReport,
    model_cg: &ModelCgEnvelopeAssessment,
    envelope: Option<&OperationalEnvelopeAssessment>,
) -> Option<(
    alas_perf::landing_gear::geometry::TipBackBasis,
    ReachableGroundCg,
)> {
    let frame = report.airplane.mac_frame()?;
    let reachable = most_aft_reachable_ground_cg(envelope, model_cg, frame, report.physical_cg[0]);
    Some((
        alas_perf::landing_gear::geometry::TipBackBasis {
            most_aft_ground_cg_x_m: Some(reachable.x_m),
            scrape_angle_deg: model_cg.scrape_angle_deg,
            cg_height_m: reachable.cg_height_m,
        },
        reachable,
    ))
}
