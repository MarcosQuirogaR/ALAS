// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The load plan of an analysed aircraft: where the hold and cabin load sits
//! so the zero-fuel and take-off CG fall inside the model's CG envelope.
//!
//! The nominal payload pass steers the hold load so the payload centroid sits
//! on the empty-aircraft CG. That is a placement rule, not a load plan: it
//! knows nothing of the fuel moment or of the envelope, so a heavy fuel load
//! on a narrow envelope (B747-400 at MTOW: take-off CG 27.9 %MAC against
//! 22.4-24.2 published) reports a violation no dispatcher would load.
//! [`run_planned`] runs the nominal analysis, reads the envelope windows of the
//! loaded states, asks [`alas_payload::load_plan::steer_payload_cg`] for the
//! payload centroid that puts them nearest their targets (window midpoint, or
//! the operator's declared take-off target), places the hold and cabin load
//! there ([`build_steered_layout`]) and analyses again. Method and sources:
//! `alas_payload::load_plan`. Payload mass and fuel mass never change; the
//! second analysis is kept only when it leaves the states no further outside
//! their windows than the nominal one.
//!
//! Placement freedom used: baggage and freight by hold compartment (fore and
//! aft) and, on a freighter, the main-deck positions; the container set and
//! its tare stay those of the nominal load. Seats are filled at the
//! design load case (every booked seat occupied), so seat zones carry no
//! freedom there. Fuel stays on the declared tank sequence
//! (`alas_mass::tanks::order`, trim tank filled last), which is the AFM
//! usage order the model carries; a trim-tank fill can only move fuel aft of
//! the wing tanks, so it is no help for an aft-heavy loading.
//!
//! The plan is reported in the analysis summary under `load_plan_*` keys.

use alas_opt::envelope::{
    ModelCgEnvelopeAssessment, ModelCgLoadingAssessment, ModelCgLoadingState,
};
use alas_payload::load_plan::{self as plan, PlanState};

use super::*;

/// When a load plan replaces the nominal placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoadPlanPolicy {
    /// Nominal placement only.
    Off,
    /// Plan when the nominal loading leaves a state outside its CG window.
    #[default]
    OnViolation,
    /// Always place the load at the window targets.
    Always,
}

/// Mass-and-balance fit of one planned state, kept for the summary.
struct Fit {
    gap_pct_mac: BTreeMap<&'static str, f64>,
    /// Planned states whose envelope has no usable window (forward limit not
    /// forward of the aft limit): no placement can fit them.
    no_window: usize,
}

/// Build the payload layout with its loaded containers relocated toward the
/// payload station `steer_x_m`.
///
/// Only placement changes: `alas_payload::CargoLoadManager::relocate_same_set`
/// moves whole loaded containers to empty positions of the same container
/// type (hold compartments on a passenger aircraft, main-deck positions on a
/// freighter), so the container set, every net load and the tare, hence the
/// gross payload, are exactly the nominal layout's.
pub(super) fn build_steered_layout(
    plane: &Airplane,
    config: &AlasConfig,
    oew_kg: f64,
    x_oew_m: f64,
    steer_x_m: Option<f64>,
) -> Result<PayloadLayout, alas_payload::geometry::CabinGeometryError> {
    let Some(steer) = steer_x_m.filter(|x| x.is_finite()) else {
        return build_payload_layout(plane, config, oew_kg, x_oew_m);
    };
    let mut planned = config.clone();
    planned.cabin.cargo.plan_payload_cg_x_m = Some(steer);
    build_payload_layout(plane, &planned, oew_kg, x_oew_m)
}

fn state_at(
    env: &ModelCgEnvelopeAssessment,
    kind: ModelCgLoadingState,
) -> Option<&ModelCgLoadingAssessment> {
    env.loading_states.iter().find(|state| state.state == kind)
}

