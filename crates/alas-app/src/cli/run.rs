// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The headless run: pipeline execution, downloads and result reporting.

use std::fs;
use std::path::Path;

use super::args::{apply_cli_tool_preferences, load_config, parse_args};
use super::plots::{plots_dir, save_result_plots};
use super::summaries::{print_cpacs_summary, print_mses_summary, print_structural_summary};
use alas_exec::download::{download_files, DownloadSpec};
use alas_exec::ToolLocator;
use alas_pipeline::{read_cpacs_file, DesignPipeline, PipelineOptions};

/// Main application orchestration entry point.
pub fn run_cli(args: &[String]) -> i32 {
    let cli = match parse_args(args) {
        Ok(Some(c)) => c,
        Ok(None) => return 0,
        Err(e) => {
            eprintln!("Error: {e}");
            return 1;
        }
    };

    // If --gui or run without headless options, launch desktop GUI
    if cli.gui || (args.is_empty() && cli.config.is_none() && cli.save_config.is_none()) {
        if let Err(e) = alas_gui::run() {
            eprintln!("GUI Launch Error: {e}");
            return 1;
        }
        return 0;
    }

    let mut config = match load_config(&cli) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Configuration Error: {e}");
            return 1;
        }
    };

    let locator = ToolLocator::for_current_process();
    let preferences = locator.load_preferences();
    let openvsp_dir = preferences.openvsp_dir.clone();
    let avl_exe = preferences.avl_exe.clone();
    apply_cli_tool_preferences(&mut config, &preferences, cli.config.is_none());

    if cli.download_navdata {
        let target = locator.resolve_data_path(Path::new(&config.mission.navdata_dir));
        let specs = alas_route::assets::NAVDATA_FILES
            .iter()
            .map(|file| {
                let spec = DownloadSpec::new(
                    file.name,
                    alas_route::assets::navdata_file_url(file),
                    file.min_bytes,
                );
                match file.expected_sha256 {
                    Some(hash) => spec.with_reviewed_sha256(hash),
                    None => spec,
                }
            })
            .collect::<Vec<_>>();
        // Headless/CI use has no interactive cancel control; the flag is
        // created armed-off and never set.
        let cancel = std::sync::atomic::AtomicBool::new(false);
        match download_files(&specs, &target, 120.0, &cancel) {
            Ok(outcome) => {
                let summary = outcome.report();
                println!(
                    "Navigation data ready at {} (downloaded {}, skipped {}).",
                    summary.target_dir.display(),
                    summary.downloaded.len(),
                    summary.skipped.len()
                );
                return 0;
            }
            Err(error) => {
                eprintln!("Navigation-data download failed: {error}");
                return 1;
            }
        }
    }

    if let Some(ref save_path) = cli.save_config {
        let yaml = match serde_yaml::to_string(&config) {
            Ok(y) => y,
            Err(e) => {
                eprintln!("Serialization Error: {e}");
                return 1;
            }
        };
        if let Err(e) = fs::write(save_path, yaml) {
            eprintln!("Failed to write config file {}: {e}", save_path.display());
            return 1;
        }
        println!("Effective configuration written to {}", save_path.display());
        return 0;
    }

    let pipeline = if let Some(path) = cli.cpacs_input.as_ref() {
        if !cli.no_optimize || !cli.no_baseline {
            eprintln!("CPACS input currently requires both --no-optimize and --no-baseline");
            return 1;
        }
        let document = match read_cpacs_file(path) {
            Ok(document) => document,
            Err(error) => {
                eprintln!("CPACS Input Error: {error}");
                return 1;
            }
        };
        match DesignPipeline::new_with_cpacs_document(config.clone(), document) {
            Ok(pipeline) => pipeline,
            Err(error) => {
                eprintln!("CPACS Input Error: {error}");
                return 1;
            }
        }
    } else {
        DesignPipeline::new(config.clone())
    };
    let options = PipelineOptions {
        optimize: !cli.no_optimize,
        compare_baseline: !cli.no_baseline,
        parallel: !cli.no_parallel,
        aerodynamic_solver: cli.aerodynamic_solver,
        optimization_solver: cli.optimization_solver,
        output_dir: Some(cli.output.clone()),
        save_plots: cli.plots || cli.show,
        seed: cli.seed,
        quiet: cli.quiet,
    };

    let environment = locator.resolve_environment(
        Path::new(&config.mses.mses_dir),
        Path::new(&config.structures.nastran_exe_path),
        Path::new(&config.structures.patran_exe_path),
        Path::new(openvsp_dir.as_deref().unwrap_or("")),
        Path::new(avl_exe.as_deref().unwrap_or("")),
    );

    // The pipeline already measures every stage; collecting the events is
    // what turns "the run took 882 s" into a per-stage account of where that
    // time went. The callback only appends, so it cannot change the run.
    let collected_events: std::sync::Mutex<Vec<alas_pipeline::runs::RunEvent>> =
        std::sync::Mutex::new(Vec::new());
    let record_event = |event: alas_pipeline::runs::RunEvent| {
        if let Ok(mut events) = collected_events.lock() {
            events.push(event);
        }
    };
    let result =
        match pipeline.run_with_environment_and_events(&options, &environment, &record_event) {
            Ok(res) => res,
            Err(e) => {
                eprintln!("Pipeline Execution Error: {e}");
                return 1;
            }
        };

    // A manifest that cannot be written is reported and does not fail the
    // run: the design results are already valid without it.
    if let Some(output_dir) = options.output_dir.as_deref() {
        let events = collected_events
            .lock()
            .map(|events| events.clone())
            .unwrap_or_default();
        let manifest = alas_pipeline::run_manifest::RunManifest::from_run(&result, &events);
        match manifest.write(output_dir) {
            Ok(path) if !cli.quiet => println!("Wrote run manifest to {}.", path.display()),
            Ok(_) => {}
            Err(error) => eprintln!("Run Manifest Error: {error}"),
        }
    }

    if options.save_plots {
        match save_result_plots(&result, options.output_dir.as_deref()) {
            Ok(count) if !cli.quiet => {
                println!(
                    "Saved {count} SVG plot(s) to {}.",
                    plots_dir(options.output_dir.as_deref()).display()
                );
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("Plot Export Error: {e}");
                return 1;
            }
        }
    }

    if !cli.quiet {
        if let Some(ref rep) = result.optimized_report {
            println!(
                "\n{}\n{}",
                alas_pipeline::format_summary(rep, Some(&result.config)),
                alas_pipeline::format_feasibility(&result.feasibility)
            );
        }
    }

    print_mses_summary(&result, cli.quiet);
    print_structural_summary(&result, cli.quiet);
    print_cpacs_summary(&result, cli.quiet);

    0
}
