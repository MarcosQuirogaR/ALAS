// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The load plan of a search candidate: the balance rows judge the loading
//! the report judges.
//!
//! The reported analysis places the hold and cabin load so the loaded states
//! sit inside their CG windows (`alas_pipeline::full_analysis::load_plan`,
//! method and sources in [`alas_payload::load_plan`]). The search used to
//! judge the nominal placement only (payload centroid on the empty-aircraft
//! CG), so a candidate, or the registered aircraft itself, was rejected on a
//! CG row for a loading its own report replans and accepts (C295: tip-back
//! 9.5 deg against 15 deg in the search, hard constraints passing in the
//! report after a 1.7 m forward plan).
//!
//! [`planned`] applies the report's rule to the search's ledger states: the
//! same planned states and weights, the same window gap, the same payload
//! station from [`alas_payload::load_plan::steer_payload_cg`], and the same
//! acceptance (the planned loading is kept only when it leaves the states
//! strictly less far outside their windows than the nominal one).
//!
//! The report plans the loading its gate analyses, which is the dispatched
//! route's on one aircraft (C295) and the maximum-fuel design take-off on
//! another (LJ60), and then judges both loadings on the one planned
//! placement. The balance rows gate both loadings of every candidate, so the
//! plan here reads both: it is needed when either has a state outside its
//! window, the station minimises the largest weighted deviation over the
//! states of both, and the gap compared is their sum.
//!
//! Payload and fuel masses never change. The closure's masses, trim and fuel
//! are those of the nominal placement; the finalist is verified on the
//! report's own planned analysis.
//!
//! Stations in metres aft of the nose, masses in kilograms, gaps in % MAC.

use alas_geom::aircraft::mac_frame::MacFrame;
use alas_payload::load_plan::{self as plan, PlanState};

use crate::envelope::{ModelCgEnvelopeAssessment, ModelCgLoadingAssessment, ModelCgLoadingState};

use super::relative_balance::States;

/// States a plan keeps inside their windows and their weights: the zero-fuel
/// and take-off states carry full weight, the part-fuel states half. The
/// report's list (`alas_pipeline::full_analysis::load_plan::PLANNED`).
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

fn state_at(
    envelope: &ModelCgEnvelopeAssessment,
    kind: ModelCgLoadingState,
) -> Option<&ModelCgLoadingAssessment> {
    envelope
        .loading_states
        .iter()
        .find(|state| state.state == kind)
}

/// Distance of one state outside its CG window, % MAC: negative forward of
/// the forward limit, positive aft of the aft limit, zero inside or when the
/// state has no usable window.
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

/// The planned states of both loadings: the design loading, then the flown
/// one when the route is dispatched.
fn planned_states(
    states: &States,
) -> impl Iterator<Item = (&ModelCgLoadingAssessment, &'static str, f64)> {
    std::iter::once(&states.0)
        .chain(states.1.as_ref())
        .flat_map(|envelope| {
            PLANNED.iter().filter_map(move |(kind, label, weight)| {
                state_at(envelope, *kind).map(|state| (state, *label, *weight))
            })
        })
}

/// Sum over the planned states of both loadings of the distance outside the
/// window, % MAC.
fn total_gap_pct_mac(states: &States) -> f64 {
    planned_states(states)
        .map(|(state, _, _)| gap_pct_mac(state).abs())
        .sum()
}

/// The payload station, m, the plan asks for: the planned states of both
/// loadings as plan states, with the payload mass and centroid they were
/// assessed with (the zero-fuel state less the operating-empty one, which the
/// two loadings share).
fn steer_station_m(states: &States, frame: MacFrame) -> Option<f64> {
    let empty = state_at(&states.0, ModelCgLoadingState::OperatingEmpty)?;
    let zero_fuel = state_at(&states.0, ModelCgLoadingState::AnalyzedZeroFuel)?;
    let payload_kg = zero_fuel.mass_kg - empty.mass_kg;
    if !(payload_kg.is_finite() && payload_kg > 0.0) {
        return None;
    }
    let payload_cg_x_m =
        (zero_fuel.mass_kg * zero_fuel.cg_x_m - empty.mass_kg * empty.cg_x_m) / payload_kg;
    let station = |pct: f64| frame.x_lemac_m + pct / 100.0 * frame.chord_m;
    let plan_states: Vec<PlanState> = planned_states(states)
        .map(|(state, label, weight)| {
            let limits = &state.physical_limits;
            let window = plan::usable_window(
                station(limits.fwd_limit_pct_mac),
                station(limits.aft_limit_pct_mac),
            );
            PlanState {
                label,
                mass_kg: state.mass_kg,
                other_moment_kg_m: state.mass_kg * state.cg_x_m - payload_kg * payload_cg_x_m,
                window_x_m: window,
                target_x_m: window.map_or(state.cg_x_m, plan::window_midpoint_m),
                weight,
            }
        })
        .collect();
    plan::steer_payload_cg(payload_kg, &plan_states).filter(|x| x.is_finite())
}

/// `nominal`, or the states of the planned loading when a plan is needed and
/// improves the fit.
///
/// `assess_at` assesses the candidate with its payload steered toward a
/// station, m. A planned loading that cannot be assessed keeps the nominal
/// one, as does a plan that does not reduce the total gap.
pub(super) fn planned(
    nominal: States,
    frame: Option<MacFrame>,
    assess_at: impl FnOnce(f64) -> Result<States, String>,
) -> States {
    let nominal_gap = total_gap_pct_mac(&nominal);
    if nominal_gap == 0.0 {
        return nominal;
    }
    let Some(steer_x_m) = frame.and_then(|frame| steer_station_m(&nominal, frame)) else {
        return nominal;
    };
    match assess_at(steer_x_m) {
        Ok(candidate) if total_gap_pct_mac(&candidate) < nominal_gap => candidate,
        Ok(_) => nominal,
        Err(error) => {
            tracing::debug!(%error, "planned loading not assessable; nominal placement kept");
            nominal
        }
    }
}
