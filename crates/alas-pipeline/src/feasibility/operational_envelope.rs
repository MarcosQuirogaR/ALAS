// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Loading potato, fuel vector and the ZFW operational envelope : consumes the
//! seat-map/cargo loading sequences (`alas_payload::loading_sequence`) and
//! the fill-level-dependent fuel vector (`alas_mass::tanks::fuel_vector`),
//! and checks every point against the
//! physical CG limits [`crate::feasibility::model_cg_assessment`] already
//! placed per named loading state.
//!
//! Weight-dependent aft limits (tip-back) are interpolated linearly in mass
//! between the two nearest named states' own [`alas_opt::envelope::PhysicalCgLimits`]
//! rather than recomputing `h_cg` at an arbitrary sequence point (a coarse,
//! documented approximation: the item's own "cheap mode" allowance).

use alas_config::AlasConfig;
use alas_mass::tanks::{fuel_vector, resolve_product_layout, FuelVectorPoint};
use alas_opt::envelope::ModelCgEnvelopeAssessment;
use alas_payload::loading_sequence::{
    cargo_loading_sequences, concat_sequences, passenger_loading_sequences, potato_boundary,
    LoadingPoint, LoadingSequence, PotatoPoint,
};

use crate::full_analysis::AnalysisReport;

use super::{FindingCode, FindingSeverity, PhysicalFinding};

/// Warning-severity findings for every extreme potato/fuel-vector point
/// outside the physical band, one per category (worst point only): a real
/// finding a lumped centroid could never surface, but not a hard rejection -- the per-state gate already governs
/// delivery feasibility on the named loading states.
pub fn append_envelope_findings(
    envelope: Option<&OperationalEnvelopeAssessment>,
    findings: &mut Vec<PhysicalFinding>,
) {
    let Some(envelope) = envelope else {
        return;
    };
    for (label, checks) in [
        ("potato extreme", &envelope.potato_checks),
        ("fuel-vector", &envelope.fuel_vector_checks),
    ] {
        let Some(worst) = checks
            .iter()
            .filter(|check| !check.inside_limits)
            .min_by(|a, b| a.cg_pct_mac.total_cmp(&b.cg_pct_mac))
        else {
            continue;
        };
        let limit = if worst.cg_pct_mac < worst.fwd_limit_pct_mac {
            worst.fwd_limit_pct_mac
        } else {
            worst.aft_limit_pct_mac
        };
        findings.push(PhysicalFinding {
            code: FindingCode::ModelCgForwardRangeViolation,
            severity: FindingSeverity::Warning,
            message: format!(
                "{label} point at {:.0} kg lies at {:.2} % MAC, outside the physical [{:.2}, \
                 {:.2}] % MAC band",
                worst.mass_kg, worst.cg_pct_mac, worst.fwd_limit_pct_mac, worst.aft_limit_pct_mac
            ),
            actual: Some(worst.cg_pct_mac),
            limit: Some(limit),
            unit: "% MAC",
        });
    }
}

/// One checked point: its own mass/CG, the physical limits interpolated at
/// that mass, and whether it lies inside them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CheckedPoint {
    /// Cumulative aircraft mass, kg.
    pub mass_kg: f64,
    /// Longitudinal CG, aircraft body axes, m.
    pub x_m: f64,
    /// Longitudinal CG, percent MAC.
    pub cg_pct_mac: f64,
    /// Physical aft limit interpolated at this point's mass, percent MAC.
    pub aft_limit_pct_mac: f64,
    /// Physical forward limit at this point's mass (weight-independent
    /// except for the interpolation's own bracketing), percent MAC.
    pub fwd_limit_pct_mac: f64,
    /// Whether `cg_pct_mac` lies inside `[fwd_limit_pct_mac, aft_limit_pct_mac]`.
    pub inside_limits: bool,
}