/// States a plan keeps inside their windows: the zero-fuel and take-off states
/// carry full weight, the part-fuel states half.
const PLANNED: [(ModelCgLoadingState, &str, f64); 5] = [
    (ModelCgLoadingState::AnalyzedZeroFuel, "ZFW", 1.0),
    (
        ModelCgLoadingState::OperationalMidMission,
        "mid-mission",
        0.5,
    ),
    (ModelCgLoadingState::OperationalReserve, "reserve", 0.5),
    (ModelCgLoadingState::AnalyzedTakeoff, "TOW", 1.0),
    (ModelCgLoadingState::AnalyzedLanding, "landing", 0.5),
];

fn gap_pct_mac(state: &ModelCgLoadingAssessment) -> f64 {
    let limits = &state.physical_limits;
    let (lo, hi) = (limits.fwd_limit_pct_mac, limits.aft_limit_pct_mac);
    if !(lo.is_finite() && hi.is_finite() && lo < hi) {
        0.0
    } else if state.cg_pct_mac < lo {
        state.cg_pct_mac - lo
    } else if state.cg_pct_mac > hi {
        state.cg_pct_mac - hi
    } else {
        0.0
    }
}

fn fit_of(env: &ModelCgEnvelopeAssessment) -> Fit {
    let gap_pct_mac = PLANNED
        .iter()
        .filter_map(|(kind, label, _)| state_at(env, *kind).map(|s| (*label, gap_pct_mac(s))))
        .collect();
    let no_window = PLANNED
        .iter()
        .filter_map(|(kind, _, _)| state_at(env, *kind))
        .filter(|state| {
            let limits = &state.physical_limits;
            plan::usable_window(limits.fwd_limit_pct_mac, limits.aft_limit_pct_mac).is_none()
        })
        .count();
    Fit {
        gap_pct_mac,
        no_window,
    }
}

impl Fit {
    fn total_gap(&self) -> f64 {
        self.gap_pct_mac.values().map(|gap| gap.abs()).sum()
    }
}

/// The envelope states as plan states, and the payload mass and centroid they
/// were analysed with.
fn plan_states(
    env: &ModelCgEnvelopeAssessment,
    frame: alas_geom::aircraft::mac_frame::MacFrame,
    payload_cg_x_m: f64,
    operator_tow_pct_mac: Option<f64>,
) -> Option<(f64, Vec<PlanState>)> {
    let empty = state_at(env, ModelCgLoadingState::OperatingEmpty)?;
    let zero_fuel = state_at(env, ModelCgLoadingState::AnalyzedZeroFuel)?;
    let payload_kg = zero_fuel.mass_kg - empty.mass_kg;
    let station = |pct: f64| frame.x_lemac_m + pct / 100.0 * frame.chord_m;
    let states = PLANNED
        .iter()
        .filter_map(|(kind, label, weight)| {
            let state = state_at(env, *kind)?;
            let limits = &state.physical_limits;
            let window = plan::usable_window(
                station(limits.fwd_limit_pct_mac),
                station(limits.aft_limit_pct_mac),
            );
            let target = match (kind, operator_tow_pct_mac) {
                (ModelCgLoadingState::AnalyzedTakeoff, Some(pct)) => station(pct),
                _ => window.map_or(state.cg_x_m, plan::window_midpoint_m),
            };
            Some(PlanState {
                label,
                mass_kg: state.mass_kg,
                other_moment_kg_m: state.mass_kg * state.cg_x_m - payload_kg * payload_cg_x_m,
                window_x_m: window,
                target_x_m: target,
                weight: *weight,
            })
        })
        .collect();
    Some((payload_kg, states))
}

fn annotate(
    report: &mut AnalysisReport,
    applied: bool,
    steer_x_m: Option<f64>,
    shift_m: f64,
    nominal: &Fit,
    fin: &Fit,
) {
    let summary = &mut report.geometry_summary;
    summary.insert("load_plan_applied".to_owned(), f64::from(applied));
    summary.insert(
        "load_plan_fits".to_owned(),
        f64::from(fin.no_window == 0 && fin.gap_pct_mac.values().all(|gap| *gap == 0.0)),
    );
    summary.insert(
        "load_plan_no_window_states".to_owned(),
        fin.no_window as f64,
    );
    summary.insert("load_plan_payload_cg_shift_m".to_owned(), shift_m);
    if let Some(x) = steer_x_m {
        summary.insert("load_plan_steer_payload_cg_x_m".to_owned(), x);
    }
    for (label, gap) in &fin.gap_pct_mac {
        summary.insert(format!("load_plan_gap_pct_mac_{label}"), *gap);
    }
    for (label, gap) in &nominal.gap_pct_mac {
        summary.insert(format!("load_plan_nominal_gap_pct_mac_{label}"), *gap);
    }
}

