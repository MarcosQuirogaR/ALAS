// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One headless pipeline run, shared by the command line and the stdio server.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

use alas_config::AlasConfig;
use alas_exec::ToolLocator;
use alas_pipeline::runs::RunEvent;
use alas_pipeline::{read_cpacs_file, DesignPipeline, PipelineOptions, PipelineResult};

use super::args::CliArgs;
use super::plots::save_result_plots;

/// What a completed run produced besides the result itself.
pub(crate) struct RunOutcome {
    /// The pipeline result.
    pub result: PipelineResult,
    /// The run manifest path, or why it could not be written; `None` when the
    /// run has no output directory.
    pub manifest: Option<Result<PathBuf, String>>,
    /// Number of SVG figures written, or why the export failed; `None` when
    /// figures were not requested.
    pub plots: Option<Result<usize, String>>,
}

/// Run the pipeline the way `cli` asks, on `config`, forwarding every typed
/// event to `events`. `cancel`, when given, is observed at stage boundaries.
///
/// # Errors
///
/// The CPACS input cannot be read or the pipeline fails; the message names
/// which, in the same words the command line prints.
pub(crate) fn execute_run(
    cli: &CliArgs,
    config: AlasConfig,
    locator: &ToolLocator,
    events: &(dyn Fn(RunEvent) + Sync),
    cancel: Option<&AtomicBool>,
) -> Result<RunOutcome, String> {
    let preferences = locator.load_preferences();
    let pipeline = if let Some(path) = cli.cpacs_input.as_ref() {
        if !cli.no_optimize || !cli.no_baseline {
            return Err(
                "CPACS input currently requires both --no-optimize and --no-baseline".to_owned(),
            );
        }
        let document = read_cpacs_file(path).map_err(|e| format!("CPACS Input Error: {e}"))?;
        let pipeline = DesignPipeline::new_with_cpacs_document(config.clone(), document)
            .map_err(|e| format!("CPACS Input Error: {e}"))?;
        for warning in pipeline.import_warnings() {
            eprintln!("CPACS Input Warning: {warning}");
        }
        pipeline
    } else {
        DesignPipeline::new(config.clone())
    };
    alas_struct::nastran95::request_modes_figure(cli.plots || cli.show);
    let pipeline = pipeline.with_live_dispatch_fetch(cli.live_dispatch);
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
        Path::new(preferences.openvsp_dir.as_deref().unwrap_or("")),
        Path::new(preferences.avl_exe.as_deref().unwrap_or("")),
    );

    // The pipeline already measures every stage; collecting the events is
    // what turns "the run took 882 s" into a per-stage account of where that
    // time went. The callback only appends and forwards, so it cannot change
    // the run.
    let collected_events: Mutex<Vec<RunEvent>> = Mutex::new(Vec::new());
    let record_event = |event: RunEvent| {
        if let Ok(mut collected) = collected_events.lock() {
            collected.push(event.clone());
        }
        events(event);
    };
    let result = match cancel {
        Some(cancel) => {
            pipeline.run_with_events_and_cancel(&options, &environment, &record_event, cancel)
        }
        None => pipeline.run_with_environment_and_events(&options, &environment, &record_event),
    }
    .map_err(|e| format!("Pipeline Execution Error: {e}"))?;

    // A manifest that cannot be written is reported and does not fail the
    // run: the design results are already valid without it.
    let manifest = options.output_dir.as_deref().map(|output_dir| {
        let collected = collected_events
            .lock()
            .map(|collected| collected.clone())
            .unwrap_or_default();
        alas_pipeline::run_manifest::RunManifest::from_run(&result, &collected)
            .write(output_dir)
            .map_err(|error| error.to_string())
    });

    let plots = options.save_plots.then(|| {
        save_result_plots(&result, options.output_dir.as_deref(), cli.plot_data)
            .map_err(|error| error.to_string())
    });

    Ok(RunOutcome {
        result,
        manifest,
        plots,
    })
}