/// The extreme loading sequences, their potato envelope, the
/// fuel-burn CG vector from analyzed takeoff, and the ZFW operational
/// forward/aft limits those two combine into.
#[derive(Debug, Clone, PartialEq)]
pub struct OperationalEnvelopeAssessment {
    /// The four window/aisle x front/back passenger boarding sequences,
    /// DOW to ZFW; empty with no seat rows (freighter/empty cabin).
    pub passenger_sequences: Vec<LoadingSequence>,
    /// The two forward/aft-hold-first cargo sequences, DOW to ZFW; empty
    /// with no lower-hold cargo.
    pub cargo_sequences: Vec<LoadingSequence>,
    /// The min/max CG envelope ("potato") every sampled sequence bounds, in
    /// percent MAC (mass, min %MAC, max %MAC).
    pub potato_pct_mac: Vec<(f64, f64, f64)>,
    /// Every potato extreme point checked against the interpolated physical
    /// limits at its own mass.
    pub potato_checks: Vec<CheckedPoint>,
    /// The aircraft's CG vector from the analyzed takeoff fuel load down to
    /// zero fuel, following the tank layout's own burn order
    /// (`alas_mass::tanks::fuel_vector`), each point checked against the
    /// physical limits at its own mass.
    pub fuel_vector_checks: Vec<CheckedPoint>,
    /// The zero-fuel operational envelope: the range of ZFW CGs, percent
    /// MAC, from which adding fuel along [`Self::fuel_vector_checks`]'s own
    /// burn order stays inside the physical limits at every intermediate
    /// mass up to the analyzed takeoff. `None` when the analyzed ZFW itself
    /// could not be bracketed (e.g. no finite forward/aft limit at ZFW mass).
    pub zfw_operational_limits_pct_mac: Option<(f64, f64)>,
}

/// `index = W * (x - x_ref) / c + k`: a load-and-trim-sheet balance index,
/// the A320-style convention (`x_ref` at 25 %MAC, `c` = 1000 kg*m, `k` = 50)
/// unless the caller states its own.
#[must_use]
pub fn balance_index(
    mass_kg: f64,
    x_m: f64,
    x_ref_m: f64,
    index_constant_kg_m: f64,
    k: f64,
) -> f64 {
    mass_kg * (x_m - x_ref_m) / index_constant_kg_m + k
}

/// Interpolate the physical aft limit, percent MAC, at `mass_kg` between the
/// two nearest named loading states' own [`alas_opt::envelope::PhysicalCgLimits`].
/// Clamps to the lightest/heaviest state outside the bracket rather than
/// extrapolating.
fn interpolated_aft_limit_pct_mac(model_cg: &ModelCgEnvelopeAssessment, mass_kg: f64) -> f64 {
    let mut states: Vec<(f64, f64)> = model_cg
        .loading_states
        .iter()
        .map(|state| (state.mass_kg, state.physical_limits.aft_limit_pct_mac))
        .filter(|(mass, limit)| mass.is_finite() && limit.is_finite())
        .collect();
    states.sort_by(|a, b| a.0.total_cmp(&b.0));
    match states.as_slice() {
        [] => f64::NAN,
        [(_, only)] => *only,
        _ => {
            if mass_kg <= states[0].0 {
                return states[0].1;
            }
            if mass_kg >= states[states.len() - 1].0 {
                return states[states.len() - 1].1;
            }
            for window in states.windows(2) {
                let (mass_lo, limit_lo) = window[0];
                let (mass_hi, limit_hi) = window[1];
                if mass_kg >= mass_lo && mass_kg <= mass_hi {
                    let span = (mass_hi - mass_lo).max(1.0e-9);
                    let fraction = (mass_kg - mass_lo) / span;
                    return limit_lo + fraction * (limit_hi - limit_lo);
                }
            }
            states[states.len() - 1].1
        }
    }
}

/// The physical forward limit, percent MAC: weight-independent,
/// so any named state's own value stands for every mass.
fn forward_limit_pct_mac(model_cg: &ModelCgEnvelopeAssessment) -> f64 {
    model_cg.configured_forward_limit_pct_mac
}

fn check_point(
    mac_frame: alas_geom::aircraft::mac_frame::MacFrame,
    model_cg: &ModelCgEnvelopeAssessment,
    mass_kg: f64,
    x_m: f64,
) -> CheckedPoint {
    let cg_pct_mac = mac_frame.pct_mac(x_m);
    let aft_limit_pct_mac = interpolated_aft_limit_pct_mac(model_cg, mass_kg);
    let fwd_limit_pct_mac = forward_limit_pct_mac(model_cg);
    let inside_limits = cg_pct_mac.is_finite()
        && aft_limit_pct_mac.is_finite()
        && fwd_limit_pct_mac.is_finite()
        && cg_pct_mac >= fwd_limit_pct_mac
        && cg_pct_mac <= aft_limit_pct_mac;
    CheckedPoint {
        mass_kg,
        x_m,
        cg_pct_mac,
        aft_limit_pct_mac,
        fwd_limit_pct_mac,
        inside_limits,
    }
}