/// Run `analysis` with a load plan under its policy.
pub(super) fn run_planned(
    analysis: &FullAnalysis,
    design: &DesignVector,
    airplane: Airplane,
) -> Result<AnalysisReport, String> {
    let mut nominal = analysis.run_steered(design, airplane.clone(), None)?;
    let Some(env) = loaded_envelope(analysis, design, &nominal) else {
        return Ok(nominal);
    };
    let nominal_fit = fit_of(&env);
    let needs_plan = analysis.load_plan == LoadPlanPolicy::Always
        || nominal_fit.gap_pct_mac.values().any(|gap| *gap != 0.0);
    let frame = nominal.airplane.mac_frame();
    let payload_x = nominal
        .mass_coordinates
        .get(alas_mass::breakdown::PAYLOAD)
        .map(|c| c[0]);
    let planned = frame.zip(payload_x).and_then(|(frame, payload_x)| {
        plan_states(&env, frame, payload_x, analysis.operator_cg_target_pct_mac).and_then(
            |(payload_kg, states)| {
                plan::steer_payload_cg(payload_kg, &states).map(|x| (x, payload_x))
            },
        )
    });
    let (Some((steer_x, nominal_x)), true) = (planned, needs_plan) else {
        annotate(&mut nominal, false, None, 0.0, &nominal_fit, &nominal_fit);
        return Ok(nominal);
    };
    let mut candidate = analysis.run_steered(design, airplane, Some(steer_x))?;
    let candidate_fit = loaded_envelope(analysis, design, &candidate)
        .as_ref()
        .map(fit_of);
    let kept = candidate_fit.as_ref().is_some_and(|fit| {
        let (new, old) = (fit.total_gap(), nominal_fit.total_gap());
        new < old || (new == old && analysis.load_plan == LoadPlanPolicy::Always)
    });
    if kept {
        let fit = candidate_fit.unwrap_or_else(|| fit_of(&env));
        let achieved = candidate
            .mass_coordinates
            .get(alas_mass::breakdown::PAYLOAD)
            .map_or(nominal_x, |c| c[0]);
        annotate(
            &mut candidate,
            true,
            Some(steer_x),
            achieved - nominal_x,
            &nominal_fit,
            &fit,
        );
        Ok(candidate)
    } else {
        annotate(
            &mut nominal,
            false,
            Some(steer_x),
            0.0,
            &nominal_fit,
            &nominal_fit,
        );
        Ok(nominal)
    }
}

/// The CG envelope of `report` on the item-level mass ledger, the basis the
/// feasibility hard gate judges (`feasibility::model_cg`).
fn loaded_envelope(
    analysis: &FullAnalysis,
    design: &DesignVector,
    report: &AnalysisReport,
) -> Option<ModelCgEnvelopeAssessment> {
    crate::feasibility::assess_physical_feasibility(&analysis.config, design, report, None).model_cg
}

