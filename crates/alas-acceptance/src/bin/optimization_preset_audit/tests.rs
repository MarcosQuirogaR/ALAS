// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez
// Test fixtures fail immediately when required setup or expected values are absent.
// Controlled fixtures use unwrap/expect so invalid setup fails at the assertion.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::configuration::resolve_settings;
use super::*;

fn args() -> Args {
    Args {
        output_dir: PathBuf::from("."),
        seed: DEFAULT_SEED,
        solver_preset: "quick_draft".to_owned(),
        timeout_s: None,
        preset: Some("A320-200".to_owned()),
        experiment: None,
        native_only: true,
        mtow_mode: None,
        time_limit_s: None,
        max_evaluations: None,
        screening_time_limit_s: None,
        screening_max_evaluations: None,
        replay_evaluations: None,
        stop_on_evaluations: false,
        workers: None,
    }
}

#[test]
fn full_fidelity_valid_counts_distinct_feasible_designs_only() {
    let nominal = alas_config::DesignVector::default();
    let changed = alas_config::DesignVector {
        span_m: nominal.span_m + 1.0,
        ..nominal
    };
    let signed_zero = alas_config::DesignVector {
        wing_x_shift_m: -0.0,
        ..nominal
    };
    let history = alas_opt::OptimizationHistory {
        design_vectors: vec![nominal, nominal, signed_zero, changed],
        valid: vec![true, true, true, false],
        hard_violation: vec![0.0; 4],
        cost: vec![1.0; 4],
        objective_value: vec![1.0; 4],
        ..Default::default()
    };
    assert_eq!(exposure::full_fidelity_valid(&history), 1);
    let mut history = history;
    history.valid[3] = true;
    assert_eq!(exposure::full_fidelity_valid(&history), 2);
    history.hard_violation[3] = f64::EPSILON;
    assert_eq!(exposure::full_fidelity_valid(&history), 1);
    history.hard_violation[3] = 0.0;
    history.objective_value[3] = f64::NAN;
    assert_eq!(exposure::full_fidelity_valid(&history), 1);
}

#[test]
fn a_worker_count_is_a_labelled_override() {
    let wide = Args {
        workers: Some(8),
        ..args()
    };
    assert!(Configuration::resolve(&wide).is_err(), "unlabelled workers");
    let labelled = Args {
        experiment: Some("replay-a320-8".to_owned()),
        ..wide
    };
    let (configuration, settings) = Configuration::resolve(&labelled).expect("labelled");
    assert_eq!(settings.workers, 8);
    let overrides = &configuration.provenance()["search_effort_overrides"];
    assert_eq!(overrides[0]["setting"], "solver.workers");
    let parsed = parse_args(&[
        "--output-dir".into(),
        ".".into(),
        "--workers".into(),
        "8".into(),
    ])
    .expect("parses");
    assert_eq!(parsed.workers, Some(8));
}

