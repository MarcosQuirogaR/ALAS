// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::args::apply_cli_tool_preferences;
use super::{load_config, parse_args, CliArgs};
use alas_config::AlasConfig;
use alas_exec::ToolPreferences;
use alas_pipeline::{AerodynamicSolverMode, OptimizationSolverMode};

#[test]
fn solver_flags_select_independent_analysis_and_optimization_backends() {
    let args = [
        "--aero-solver".to_owned(),
        "avl".to_owned(),
        "--optimization-solver".to_owned(),
        "both".to_owned(),
    ];
    let parsed = parse_args(&args)
        .unwrap_or_else(|error| panic!("solver flags parse: {error}"))
        .unwrap_or_else(|| panic!("solver flags do not request help"));
    assert_eq!(parsed.aerodynamic_solver, AerodynamicSolverMode::Avl);
    assert_eq!(parsed.optimization_solver, OptimizationSolverMode::Both);
}

#[test]
fn solver_flags_reject_unknown_modes() {
    let args = ["--aero-solver".to_owned(), "panel".to_owned()];
    let error = parse_args(&args)
        .err()
        .unwrap_or_else(|| panic!("unknown solver mode must be rejected"));
    assert!(error.contains("unknown aerodynamic solver"), "{error}");
}

#[test]
fn optimization_method_accepts_each_configured_profile() {
    for method in ["scipy_legacy", "differential_evolution"] {
        let args = ["--optimization-method".to_owned(), method.to_owned()];
        let parsed = parse_args(&args)
            .unwrap_or_else(|error| panic!("optimization method parses: {error}"))
            .unwrap_or_else(|| panic!("optimization method does not request help"));
        assert_eq!(parsed.optimization_method.as_deref(), Some(method));
    }
}

#[test]
fn optimization_method_rejects_removed_search_names() {
    // Search names that are no longer product profiles are rejected by
    // the flag itself; saved configuration documents that still carry
    // them are migrated by `alas_config::settings_load_notes`.
    for method in ["feasibility_first_de", "nsga2", "turbo_1", "cma_es", "sqp"] {
        let args = ["--optimization-method".to_owned(), method.to_owned()];
        let error = parse_args(&args)
            .err()
            .unwrap_or_else(|| panic!("{method} must be rejected as a CLI flag value"));
        assert!(error.contains("invalid optimization method"), "{error}");
    }
}

#[test]
fn optimization_method_rejects_unknown_strategy() {
    let args = [
        "--optimization-method".to_owned(),
        "random_search".to_owned(),
    ];
    let error = parse_args(&args)
        .err()
        .unwrap_or_else(|| panic!("unknown optimization method must be rejected"));
    assert!(error.contains("invalid optimization method"), "{error}");
}

#[test]
fn optimization_method_overrides_the_effective_configuration() {
    let args = CliArgs {
        optimization_method: Some("differential_evolution".to_owned()),
        ..CliArgs::default()
    };
    let config =
        load_config(&args).unwrap_or_else(|error| panic!("effective configuration loads: {error}"));
    assert_eq!(config.optimizer.solver.method, "differential_evolution");
}

#[test]
fn cli_applies_nastran_solver_preference_without_overriding_explicit_config() {
    let preferences = ToolPreferences {
        nastran_solver: Some("C:/MSC/analysis.exe".to_owned()),
        navdata_dir: Some("C:/ALAS/navdata".to_owned()),
        routes_dir: Some("C:/ALAS/routes".to_owned()),
        ..ToolPreferences::default()
    };

    let mut default_config = AlasConfig::default();
    apply_cli_tool_preferences(&mut default_config, &preferences, true);
    assert_eq!(
        default_config.structures.nastran_solver_path,
        "C:/MSC/analysis.exe"
    );
    assert_eq!(default_config.mission.navdata_dir, "C:/ALAS/navdata");
    assert_eq!(default_config.mission.routes_dir, "C:/ALAS/routes");

    let mut explicit_config = AlasConfig::default();
    explicit_config.structures.nastran_solver_path = "D:/project/analysis.exe".to_owned();
    apply_cli_tool_preferences(&mut explicit_config, &preferences, false);
    assert_eq!(
        explicit_config.structures.nastran_solver_path,
        "D:/project/analysis.exe"
    );
    assert_ne!(explicit_config.mission.navdata_dir, "C:/ALAS/navdata");
    assert_ne!(explicit_config.mission.routes_dir, "C:/ALAS/routes");
}

#[test]
fn cpacs_input_path_is_parsed_without_changing_solver_defaults() {
    let args = [
        "--cpacs-input".to_owned(),
        "aircraft.cpacs.xml".to_owned(),
        "--no-optimize".to_owned(),
        "--no-baseline".to_owned(),
    ];
    let parsed = parse_args(&args)
        .unwrap_or_else(|error| panic!("CPACS input flags parse: {error}"))
        .unwrap_or_else(|| panic!("CPACS input flags do not request help"));
    assert_eq!(
        parsed.cpacs_input.as_deref(),
        Some(std::path::Path::new("aircraft.cpacs.xml"))
    );
    assert!(parsed.no_optimize);
    assert!(parsed.no_baseline);
    assert_eq!(parsed.aerodynamic_solver, AerodynamicSolverMode::Both);
}

#[test]
fn a_stray_positional_argument_is_rejected_rather_than_ignored() {
    let args = ["--no-optimize".to_owned(), "config.yaml".to_owned()];
    let error = parse_args(&args)
        .err()
        .unwrap_or_else(|| panic!("a positional argument must be rejected"));
    assert!(
        error.contains("unexpected argument: config.yaml"),
        "{error}"
    );
}

#[test]
fn a_seed_outside_the_configuration_range_is_rejected_rather_than_wrapped() {
    let args = CliArgs {
        seed: Some(u64::MAX),
        ..CliArgs::default()
    };
    let error = load_config(&args)
        .err()
        .unwrap_or_else(|| panic!("a seed above i64::MAX must be rejected"));
    assert!(error.contains("seed exceeds"), "{error}");

    let args = CliArgs {
        seed: Some(i64::MAX as u64),
        ..CliArgs::default()
    };
    let config = load_config(&args)
        .unwrap_or_else(|error| panic!("the largest representable seed loads: {error}"));
    assert_eq!(config.optimizer.solver.seed, Some(i64::MAX));
}

#[test]
fn navdata_download_action_is_parsed_as_a_non_pipeline_command() {
    let args = ["--download-navdata".to_owned()];
    let parsed = parse_args(&args)
        .unwrap_or_else(|error| panic!("navdata flag parses: {error}"))
        .unwrap_or_else(|| panic!("navdata flag does not request help"));
    assert!(parsed.download_navdata);
}
