// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Calibration/validation report for `MtowSizing::Unconstrained`.
//!
//! For every registered aircraft preset with a published reference MTOW
//! ([`AircraftReferenceData::mtow_kg`](alas_config::AircraftReferenceData) is
//! `Some`, which naturally excludes the synthetic "AVE" preset), this runs
//! the preset's own design vector through [`assess_product_candidate`]
//! exactly as documented -- the preset's own configuration, unmodified apart
//! from `optimizer.objective.mtow_sizing` -- twice: once with
//! [`MtowSizing::Unconstrained`], where the declared MTOW seeds only the
//! first pass and is never re-applied as a dispatch ceiling, an Aitken
//! admissibility bound, or a landing-mass-fraction basis, and once with the
//! crate's own [`MtowSizing::SizedByMission`] default for a side-by-side
//! contrast column.
//!
//! # What this does and does not show
//!
//! A freely converged mass landing near the published MTOW would be evidence
//! that the closure is numerically robust *and* that FLOPS's absolute mass
//! regressions are reasonably accurate for these airframes. It would not
//! itself be a physical validation: `docs/flops-mass-model.md` already
//! states this FLOPS implementation has been checked for
//! equation-reproduction parity against NASA Aviary, "not physical
//! validation against a weighed aircraft," and nothing in this model has
//! been calibrated against one.
//!
//! ATR72-600 (a turboprop) is expected to fail in every mode regardless of
//! the finding below: this FLOPS implementation has no propeller/
//! shaft-power mass equation and returns an explicit
//! `unsupported_propulsion_technology` error for it (see
//! `docs/flops-mass-model.md`).
//!
//! # A pre-existing gap this harness surfaced, now fixed
//!
//! `alas_opt::assess_product_candidate` is, in production, only ever called
//! on a design vector an optimizer run has already materialized
//! (`alas-pipeline`'s finalist replay in `pipeline.rs`, gated on
//! `options.optimize && optimization_result.is_some()`, or a search
//! candidate that went through `mdo::canonicalize_design`); no test in this
//! repository had previously called it directly on a bare, unmodified
//! registered preset, and `alas-acceptance`'s own all-preset matrix runs
//! with `optimize: false`, so it never reached this call either. Doing so
//! here, exactly as this validation is specified, first surfaced a real,
//! pre-existing bug that was independent of `MtowSizing` and reproduced
//! identically under `SizedByMission`:
//!
//! `objective_model::apply_candidate_payload_load_case` (called on every
//! candidate build, unconditionally, because
//! `DesignRequirements::resolves_payload_from_candidate_geometry` is
//! hard-coded `true`) always re-derives the passenger count from a
//! geometry-driven capacity solve for every named preset, which is the
//! intended product behavior. The bug was that the declared FLOPS cabin
//! class split (`first`/`business`/`tourist_class_passenger_count`, sourced
//! per aircraft in `preset_flops.rs`) was only re-synchronized to match
//! inside a since-retired `uses_fixed_passenger_target` branch that was
//! never `true` for a named preset, so it stayed stale at the registered
//! aircraft's original count. The buildup's own class-count/passenger-count
//! consistency check (`FlopsTransportUnverifiedReason::PassengerClassCounts`
//! in `alas_mass::flops_transport::product`) then rejected every candidate
//! outright before a takeoff mass was ever closed.
//!
//! The fix (see `objective_model::apply_candidate_payload_load_case`) makes
//! that FLOPS class-count sync unconditional -- always derived from
//! whatever total the dynamic cabin solve just produced, for every design,
//! clean-sheet included, so the two can never disagree by construction. A
//! new, advanced-only `DesignRequirements::min_passenger_capacity` (0 =
//! disabled) supplies an optional hard floor on that dynamically resolved
//! capacity, scored under the existing geometry constraint policy
//! (`mdo::residuals_geometry`'s `passenger_shortfall` residual) rather than
//! forcing an exact count. This is a synchronization/consistency fix, not
//! a change to how capacity is resolved: registered presets still derive
//! their passenger count from geometry exactly as before, which is why the
//! MTOW table below compares against each preset's *registered* payload
//! point rather than a copied planning count.
//!
//! Usage: `cargo run -p alas-opt --release --example mtow_unconstrained_validation`

