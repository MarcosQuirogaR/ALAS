// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Evaluate every registered preset for [`super::run_acceptance_matrix`].

use super::*;

/// Evaluate `names` concurrently, one thread per preset, and return their
/// results in `names` order.
///
/// Presets are independent: each builds its own config and pipeline, the
/// default run environment launches no external tool, and the dense LU is
/// pinned sequential, so running them concurrently changes no number (the
/// matrix JSON was compared byte for byte against the serial loop). Plain
/// threads rather than rayon keep the VLM's own pool undersubscribed. A panic
/// is re-raised as it would have been from the serial loop.
pub(super) fn evaluate_presets(names: &[&'static str]) -> Vec<PresetAcceptanceResult> {
    let outcomes: Vec<_> = std::thread::scope(|scope| {
        let handles: Vec<_> = names
            .iter()
            .map(|name| scope.spawn(move || evaluate_preset(name)))
            .collect();
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            })
            .collect()
    });
    names
        .iter()
        .zip(outcomes)
        .map(|(&name, outcome)| {
            outcome.unwrap_or_else(|e| {
                tracing::error!(preset = name, error = %e, "preset acceptance evaluation failed");
                unevaluated(name)
            })
        })
        .collect()
}

/// The row a preset gets when its evaluation failed before producing one.
fn unevaluated(name: &str) -> PresetAcceptanceResult {
    PresetAcceptanceResult {
        name: name.to_owned(),
        geometry_valid: false,
        mtow_kg: 0.0,
        oew_kg: 0.0,
        mtow_closure_fuel_kg: 0.0,
        usable_fuel_capacity_kg: None,
        analyzed_carried_fuel_kg: 0.0,
        analyzed_takeoff_mass_kg: 0.0,
        mtow_shortfall_kg: 0.0,
        payload_kg: 0.0,
        cruise_l_over_d: 0.0,
        static_margin: 0.0,
        neutral_point_x: 0.0,
        cruise_mach: 0.0,
        wingbox_mass_kg: 0.0,
        figure_scenes_count: 0,
        public_design_matches_preset: false,
        model_cg_envelope_ok: false,
        model_cg_static_margin_floor: f64::NAN,
        model_cg_analyzed_takeoff_static_margin: f64::NAN,
        model_cg_minimum_loading_static_margin: f64::NAN,
        model_cg_target_static_margin: f64::NAN,
        model_cg_target_preference_met: false,
        public_planning_cg_status: PlanningCgStatus::NotEvaluated,
        model_cg_pct_mac: f64::NAN,
        public_planning_cg_pct_mac: None,
        trim_cm_residual: None,
        mission_converged: false,
        mission_fuel_within_available: false,
        cruise_equilibrium: None,
        fuel_capacity_evidence: FuelCapacityEvidence::Unavailable,
        design_mission_status: PresetDesignMissionStatus::Unverified,
        wing_area_within_limit: false,
        execution_passed: false,
        physical_passed: false,
        physical_findings: Vec::new(),
        model_audit: PresetModelAudit::default(),
    }
}
