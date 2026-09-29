// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Search and report must use item-level loading states for their forward
//! CG gate, independently of their different neutral-point mesh resolutions.

#[test]
fn a220_search_and_finalist_gate_use_the_same_item_ledger() {
    let config = alas_config::AlasConfig::from_value(&serde_json::json!({
        "preset": "A220-300",
        "optimizer": {"design_space": {"mode": "reference_adaptation"}}
    }))
    .unwrap_or_else(|error| panic!("{error}"));
    let design = alas_config::presets::get("A220-300")
        .unwrap_or_else(|error| panic!("{error}"))
        .design_vector;
    let search = alas_opt::assess_product_candidate(&config, &design)
        .unwrap_or_else(|error| panic!("{error}"));
    let report = alas_pipeline::FullAnalysis::new(config.clone())
        .run_at_sized_takeoff_mass(&search.resolved.design, true, search.sized.takeoff_mass_kg)
        .unwrap_or_else(|error| panic!("{error}"));
    let physical =
        alas_pipeline::assess_physical_feasibility(&config, &search.resolved.design, &report, None);
    let envelope = physical
        .model_cg
        .as_ref()
        .unwrap_or_else(|| panic!("finalist item CG envelope unavailable"));
    let report_worst = envelope
        .loading_states
        .iter()
        .flat_map(|state| state.constraints.iter())
        .filter(|constraint| {
            constraint.constraint == alas_opt::ModelCgConstraint::PhysicalForwardCgLimit
        })
        .max_by(|left, right| {
            left.normalized_exceedance
                .total_cmp(&right.normalized_exceedance)
        })
        .unwrap_or_else(|| panic!("finalist forward CG gate unavailable"));
    let search_worst = search
        .residuals
        .iter()
        .find(|residual| residual.id == "forward_cg_range")
        .unwrap_or_else(|| {
            panic!(
                "search item CG envelope unavailable: {:?}",
                search.violated_hard_ids()
            )
        });
    // Both statements represent one closed load case, including the fixed
    // aircraft's design-mass wingbox centroid rather than a dispatch-mass
    // centroid. The tolerance admits only numerical roundoff.
    assert!(
        (search_worst.actual - report_worst.actual).abs() < 1.0e-6,
        "search {} versus report {} %MAC",
        search_worst.actual,
        report_worst.actual
    );
    assert!((search_worst.limit - report_worst.limit).abs() < 1.0e-6);
}