/// Build the operational envelope from a completed report and its already-
/// formed model CG assessment. `None` when the payload layout or the fuel
/// tank arrangement cannot be resolved on this geometry: reported by the
/// caller's own existing findings for those failures, not duplicated here.
#[must_use]
pub fn assess_operational_envelope(
    config: &AlasConfig,
    report: &AnalysisReport,
    model_cg: &ModelCgEnvelopeAssessment,
) -> Option<OperationalEnvelopeAssessment> {
    let mac_frame = report.airplane.mac_frame()?;
    let payload_layout = report.payload_layout.as_ref()?;
    let dow_state = model_cg
        .loading_states
        .iter()
        .find(|state| state.state == alas_opt::envelope::ModelCgLoadingState::OperatingEmpty)?;
    let dow = LoadingPoint {
        mass_kg: dow_state.mass_kg,
        x_m: dow_state.cg_x_m,
    };
    // The single DOW definition this item's sequences and the hard gate's
    // own OEW loading state share: both read
    // `ModelCgLoadingState::OperatingEmpty` off the same `model_cg`
    // assessment passed in by the caller, so there is exactly one DOW mass
    // and CG in the pipeline, not a second one computed independently by
    // the payload lane. An earlier draft of this item quoted a different
    // DOW %MAC (33.81, versus this state's 31.20) from a provisional
    // calculation made before the sequences were wired to this same
    // `model_cg` state; that number was never a second, competing
    // definition in the shipped code, only a stale estimate.
    let zfw_state = model_cg
        .loading_states
        .iter()
        .find(|state| state.state == alas_opt::envelope::ModelCgLoadingState::AnalyzedZeroFuel)?;
    let passenger_sequences = passenger_loading_sequences(payload_layout, dow);
    let cabin_geometry = alas_payload::geometry::CabinGeometry::new(
        &report.airplane,
        &config.geometry,
        config.cabin.passenger.wall_thickness_m,
    )
    .ok();
    let wing_box_x_range = cabin_geometry.map(|geometry| geometry.wing_box_x_range());
    let cargo_sequences = wing_box_x_range
        .map(|range| cargo_loading_sequences(payload_layout, range, dow))
        .unwrap_or_default();

    // Physically compose the cargo and passenger orders end to end: loading cargo alone or boarding passengers
    // alone each only carries the aircraft to a *partial* payload mass (one
    // category's own total, not the design ZFW), so neither, by itself, is
    // a real loading order for a mixed-payload aircraft. A cargo-only
    // sequence that ends below ZFW would hold its last CG flat at every
    // higher mass a passenger-only sequence still reaches (a lower bound stuck
    // at the DOW-adjacent CG all the way to ZFW). Composing cargo-then-passengers (started from the
    // cargo end point) and passengers-then-cargo (started from the
    // passenger end point), in every combination, makes every sequence fed
    // to `potato_boundary` end at the same design ZFW point, so no sequence
    // is ever asked to cover a mass range it did not actually load through.
    let mut all_sequences: Vec<LoadingSequence> = Vec::new();
    for cargo_seq in &cargo_sequences {
        let Some(cargo_end) = cargo_seq.points.last().copied() else {
            continue;
        };
        for pax_seq in passenger_loading_sequences(payload_layout, cargo_end) {
            let name = format!("{} + {}", cargo_seq.name, pax_seq.name);
            all_sequences.push(concat_sequences(&name, cargo_seq, &pax_seq));
        }
    }
    for pax_seq in &passenger_sequences {
        let Some(pax_end) = pax_seq.points.last().copied() else {
            continue;
        };
        if let Some(range) = wing_box_x_range {
            for cargo_seq in cargo_loading_sequences(payload_layout, range, pax_end) {
                let name = format!("{} + {}", pax_seq.name, cargo_seq.name);
                all_sequences.push(concat_sequences(&name, pax_seq, &cargo_seq));
            }
        }
    }
    // A single-category payload (no cargo, or no seats) has no cross
    // composition to build; its own sequences already run DOW to the
    // design ZFW and stand on their own.
    if cargo_sequences.is_empty() {
        all_sequences.extend(passenger_sequences.clone());
    }
    if passenger_sequences.is_empty() {
        all_sequences.extend(cargo_sequences.clone());
    }

    let potato: Vec<PotatoPoint> = if all_sequences.is_empty() {
        Vec::new()
    } else {
        potato_boundary(&all_sequences, 21)
    };
    let potato_pct_mac: Vec<(f64, f64, f64)> = potato
        .iter()
        .map(|point| {
            (
                point.mass_kg,
                mac_frame.pct_mac(point.min_x_m),
                mac_frame.pct_mac(point.max_x_m),
            )
        })
        .collect();
    let potato_checks: Vec<CheckedPoint> = potato
        .iter()
        .flat_map(|point| {
            [
                check_point(mac_frame, model_cg, point.mass_kg, point.min_x_m),
                check_point(mac_frame, model_cg, point.mass_kg, point.max_x_m),
            ]
        })
        .collect();

    let tanks = resolve_product_layout(config, &report.design, &report.airplane).ok()?;
    let tow_state = model_cg
        .loading_states
        .iter()
        .find(|state| state.state == alas_opt::envelope::ModelCgLoadingState::AnalyzedTakeoff)?;
    let takeoff_fuel_kg = (tow_state.mass_kg - zfw_state.mass_kg).max(0.0);
    let takeoff_fuel_state = tanks.distribute(takeoff_fuel_kg).ok()?;
    let vector: Vec<FuelVectorPoint> = fuel_vector(&tanks, &takeoff_fuel_state, 21);
    let fuel_vector_checks: Vec<CheckedPoint> = vector
        .iter()
        .map(|point| {
            let mass_kg = zfw_state.mass_kg + point.fuel_kg;
            let x_m = if point.fuel_kg > 0.0 {
                (zfw_state.mass_kg * zfw_state.cg_x_m + point.fuel_kg * point.x_m)
                    / mass_kg.max(1.0)
            } else {
                zfw_state.cg_x_m
            };
            check_point(mac_frame, model_cg, mass_kg, x_m)
        })
        .collect();

    // The zero-fuel operational envelope : the set of ZFW CGs from
    // which adding fuel along the same burn order stays inside the physical
    // limits at every intermediate mass. Grid search over a bounded number
    // of candidate ZFW stations (cheap, like the potato sampling):
    // the physical limits at ZFW mass already bracket the search domain.
    const ZFW_CANDIDATES: usize = 21;
    let zfw_aft = interpolated_aft_limit_pct_mac(model_cg, zfw_state.mass_kg);
    let zfw_fwd = forward_limit_pct_mac(model_cg);
    let zfw_operational_limits_pct_mac = if zfw_aft.is_finite() && zfw_fwd.is_finite() {
        let mut admissible_fwd = f64::INFINITY;
        let mut admissible_aft = f64::NEG_INFINITY;
        for step in 0..ZFW_CANDIDATES {
            let fraction = step as f64 / (ZFW_CANDIDATES - 1).max(1) as f64;
            let candidate_pct_mac = zfw_fwd + fraction * (zfw_aft - zfw_fwd);
            let candidate_x_m = mac_frame.x_lemac_m + candidate_pct_mac / 100.0 * mac_frame.chord_m;
            let admissible = vector.iter().all(|point| {
                let mass_kg = zfw_state.mass_kg + point.fuel_kg;
                let x_m = if point.fuel_kg > 0.0 {
                    (zfw_state.mass_kg * candidate_x_m + point.fuel_kg * point.x_m)
                        / mass_kg.max(1.0)
                } else {
                    candidate_x_m
                };
                let checked = check_point(mac_frame, model_cg, mass_kg, x_m);
                checked.inside_limits
            });
            if admissible {
                admissible_fwd = admissible_fwd.min(candidate_pct_mac);
                admissible_aft = admissible_aft.max(candidate_pct_mac);
            }
        }
        (admissible_fwd.is_finite() && admissible_aft.is_finite())
            .then_some((admissible_fwd, admissible_aft))
    } else {
        None
    };

    Some(OperationalEnvelopeAssessment {
        passenger_sequences,
        cargo_sequences,
        potato_pct_mac,
        potato_checks,
        fuel_vector_checks,
        zfw_operational_limits_pct_mac,
    })
}