#[test]
fn a_replay_is_a_labelled_override_that_sets_both_stage_counts() {
    let replay = Args {
        replay_evaluations: Some((64, 590, Some(915), Some(12))),
        ..args()
    };
    assert!(
        Configuration::resolve(&replay).is_err(),
        "unlabelled replay"
    );
    let labelled = Args {
        experiment: Some("replay-a320".to_owned()),
        stop_on_evaluations: true,
        ..replay
    };
    let (_, settings) = Configuration::resolve(&labelled).expect("labelled replay");
    assert_eq!(settings.screening.replay_evaluations, Some(64));
    assert_eq!(settings.refinement.replay_evaluations, Some(590));
    assert_eq!(settings.refinement.replay_planned_evaluations, Some(915));
    assert_eq!(settings.refinement.replay_restoration_evaluations, Some(12));
    assert_eq!(settings.screening.replay_planned_evaluations, None);
    assert!(settings.stop_on_evaluations_only);
    let parse = |text: &str| controls::parse_replay(Some(&text.to_owned())).ok();
    assert_eq!(parse("64,590"), Some((64, 590, None, None)));
    assert_eq!(parse("64, 590, 915"), Some((64, 590, Some(915), None)));
    assert_eq!(parse("64,590,915,0"), Some((64, 590, Some(915), Some(0))));
    for invalid in ["64", "64,0", "64,590,x", "64,590,915,591", "64,590,915,1,2"] {
        assert_eq!(parse(invalid), None, "{invalid}");
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
    assert_eq!(settings.screening, registered.screening);
    assert_eq!(settings.refinement, registered.refinement);
    assert_eq!(settings.tolerance, registered.tolerance);
}

/// The rule the whole mechanism exists for: search effort cannot be
/// reduced without a label, so a reduced-budget run can never be read as
/// the preset it was derived from.
#[test]
fn an_unlabelled_search_effort_override_is_refused() {
    let mut generations_only = args();
    generations_only.max_evaluations = Some(48);
    let error = Configuration::resolve(&generations_only)
        .expect_err("an unlabelled override must not resolve");
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(
        error.to_string().contains("--experiment"),
        "the refusal must name the flag that would make the run honest: {error}"
    );

    let mut time_only = args();
    time_only.screening_time_limit_s = Some(5);
    assert!(Configuration::resolve(&time_only).is_err());
}

#[test]
fn an_experiment_label_may_not_impersonate_a_registered_preset() {
    let mut request = args();
    request.max_evaluations = Some(48);
    request.experiment = Some("balanced".to_owned());
    let error =
        Configuration::resolve(&request).expect_err("a registered name is not a valid label");
    assert!(error.to_string().contains("registered solver preset"));
}

#[test]
fn a_labelled_experiment_reports_its_label_its_origin_and_every_override() {
    let mut request = args();
    request.max_evaluations = Some(48);
    request.screening_time_limit_s = Some(5);
    request.experiment = Some("a320-finite-measurement".to_owned());
    let (configuration, settings) =
        Configuration::resolve(&request).expect("a labelled override resolves");

    assert_eq!(settings.refinement.max_evaluations, 48);
    assert_eq!(settings.screening.time_limit_s, 5.0);
    // Everything the flags do not reach is still the registered preset's.
    let registered = resolve_settings("quick_draft").expect("quick_draft is registered");
    assert_eq!(settings.tolerance, registered.tolerance);
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
    assert_eq!(overrides[0]["setting"], "refinement.max_evaluations");
    assert_eq!(
        overrides[0]["registered_value"],
        registered.refinement.max_evaluations
    );
    assert_eq!(overrides[0]["experiment_value"], 48);
    assert_eq!(overrides[1]["setting"], "screening.time_limit_s");
    assert_eq!(overrides[1]["experiment_value"], 5.0);
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
    // The sleeps only order the units on an idle machine; under load the
    // scheduler can stretch the short one past the long one. The bound is
    // the larger measured unit whichever that turns out to be.
    let evaluation = snapshot
        .longest_evaluation_s
        .expect("the evaluation was timed");
    let block = snapshot.longest_block_s.expect("the block was timed");
    assert!(evaluation >= 0.002 && block >= 0.020);
    assert_eq!(bound, evaluation.max(block));
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
fn native_scope_survives_early_failure_and_external_tools_require_opt_in() {
    let values = ["--output-dir", ".", "--native-only"].map(str::to_owned);
    assert!(parse_args(&values).unwrap().native_only);
    assert!(parse_args(&values[..2]).unwrap().native_only);
    let external = ["--output-dir", ".", "--external-tools"].map(str::to_owned);
    assert!(!parse_args(&external).unwrap().native_only);
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
        None,
    );
    assert_eq!(row["status"], "error");
    assert_eq!(row["execution_scope"], "native_only");
    assert_eq!(row["configuration"]["execution_scope"], "native_only");
    assert_eq!(row["external_completion_claimed"], false);
}

#[test]
fn aircraft_benchmarks_accept_only_the_user_facing_mtow_modes() {
    for mode in MtowSizing::ALL {
        let values = ["--output-dir", ".", "--mtow-mode", mode.as_str()].map(str::to_owned);
        if super::controls::USER_MTOW_MODES.contains(&mode) {
            assert_eq!(parse_args(&values).unwrap().mtow_mode, Some(mode));
        } else {
            assert!(parse_args(&values).is_err(), "{mode:?}");
        }
    }
    let values = ["--output-dir", ".", "--mtow-mode", "nonsense"].map(str::to_owned);
    assert!(parse_args(&values).is_err());
    assert_eq!(parse_args(&values[..2]).unwrap().mtow_mode, None);
}

#[test]
fn every_stage_budget_flag_reaches_the_settings_under_a_label_and_a_limit_is_validated() {
    let parse = |extra: &[&str]| {
        let mut values: Vec<String> = ["--output-dir", ".", "--experiment", "budget-study"]
            .map(str::to_owned)
            .to_vec();
        values.extend(extra.iter().map(|value| (*value).to_owned()));
        Configuration::resolve(&parse_args(&values).unwrap())
    };
    let (_, settings) = parse(&[
        "--time-limit",
        "240",
        "--max-evaluations",
        "900",
        "--screening-time-limit",
        "20",
        "--screening-max-evaluations",
        "500",
    ])
    .unwrap();
    assert_eq!(settings.refinement.time_limit_s, 240.0);
    assert_eq!(settings.refinement.max_evaluations, 900);
    assert_eq!(settings.screening.time_limit_s, 20.0);
    assert_eq!(settings.screening.max_evaluations, 500);
    // Above the 300 s stage maximum is refused, not clipped.
    assert!(parse(&["--time-limit", "301"]).is_err());
}