// Tests construct every fixture they assert on, so a failed unwrap or expect
// is the assertion failing rather than a library invariant breaking.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn report(name: &str, analysis: FullAnalysis) -> AnalysisReport {
        let design = alas_config::presets::get(name).unwrap().design_vector;
        analysis.run(&design, true).unwrap()
    }

    fn analysis(name: &str) -> FullAnalysis {
        FullAnalysis::new(AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap())
    }

    fn total_kg(report: &AnalysisReport) -> f64 {
        report.component_masses.values().sum()
    }

    /// The B747-400 at MTOW carries its fuel aft of the wing-body envelope:
    /// the nominal take-off CG is outside the window. The plan moves only the
    /// hold load, keeps the take-off mass, brings the take-off CG closer to
    /// its window and reports what it could not fit.
    #[test]
    fn the_b747_400_plan_moves_the_takeoff_cg_toward_its_window_without_changing_the_takeoff_mass()
    {
        let nominal = report("B747-400", analysis("B747-400").without_load_plan());
        let planned = report("B747-400", analysis("B747-400"));
        assert_eq!(planned.geometry_summary["load_plan_applied"], 1.0);
        let before = planned.geometry_summary["load_plan_nominal_gap_pct_mac_TOW"];
        let after = planned.geometry_summary["load_plan_gap_pct_mac_TOW"];
        assert!(
            before > 1.0,
            "the nominal take-off CG is outside its window"
        );
        assert!(after.abs() < before.abs(), "{after} against {before}");
        assert!(
            (total_kg(&planned) - total_kg(&nominal)).abs() < 1.0e-6 * total_kg(&nominal),
            "take-off mass changed"
        );
        let (p, n) = (
            planned.component_masses[alas_mass::breakdown::PAYLOAD],
            nominal.component_masses[alas_mass::breakdown::PAYLOAD],
        );
        assert!(
            (p - n).abs() < 0.01 * n,
            "payload {p} against {n}: only ULD tare may differ"
        );
        assert!(planned.physical_cg[0] < nominal.physical_cg[0]);
    }

    /// A 747-class freighter plan relocates main-deck containers only: the
    /// gross payload and the fuel are identical, the take-off CG moves forward.
    #[test]
    fn a_freighter_plan_relocates_main_deck_containers_without_changing_any_mass() {
        let config = AlasConfig::from_value(&serde_json::json!({
            "preset": "B747-400",
            "requirements": { "aircraft_type": "cargo", "cargo_payload_kg": 80000.0 }
        }))
        .unwrap();
        let design = alas_config::presets::get("B747-400").unwrap().design_vector;
        let nominal = FullAnalysis::new(config.clone())
            .without_load_plan()
            .run(&design, true)
            .unwrap();
        let planned = FullAnalysis::new(config).run(&design, true).unwrap();
        assert_eq!(planned.geometry_summary["load_plan_applied"], 1.0);
        for (key, value) in &nominal.component_masses {
            assert!((planned.component_masses[key] - value).abs() < 0.5, "{key}");
        }
        assert!(planned.physical_cg[0] < nominal.physical_cg[0]);
    }

    /// A loading already inside its envelope keeps its nominal placement.
    #[test]
    fn a_loading_inside_its_envelope_is_not_replanned() {
        let planned = report("A320-200", analysis("A320-200"));
        assert_eq!(planned.geometry_summary["load_plan_applied"], 0.0);
        assert_eq!(planned.geometry_summary["load_plan_fits"], 1.0);
    }

    /// An inverted envelope (no forward limit aft of the aft limit) has no
    /// placement that fits and the plan says so. The envelope is synthetic: the
    /// AVE is a concept with no published gear-load table, so the class steering
    /// minimum applies as set, and raising it to 19 % of the weight puts the
    /// aft ground limit (the CG that keeps that nose load) far ahead of every
    /// forward limit. The DC-10 preset used to stand here while its gear and
    /// frame were misplaced (nose gear 0.4 m forward, LEMAC 0.4 m forward); with
    /// the stations of the Boeing ACAP and NASA CR-3677 its envelope is not inverted.
    #[test]
    fn an_inverted_envelope_is_reported_as_unfittable() {
        let config = AlasConfig::from_value(&serde_json::json!({
            "preset": "AVE",
            "mass_model": { "pct_load_nlg_min": 0.19 },
        }))
        .unwrap();
        let planned = report("AVE", FullAnalysis::new(config));
        assert_eq!(planned.geometry_summary["load_plan_fits"], 0.0);
        assert!(planned.geometry_summary["load_plan_no_window_states"] > 0.0);
    }
}
