// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A clean-sheet transport's sized loading is priced on the aircraft its report describes.
//!
//! A clean-sheet passenger candidate derives its wing setting after the first trim and rebuilds
//! the wing rotated. The report ledger is built on the rotated wing, so the candidate's first
//! mass pass and its takeoff loading are priced on it too: priced on the wing before the
//! rotation, the two zero-fuel masses differ by 0.05 to 5 kg and the design-loading identity of
//! the model CG assessment rejects the candidate, the registered aircraft included (seen in
//! passenger clean-sheet optimisation runs).

// A test asserts on values it built from registered presets, so a failed unwrap there is the
// assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig, DesignMode};
use alas_pipeline::acceptance::apply_assessed_aircraft;
use alas_pipeline::{assess_physical_feasibility, FullAnalysis};

#[test]
fn the_sized_loading_of_a_clean_sheet_transport_is_the_reports_ledger() {
    let mut rotated_any = false;
    for name in ["E195-E2", "A220-300"] {
        let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
        config.optimizer.design_space.mode = DesignMode::CleanSheet;
        let design = presets::get(name).unwrap().design_vector;
        let assessment = alas_opt::assess_product_candidate(&config, &design)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        rotated_any |= assessment.resolved.tail_sizing.wing_incidence_deg.is_some();

        let mut sized_config = config.clone();
        let sized_design = apply_assessed_aircraft(&assessment.resolved, &mut sized_config);
        let report = FullAnalysis::new(sized_config.clone())
            .run_sized_candidate(&sized_design, &assessment.sized)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let feasibility = assess_physical_feasibility(&sized_config, &sized_design, &report, None);

        let loading = assessment.sized.takeoff_loading.expect("hard-MTOW loading");
        let balance = feasibility.mass_balance.as_ref().expect("mass ledger");
        let zero_fuel = balance
            .states
            .iter()
            .find(|state| state.label == "zero fuel")
            .expect("zero-fuel state");
        assert!(
            (zero_fuel.mass_kg - loading.zero_fuel_mass_kg).abs() <= 1e-9 * loading.takeoff_mass_kg,
            "{name}: ledger ZFW {} kg against the sized loading's {} kg",
            zero_fuel.mass_kg,
            loading.zero_fuel_mass_kg
        );
        let rejecting: Vec<_> = feasibility
            .findings
            .iter()
            .filter(|finding| finding.message.contains("ledger loading differs"))
            .map(|finding| finding.message.clone())
            .collect();
        assert!(rejecting.is_empty(), "{name}: {rejecting:?}");
    }
    assert!(
        rotated_any,
        "neither aircraft derived a wing rotation, so the test pins nothing"
    );
}