#![allow(clippy::print_stdout)]

use alas_config::{presets, AircraftPreset, AlasConfig, MtowSizing};
use alas_mass::dispatch::DispatchStatus;
use alas_opt::{assess_product_candidate, CandidateAssessment};

/// One mode's outcome for one preset.
struct RunOutcome {
    /// The converged (or last-evaluated) takeoff mass, kg, when the
    /// candidate was built at all.
    takeoff_mass_kg: Option<f64>,
    /// Outer sizing passes taken.
    passes: usize,
    /// Human-readable outcome: `converged`, `not_converged: <dispatch
    /// status>`, `rejected: <hard residual ids>`, or `error: <reason>`.
    status: String,
}

impl RunOutcome {
    fn error(reason: &str) -> Self {
        Self {
            takeoff_mass_kg: None,
            passes: 0,
            status: format!("error: {reason}"),
        }
    }

    fn from_assessment(assessment: &CandidateAssessment) -> Self {
        let sized = &assessment.sized;
        let status = if !assessment.hard_feasible {
            format!("rejected: {}", assessment.violated_hard_ids().join("+"))
        } else if !sized.sizing_closed {
            format!(
                "not_converged: {}",
                dispatch_status_text(&sized.dispatch.status)
            )
        } else {
            "converged".to_owned()
        };
        Self {
            takeoff_mass_kg: Some(sized.takeoff_mass_kg),
            passes: sized.sizing_iterations,
            status,
        }
    }
}

fn dispatch_status_text(status: &DispatchStatus) -> String {
    match status {
        DispatchStatus::Converged => "converged".to_owned(),
        DispatchStatus::MtowLimited { shortfall_kg } => {
            format!("mtow_limited (shortfall {shortfall_kg:.0} kg)")
        }
        DispatchStatus::TankLimited { shortfall_kg } => {
            format!("tank_limited (shortfall {shortfall_kg:.0} kg)")
        }
        DispatchStatus::NotConverged { last_change_kg } => {
            format!("not_converged (last change {last_change_kg:.0} kg)")
        }
        DispatchStatus::ModelFailed(reason) => format!("model_failed ({reason})"),
    }
}

/// Select `preset` exactly as the product does, then run the mission-sized
/// closure once under `mtow_sizing`. The configuration is otherwise
/// unmodified from what `AlasConfig::from_value` resolves for the named
/// preset.
fn run_mode(preset: &AircraftPreset, mtow_sizing: MtowSizing) -> RunOutcome {
    let mut config = match AlasConfig::from_value(&serde_json::json!({ "preset": preset.name })) {
        Ok(config) => config,
        Err(error) => return RunOutcome::error(&format!("config: {error}")),
    };
    // `MtowSizing::SizedByMission` is already this field's default, so
    // setting it here for the contrast column leaves the preset's own
    // configuration unmodified; it is set explicitly rather than left
    // implicit so this harness keeps reporting the intended column even if
    // that default is ever changed.
    config.optimizer.objective.mtow_sizing = mtow_sizing;
    match assess_product_candidate(&config, &preset.design_vector) {
        Ok(assessment) => RunOutcome::from_assessment(&assessment),
        Err(reason) => RunOutcome::error(&reason),
    }
}

struct ReportRow {
    preset: &'static str,
    reference_mtow_kg: f64,
    unconstrained: RunOutcome,
    sized_by_mission: RunOutcome,
}

fn fmt_mass(mass: Option<f64>) -> String {
    mass.map_or_else(|| "-".to_owned(), |value| format!("{value:.0}"))
}

