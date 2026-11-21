// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The headless run: pipeline execution, downloads and result reporting.

use std::fs;
use std::path::Path;

use super::args::{
    apply_cli_tool_preferences, discard_config_tool_paths, load_config, parse_args,
    read_config_document, CliArgs,
};
use super::execute::execute_run;
use super::plots::plots_dir;
use super::summaries::{
    print_cpacs_summary, print_flowunsteady_summary, print_mses_summary,
    print_optimization_summary, print_structural_summary, print_tool_failures,
};
use alas_config::AlasConfig;
use alas_exec::download::{download_files, DownloadSpec};
use alas_exec::ToolLocator;

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

    // The stdio server owns stdout for its protocol; it never starts the GUI.
    if cli.mcp {
        let locator = ToolLocator::for_current_process();
        let workspace = crate::server::Workspace::for_locator(&locator, cli.workspace.clone());
        return crate::server::serve(workspace);
    }

    // If --gui or run without headless options, launch desktop GUI
    if cli.gui || (args.is_empty() && cli.config.is_none() && cli.save_config.is_none()) {
        if let Err(e) = alas_gui::run() {
            eprintln!("GUI Launch Error: {e}");
            return 1;
        }
        return 0;
    }

    // Headless runs hold deep pipeline frames: leave the 1 MiB main-thread
    // stack. The GUI branch above stays on the main thread (winit).
    alas_gui::worker_stack::run_on_large_stack(move || run_headless(cli))
}