// Fixtures are registered presets; a failed unwrap/expect is the assertion
// failing, not a library invariant breaking.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::full_analysis::FullAnalysis;
    use alas_config::AlasConfig;

    fn a320_report_and_model_cg() -> (AlasConfig, AnalysisReport, ModelCgEnvelopeAssessment) {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" }))
            .expect("A320-200 preset config");
        let preset = alas_config::presets::get("A320-200").expect("registered preset");
        let report = FullAnalysis::new(config.clone())
            .run(&preset.design_vector, true)
            .expect("A320-200 must analyze");
        let model_cg = crate::feasibility::assess_physical_feasibility(
            &config,
            &preset.design_vector,
            &report,
            None,
        )
        .model_cg
        .expect("A320-200 must produce a model CG assessment");
        (config, report, model_cg)
    }

    /// Every composed potato sequence ends at the same
    /// design ZFW the hard gate's `AnalyzedZeroFuel` loading state
    /// analyzed, within 1 kg and 0.01 %MAC: the potato's highest sampled
    /// mass level is that shared ZFW mass, and it must not still be
    /// widening there (the composition bug this item fixes let a
    /// shorter, single-category sequence hold a stale, too-forward CG
    /// flat all the way to the top of the envelope).
    #[test]
    fn every_composed_sequence_ends_at_the_gates_analyzed_zfw() {
        let (config, report, model_cg) = a320_report_and_model_cg();
        let mac_frame = report.airplane.mac_frame().expect("A320-200 mac frame");
        let envelope = assess_operational_envelope(&config, &report, &model_cg)
            .expect("the A320-200 resolves a payload layout and a fuel-tank arrangement");
        let zfw_state = model_cg
            .loading_states
            .iter()
            .find(|state| state.state == alas_opt::envelope::ModelCgLoadingState::AnalyzedZeroFuel)
            .expect("A320-200 has an analyzed ZFW state");
        let zfw_pct_mac = mac_frame.pct_mac(zfw_state.cg_x_m);
        let top = envelope
            .potato_pct_mac
            .last()
            .expect("a non-empty composed potato");
        assert!(
            (top.0 - zfw_state.mass_kg).abs() < 1.0,
            "top potato mass {} should equal the gate's analyzed ZFW mass {} within 1 kg",
            top.0,
            zfw_state.mass_kg
        );
        assert!(
            (top.1 - top.2).abs() < 0.01,
            "the potato must not still be widening at the shared ZFW point: min {} max {}",
            top.1,
            top.2
        );
        assert!(
            (top.1 - zfw_pct_mac).abs() < 0.01,
            "top potato CG {} should equal the gate's analyzed ZFW CG {} within 0.01 %MAC",
            top.1,
            zfw_pct_mac
        );
    }

    /// A registered aircraft with a real seat map, cargo holds and fuel
    /// tanks produces non-empty sequences, a widening potato and a finite
    /// ZFW operational band -- the plumbing this item adds is exercised
    /// end to end, not stubbed behind the lumped fuel/payload centroid.
    #[test]
    fn a320_produces_real_sequences_a_potato_and_a_zfw_band() {
        let (config, report, model_cg) = a320_report_and_model_cg();
        let envelope = assess_operational_envelope(&config, &report, &model_cg)
            .expect("the A320-200 resolves a payload layout and a fuel-tank arrangement");
        assert!(!envelope.passenger_sequences.is_empty());
        assert!(!envelope.potato_pct_mac.is_empty());
        assert!(!envelope.fuel_vector_checks.is_empty());
        // The potato must actually widen somewhere between DOW and ZFW
        // (min_x < max_x at at least one interior level): a lumped centroid
        // would report a single line instead.
        assert!(envelope
            .potato_pct_mac
            .iter()
            .any(|(_, min_pct, max_pct)| max_pct - min_pct > 0.5));
        let (fwd, aft) = envelope
            .zfw_operational_limits_pct_mac
            .expect("a bracketable ZFW operational band");
        assert!(fwd.is_finite() && aft.is_finite() && fwd <= aft);
    }

    #[test]
    fn balance_index_matches_the_a320_style_convention_by_construction() {
        // index = W*(x - x_ref)/C + K; at x == x_ref the index is exactly K.
        assert!((balance_index(70_000.0, 12.0, 12.0, 1000.0, 50.0) - 50.0).abs() < 1.0e-9);
        // A positive (x - x_ref) raises the index proportional to mass.
        let baseline = balance_index(70_000.0, 12.0, 12.0, 1000.0, 50.0);
        let shifted = balance_index(70_000.0, 13.0, 12.0, 1000.0, 50.0);
        assert!((shifted - baseline - 70.0).abs() < 1.0e-9);
    }
}