fn fmt_signed(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_owned(), |value| format!("{value:+.0}"))
}

fn main() {
    let mut rows = Vec::new();
    for preset in presets::registry() {
        let Some(reference_mtow_kg) = preset.reference.mtow_kg else {
            // No published MTOW to compare against (the synthetic "AVE"
            // preset, and any future preset in the same situation).
            continue;
        };
        let unconstrained = run_mode(preset, MtowSizing::Unconstrained);
        let sized_by_mission = run_mode(preset, MtowSizing::SizedByMission);
        rows.push(ReportRow {
            preset: preset.name,
            reference_mtow_kg,
            unconstrained,
            sized_by_mission,
        });
    }

    println!(
        "{:<16} {:>12} {:>14} {:>10} {:>8} {:>16} {:>7}  status (unconstrained | sized_by_mission)",
        "preset", "ref_mtow_kg", "unconstr_kg", "delta_kg", "pct_err", "sized_by_msn_kg", "passes",
    );
    println!("{}", "-".repeat(140));

    let mut abs_pct_errors: Vec<f64> = Vec::new();
    let mut signed_pct_errors: Vec<f64> = Vec::new();

    for row in &rows {
        let delta_kg = row
            .unconstrained
            .takeoff_mass_kg
            .map(|mass| mass - row.reference_mtow_kg);
        let pct_err = delta_kg.map(|delta| 100.0 * delta / row.reference_mtow_kg);
        if row.unconstrained.status == "converged" {
            if let Some(pct_err) = pct_err {
                abs_pct_errors.push(pct_err.abs());
                signed_pct_errors.push(pct_err);
            }
        }
        println!(
            "{:<16} {:>12.0} {:>14} {:>10} {:>8} {:>16} {:>3}/{:<3}  U:{} | S:{}",
            row.preset,
            row.reference_mtow_kg,
            fmt_mass(row.unconstrained.takeoff_mass_kg),
            fmt_signed(delta_kg),
            pct_err.map_or_else(|| "-".to_owned(), |value| format!("{value:+.2}")),
            fmt_mass(row.sized_by_mission.takeoff_mass_kg),
            row.unconstrained.passes,
            row.sized_by_mission.passes,
            row.unconstrained.status,
            row.sized_by_mission.status,
        );
    }

    println!("{}", "-".repeat(140));
    println!(
        "Presets with a published reference MTOW: {}. Converged under Unconstrained: {}.",
        rows.len(),
        abs_pct_errors.len()
    );
    if abs_pct_errors.is_empty() {
        println!(
            "No preset converged to a comparable takeoff mass -- see the per-row status column \
             above and the module doc comment for the FLOPS/dispatch gaps this can still \
             legitimately reflect (e.g. ATR72-600's unsupported_propulsion_technology)."
        );
    } else {
        let mut sorted = abs_pct_errors.clone();
        sorted.sort_by(f64::total_cmp);
        let mean_abs = abs_pct_errors.iter().sum::<f64>() / abs_pct_errors.len() as f64;
        let median_abs = sorted[sorted.len() / 2];
        let worst_abs = sorted[sorted.len() - 1];
        let mean_signed = signed_pct_errors.iter().sum::<f64>() / signed_pct_errors.len() as f64;
        let high = signed_pct_errors
            .iter()
            .filter(|value| **value > 0.0)
            .count();
        let low = signed_pct_errors
            .iter()
            .filter(|value| **value < 0.0)
            .count();
        println!(
            "abs pct error: mean {mean_abs:.2}%, median {median_abs:.2}%, worst {worst_abs:.2}%"
        );
        println!(
            "signed pct error: mean {mean_signed:+.2}% ({high} preset(s) above reference, {low} below)"
        );
    }
    println!(
        "This is a numerical-closure/mass-model calibration check, not a physical validation \
         against a weighed aircraft (see docs/flops-mass-model.md)."
    );
}
