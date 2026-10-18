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

#[test]
fn an_option_value_that_is_another_option_is_rejected() {
    let args = ["--save-config".to_owned(), "--quiet".to_owned()];
    let error = parse_args(&args)
        .err()
        .unwrap_or_else(|| panic!("an option must not be taken as a file name"));
    assert!(
        error.contains("missing argument for --save-config"),
        "{error}"
    );
    let args = ["--save-config".to_owned(), "out.yaml".to_owned()];
    assert!(parse_args(&args).is_ok_and(|cli| cli.is_some()));
}

#[test]
fn show_implies_plots_and_the_help_says_it_writes_files() {
    let args = ["--show".to_owned()];
    let cli = parse_args(&args)
        .ok()
        .flatten()
        .unwrap_or_else(|| panic!("--show must parse"));
    assert!(cli.show && cli.plots);
    let help = super::args::help_text();
    assert!(!help.contains("interactively"), "{help}");
    assert!(help.contains("SVG"), "{help}");
}

#[test]
fn every_long_flag_the_parser_accepts_appears_in_the_help_text() {
    let source = include_str!("args.rs");
    let help = super::args::help_text();
    let parser = source
        .split("pub fn parse_args")
        .nth(1)
        .and_then(|rest| rest.split("fn print_help").next())
        .unwrap_or_else(|| panic!("parse_args source not found"));
    let mut flags = 0;
    for piece in parser.split('"') {
        if piece.starts_with("--") && piece.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
            flags += 1;
            assert!(help.contains(piece), "{piece} missing from the help text");
        }
    }
    assert!(flags >= 10, "only {flags} flags found; the scan is broken");
}
