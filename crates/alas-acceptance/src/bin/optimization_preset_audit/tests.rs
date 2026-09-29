// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez
// Test fixtures fail immediately when required setup or expected values are absent.
// Controlled fixtures use unwrap/expect so invalid setup fails at the assertion.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;

fn args() -> Args {
    Args {
        output_dir: PathBuf::from("."),
        seed: DEFAULT_SEED,
        solver_preset: "quick_draft".to_owned(),
        timeout_s: None,
        preset: Some("A320-200".to_owned()),
        experiment: None,
        max_iterations: None,
        population_size: None,
        native_only: false,
    }
}

#[test]
fn an_unmodified_run_reports_the_registered_preset_and_its_settings_verbatim() {
    let (configuration, settings) =
        Configuration::resolve(&args()).expect("quick_draft is registered");
    assert_eq!(configuration.reported_name(), "quick_draft");
    assert_eq!(
        configuration.provenance()["kind"],
        "registered_solver_preset"
    );
    let registered = resolve_settings("quick_draft").expect("quick_draft is registered");
    assert_eq!(settings.max_iterations, registered.max_iterations);
    assert_eq!(settings.population_size, registered.population_size);
    assert_eq!(settings.tolerance, registered.tolerance);
}

/// The rule the whole mechanism exists for: search effort cannot be
/// reduced without a label, so a reduced-budget run can never be read as
/// the preset it was derived from.
#[test]
fn an_unlabelled_search_effort_override_is_refused() {
    let mut generations_only = args();
    generations_only.max_iterations = Some(2);
    let error = Configuration::resolve(&generations_only)
        .expect_err("an unlabelled override must not resolve");
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(
        error.to_string().contains("--experiment"),
        "the refusal must name the flag that would make the run honest: {error}"
    );

    let mut population_only = args();
    population_only.population_size = Some(1);
    assert!(Configuration::resolve(&population_only).is_err());
}

#[test]
fn an_experiment_label_may_not_impersonate_a_registered_preset() {
    let mut request = args();
    request.max_iterations = Some(2);
    request.experiment = Some("balanced".to_owned());
    let error =
        Configuration::resolve(&request).expect_err("a registered name is not a valid label");
    assert!(error.to_string().contains("registered solver preset"));
}

#[test]
fn a_labelled_experiment_reports_its_label_its_origin_and_every_override() {
    let mut request = args();
    request.max_iterations = Some(2);
    request.population_size = Some(1);
    request.experiment = Some("a320-finite-measurement".to_owned());
    let (configuration, settings) =
        Configuration::resolve(&request).expect("a labelled override resolves");

    assert_eq!(settings.max_iterations, 2);
    assert_eq!(settings.population_size, 1);
    // Everything the flags do not reach is still the registered preset's.
    let registered = resolve_settings("quick_draft").expect("quick_draft is registered");
    assert_eq!(settings.tolerance, registered.tolerance);
    assert_eq!(settings.strategy, registered.strategy);
    assert_eq!(settings.method, registered.method);

    assert_eq!(
        configuration.reported_name(),
        "a320-finite-measurement",
        "the row must never report this run as the preset it was derived from"
    );
    let provenance = configuration.provenance();
    assert_eq!(provenance["kind"], "experiment");
    assert_eq!(provenance["derived_from_solver_preset"], "quick_draft");
    let overrides = provenance["search_effort_overrides"]
        .as_array()
        .expect("the overrides are a list");
    assert_eq!(overrides.len(), 2);
    assert_eq!(overrides[0]["setting"], "max_iterations");
    assert_eq!(overrides[0]["registered_value"], registered.max_iterations);
    assert_eq!(overrides[0]["experiment_value"], 2);
}

#[test]
fn the_acknowledgement_bound_is_the_largest_uninterruptible_unit() {
    let watch = CancelWatch::new();
    let scope = alas_opt::CancelScope::attach(Some(watch.flag()));
    assert!(acknowledgement_bound(&watch.snapshot()).is_none());
    scope.evaluation(|| std::thread::sleep(Duration::from_millis(2)));
    scope.block(8, || std::thread::sleep(Duration::from_millis(20)));
    let snapshot = watch.snapshot();
    let bound = acknowledgement_bound(&snapshot).expect("both units were timed");
    assert_eq!(
        bound,
        snapshot
            .longest_block_s
            .expect("the block was the longer unit")
    );
    assert!(bound > snapshot.longest_evaluation_s.unwrap_or(0.0));
}

#[test]
fn native_only_changes_only_external_execution_switches() {
    for preset in presets::available() {
        let original = AlasConfig::from_value(&json!({"preset": preset})).unwrap();
        let mut native = original.clone();
        preparation::apply_execution_scope(&mut native, true);
        assert!(
            !native.mses.enabled
                && !native.structures.run_nastran
                && !native.structures.run_patran_export
        );
        assert!(
            !native.downstream.openvsp
                && !native.downstream.vspaero
                && !native.downstream.avl
                && !native.downstream.flowunsteady
        );
        native.mses.enabled = original.mses.enabled;
        native.downstream = original.downstream.clone();
        native.structures.run_nastran = original.structures.run_nastran;
        native.structures.run_patran_export = original.structures.run_patran_export;
        assert_eq!(
            serde_json::to_value(native).unwrap(),
            serde_json::to_value(&original).unwrap()
        );
        let mut untouched = original.clone();
        preparation::apply_execution_scope(&mut untouched, false);
        assert_eq!(untouched, original);
    }
}

#[test]
fn native_scope_survives_early_failure_and_default_stays_full() {
    let values = ["--output-dir", ".", "--native-only"].map(str::to_owned);
    assert!(parse_args(&values).unwrap().native_only);
    assert!(!parse_args(&values[..2]).unwrap().native_only);
    let (configuration, settings) = Configuration::resolve(&args()).unwrap();
    let locator = ToolLocator::for_current_process();
    let row = evaluate_preset_optimization(
        "missing-preset",
        Path::new("."),
        &locator,
        &ToolPreferences::default(),
        DEFAULT_SEED,
        &configuration,
        &settings,
        None,
        true,
    );
    assert_eq!(row["status"], "error");
    assert_eq!(row["execution_scope"], "native_only");
    assert_eq!(row["configuration"]["execution_scope"], "native_only");
    assert_eq!(row["external_completion_claimed"], false);
}
