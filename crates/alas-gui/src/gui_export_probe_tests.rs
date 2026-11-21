// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A headless run of the GUI's own analysis and export paths, for timing a
//! GUI run against the CLI and for exporting every figure without a screen.
//!
//! Opt-in: `ALAS_GUI_PROBE_OUT=<folder>` (and `ALAS_GUI_PROBE_PRESETS`, comma
//! separated, default `A320-200`). For each preset the probe loads it as the
//! preset picker does, runs the analysis through `AppState::start_pipeline`
//! and `poll_worker` (the GUI's worker thread and result path), then writes
//! the figure ZIP, the PDF report and the plot-data bundle the File > Export
//! menu writes, and a `timing.txt` with the run's wall time.
//!
//! By default the external solvers and exports are switched off;
//! `ALAS_GUI_PROBE_AS_LOADED=1` keeps the tool switches the preset load sets,
//! as `alas --preset <name>` does. The design search runs only with
//! `ALAS_GUI_PROBE_SEARCH=1` (compare with the CLI run without
//! `--no-optimize`). Without `ALAS_GUI_PROBE_OUT` the test returns at once.

// The probe reports its timings on the console; nothing else consumes them.
#![allow(clippy::print_stderr)]

use std::time::{Duration, Instant};

use crate::state::AppState;

fn tools_off(state: &mut AppState) {
    for (group, names) in [
        ("mses", &["enabled"][..]),
        (
            "downstream",
            &["openvsp", "vspaero", "avl", "flowunsteady"][..],
        ),
        ("structures", &["run_nastran", "run_patran_export"][..]),
    ] {
        if let Some(values) = state.config_values.get_mut(group) {
            for name in names {
                crate::views::form_page::placement::toggle_bool(values, name, false);
            }
        }
    }
    state.bump_config_revision();
}

#[test]
fn gui_run_and_export_probe() {
    let Some(out) = std::env::var_os("ALAS_GUI_PROBE_OUT").filter(|v| !v.is_empty()) else {
        return;
    };
    let out = std::path::PathBuf::from(out);
    let presets = std::env::var("ALAS_GUI_PROBE_PRESETS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "A320-200".to_owned());
    let as_loaded = std::env::var_os("ALAS_GUI_PROBE_AS_LOADED").is_some_and(|v| !v.is_empty());
    for preset in presets.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let dir = out.join(preset);
        std::fs::create_dir_all(&dir).expect("probe folder");
        let mut state = AppState::default();
        state.load_preset(preset);
        if !as_loaded {
            tools_off(&mut state);
        }
        // The design search runs only when asked for: `alas --preset` without
        // `--no-optimize` is the CLI run to compare it with.
        state.run_options.optimize =
            std::env::var_os("ALAS_GUI_PROBE_SEARCH").is_some_and(|v| !v.is_empty());
        state.run_options.write_outputs = false;
        // Figures on worker threads, as in the running app.
        let ctx = egui::Context::default();
        state.figure_jobs.enable_for_test(&ctx);
        let optimize = state.run_options.optimize;
        let started = Instant::now();
        state.start_pipeline(false);
        assert!(state.is_running, "{preset}: the run did not start");
        while state.is_running {
            std::thread::sleep(Duration::from_millis(50));
            state.poll_worker();
            assert!(
                started.elapsed() < Duration::from_secs(1800),
                "{preset}: run exceeded the 30 min hang guard"
            );
        }
        let run_s = started.elapsed().as_secs_f64();
        assert!(
            state.pipeline_result_complete,
            "{preset}: run did not complete: {}",
            state.status_message
        );
        let figures_started = Instant::now();
        state.drain_figure_jobs();
        let figures_s = figures_started.elapsed().as_secs_f64();
        eprintln!("{preset}: all result figures built {figures_s:.2} s after the result");
        let export_started = Instant::now();
        let zip = crate::export::export_figure_archive(&state, &dir).expect("figure ZIP");
        let zip_s = export_started.elapsed().as_secs_f64();
        let pdf_started = Instant::now();
        let pdf = crate::export::export_pdf_report(&state, &dir).expect("PDF report");
        let pdf_s = pdf_started.elapsed().as_secs_f64();
        let data_started = Instant::now();
        let data = crate::export::export_all_plot_data(&state, &dir).expect("plot data");
        let data_s = data_started.elapsed().as_secs_f64();
        let pngs = crate::export::write_export_pngs(&state, &dir.join("png")).expect("figure PNGs");
        eprintln!("{preset}: {pngs} figure PNGs");
        let line = format!(
            "preset={preset} as_loaded={as_loaded} optimize={optimize} gui_run_s={run_s:.2} figures_s={figures_s:.2} \
             zip_figures={} zip_s={zip_s:.2} pdf_figures={} pdf_s={pdf_s:.2} plot_data={} plot_data_s={data_s:.2}\n",
            zip.figure_count, pdf.figure_count, data.figure_count
        );
        eprint!("{line}");
        std::fs::write(dir.join("timing.txt"), line).expect("timing");
    }
}