fn run_headless(cli: CliArgs) -> i32 {
    let mut config = match load_config(&cli) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Configuration Error: {e}");
            return 1;
        }
    };

    let locator = ToolLocator::for_current_process();
    let preferences = locator.load_preferences();
    let own_tool_paths = cli.config.is_some() && cli.trust_config_tool_paths;
    if cli.config.is_some() && !cli.trust_config_tool_paths {
        let discarded = discard_config_tool_paths(&mut config);
        if !discarded.is_empty() {
            eprintln!(
                "Configuration Warning: ignored program locations in the config file ({}); pass --trust-config-tool-paths to use them",
                discarded.join(", ")
            );
        }
    }
    if !config.mission.simbrief_username.trim().is_empty() && !cli.live_dispatch {
        eprintln!(
            "Route Note: mission.simbrief_username is set but no request is made to www.simbrief.com; pass --live-dispatch to fetch the route"
        );
    }
    let data_dirs = (
        config.mission.navdata_dir.clone(),
        config.mission.routes_dir.clone(),
    );
    apply_cli_tool_preferences(&mut config, &preferences, !own_tool_paths);
    if cli.config.is_some() {
        // Data folders stay as the config file wrote them.
        (config.mission.navdata_dir, config.mission.routes_dir) = data_dirs;
    }

    // The airports the GUI keeps, and those a saved workspace carries, are
    // part of the case: without them a route between user-entered aerodromes
    // cannot be planned headless.
    let document = cli
        .config
        .as_deref()
        .and_then(|path| read_config_document(path).ok());
    match crate::custom_airports::install(&locator.custom_airports_path(), document.as_ref()) {
        Ok(report) => {
            for warning in &report.warnings {
                eprintln!("Custom Airport Warning: {warning}");
            }
        }
        Err(error) => eprintln!("Custom Airport Warning: {error}"),
    }

    if let Some(code) = super::tools_install::run_tools_command(&cli, &locator) {
        return code;
    }

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
        // The file is meant to be shared: the dispatch account stays out of
        // it unless this invocation opted in to the live fetch.
        let mut exported = config.clone();
        if !cli.live_dispatch && !exported.mission.simbrief_username.is_empty() {
            exported.mission.simbrief_username.clear();
            eprintln!(
                "Configuration Note: mission.simbrief_username was left out of the saved file; pass --live-dispatch to keep it"
            );
        }
        let yaml = match serde_yaml::to_string(&exported) {
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

    if let Some(only) = cli.only.as_deref() {
        return run_partial(&cli, only, config, &locator);
    }

    let outcome = match execute_run(&cli, config, &locator, &|_| {}, None) {
        Ok(outcome) => outcome,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let result = &outcome.result;

    match &outcome.manifest {
        Some(Ok(path)) if !cli.quiet => println!("Wrote run manifest to {}.", path.display()),
        Some(Err(error)) => eprintln!("Run Manifest Error: {error}"),
        _ => {}
    }

    match &outcome.plots {
        Some(Ok(count)) if !cli.quiet => {
            println!(
                "Saved {count} SVG plot(s) to {}.",
                plots_dir(Some(&cli.output)).display()
            );
        }
        Some(Err(e)) => {
            eprintln!("Plot Export Error: {e}");
            return 1;
        }
        _ => {}
    }

    if !cli.quiet {
        if let Some(ref rep) = result.optimized_report {
            println!(
                "\n{}\n{}",
                alas_pipeline::format_summary(rep, Some(&result.config)),
                result.format_delivery()
            );
        } else {
            println!("\n{}", result.format_delivery());
        }
    }

    print_optimization_summary(result, cli.quiet);
    print_mses_summary(result, cli.quiet);
    print_flowunsteady_summary(result, cli.quiet);
    print_structural_summary(result, cli.quiet);
    print_cpacs_summary(result, cli.quiet);
    print_tool_failures(result, cli.quiet);

    if !result.delivery.verdict.is_success() {
        // The run completed and its files are written, but no accepted design
        // was delivered: a distinct code, so a driver can tell it from a
        // crash or a usage error (1).
        eprintln!(
            "Delivery: {} (exit code {EXIT_NOT_ACCEPTED})",
            result.delivery.verdict.label()
        );
        return EXIT_NOT_ACCEPTED;
    }
    0
}

/// Exit code of a run that completed but delivered no accepted design
/// (verdict "NOT ACCEPTED", physical status infeasible). 0 is an accepted
/// design, 1 a crash or usage error, 2 an unknown `--only` stage.
pub const EXIT_NOT_ACCEPTED: i32 = 3;

/// `alas --only <stages>`: the partial physics run of
/// `alas_pipeline::pipeline::partial`, printed as JSON and written to
/// `<output>/partial_run.json`.
fn run_partial(cli: &CliArgs, only: &str, config: AlasConfig, locator: &ToolLocator) -> i32 {
    use alas_pipeline::pipeline::partial::{PartialOptions, PartialStage};
    let stages = match PartialStage::parse_list(only) {
        Ok(stages) => stages,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    let preferences = locator.load_preferences();
    let environment = locator.resolve_environment(
        Path::new(&config.mses.mses_dir),
        Path::new(&config.structures.nastran_exe_path),
        Path::new(&config.structures.patran_exe_path),
        Path::new(preferences.openvsp_dir.as_deref().unwrap_or("")),
        Path::new(preferences.avl_exe.as_deref().unwrap_or("")),
    );
    let options = PartialOptions {
        stages,
        cache_dir: cli
            .cache_dir
            .clone()
            .unwrap_or_else(|| cli.output.join(".alas-cache")),
        refresh: cli.refresh_cache,
        output_dir: Some(cli.output.clone()),
    };
    let pipeline = alas_pipeline::DesignPipeline::new(config);
    match pipeline.run_partial(&options, &environment) {
        Ok(value) => {
            let text = serde_json::to_string_pretty(&value)
                .unwrap_or_else(|e| format!("{{\"encode_error\": \"{e}\"}}"));
            if let Err(error) = fs::create_dir_all(&cli.output)
                .and_then(|()| fs::write(cli.output.join("partial_run.json"), &text))
            {
                eprintln!("Partial Run Warning: could not write partial_run.json: {error}");
            }
            println!("{text}");
            0
        }
        Err(error) => {
            eprintln!("Partial Run Error: {error}");
            1
        }
    }
}
