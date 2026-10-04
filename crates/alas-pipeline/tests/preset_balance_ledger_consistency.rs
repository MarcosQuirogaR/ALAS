// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Search and report must use item-level loading states for their forward
//! CG gate, independently of their different neutral-point mesh resolutions.

#[test]
fn a220_search_and_finalist_gate_use_the_same_item_ledger() {
    // Mission-sized, the closure's fuel is the dispatched fuel, so the
    // search's design and flown loadings are one state and its worst forward
    // margin is the report's single analysed loading. Under Hard MTOW the
    // search also gates the route's own dispatch states, which this report,
    // built without a flown mission, does not carry.
    let config = alas_config::AlasConfig::from_value(&serde_json::json!({
        "preset": "A220-300",
        "optimizer": {
            "design_space": {"mode": "reference_adaptation"},
            "objective": {"mtow_sizing": "sized_by_mission"}
        }
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
    // The search names the loading state that governs its residual; the
    // report's state of the same name is the one compared.
    let governing = search_worst
        .detail
        .as_deref()
        .and_then(|detail| detail.split_once(' '))
        .map(|(_, label)| label)
        .unwrap_or_else(|| panic!("the search residual names its state"));
    let report_state = envelope
        .loading_states
        .iter()
        .find(|state| state.state.label() == governing)
        .unwrap_or_else(|| panic!("the report evaluates the {governing} state"));
    let report_worst = report_state
        .constraints
        .iter()
        .find(|constraint| {
            constraint.constraint == alas_opt::ModelCgConstraint::PhysicalForwardCgLimit
        })
        .unwrap_or_else(|| panic!("finalist forward CG gate unavailable"));
    assert!((search_worst.limit - report_worst.limit).abs() < 1.0e-6);
    // Both statements represent one closed load case, including the fixed
    // aircraft's design-mass wingbox centroid rather than a dispatch-mass
    // centroid. The tolerance admits only numerical roundoff. A report built
    // without a flown mission takes its landing fuel from a fallback
    // fraction, not the dispatch, so only the landing limit is comparable.
    if report_state.state != alas_opt::ModelCgLoadingState::AnalyzedLanding {
        assert!(
            (search_worst.actual - report_worst.actual).abs() < 1.0e-6,
            "search {} versus report {} %MAC ({governing})",
            search_worst.actual,
            report_worst.actual
        );
    }
}
