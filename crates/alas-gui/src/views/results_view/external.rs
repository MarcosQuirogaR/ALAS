// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Run-scoped external-tool evidence shown beside the scientific figures.
//!
//! This is intentionally a small artifact browser, not another solver page:
//! native files are opened in the user's file explorer and the status shown
//! here comes from the pipeline's process, parser, and comparison boundaries.

use std::path::{Path, PathBuf};

use alas_pipeline::PipelineResult;
use egui::{Color32, RichText, Ui};

use crate::views::tr;

pub(super) fn show_external_tools_result(ui: &mut Ui, result: &PipelineResult) {
    ui.heading(tr("External tool evidence"));
    ui.label(
        RichText::new(tr(
            "Run-scoped native statuses and retained files. A parseable artifact can still be marked not comparable when its physical contract is not satisfied.",
        ))
        .weak(),
    );
    ui.add_space(8.0);

    if ui.available_width() >= 920.0 {
        ui.columns(2, |columns| {
            let (left, right) = columns.split_at_mut(1);
            show_openvsp(result, &mut left[0]);
            show_vspaero(result, &mut right[0]);
        });
        ui.add_space(8.0);
        ui.columns(2, |columns| {
            let (left, right) = columns.split_at_mut(1);
            show_avl(result, &mut left[0]);
            show_flowunsteady(result, &mut right[0]);
        });
    } else {
        show_openvsp(result, ui);
        ui.add_space(8.0);
        show_vspaero(result, ui);
        ui.add_space(8.0);
        show_avl(result, ui);
        ui.add_space(8.0);
        show_flowunsteady(result, ui);
    }
    ui.add_space(8.0);
    show_mses(result, ui);
    ui.add_space(8.0);
    show_structures(result, ui);
}

fn show_openvsp(result: &PipelineResult, ui: &mut Ui) {
    let Some(export) = result.openvsp_export.as_ref() else {
        tool_card(ui, "OpenVSP", "not_run", None, Vec::new());
        return;
    };
    let mut artifacts = vec![
        ("AngelScript export".to_owned(), export.script_path.clone()),
        ("OpenVSP project".to_owned(), export.vsp3_path.clone()),
        ("native CAD preview".to_owned(), export.preview_path.clone()),
        (
            "VSPAERO geometry".to_owned(),
            export.vspaero_geometry_path.clone(),
        ),
    ];
    if let Some(path) = export.runtime_stdout_path.as_ref() {
        artifacts.push(("runtime stdout".to_owned(), path.clone()));
    }
    if let Some(path) = export.runtime_stderr_path.as_ref() {
        artifacts.push(("runtime stderr".to_owned(), path.clone()));
    }
    let preview_detail = if export.preview_available {
        format!(
            "native CAD preview: available at {}",
            export.preview_path.display()
        )
    } else {
        format!(
            "native CAD preview: unavailable ({})",
            export
                .preview_error
                .as_deref()
                .unwrap_or("OpenVSP did not produce a fresh PNG")
        )
    };
    let detail = export
        .runtime_error
        .as_deref()
        .map(|runtime| format!("{runtime}; {preview_detail}"))
        .unwrap_or(preview_detail);
    tool_card(
        ui,
        "OpenVSP geometry",
        export.status.as_str(),
        Some(&detail),
        artifacts,
    );
}

fn show_vspaero(result: &PipelineResult, ui: &mut Ui) {
    let Some(vspaero) = result.vspaero_result.as_ref() else {
        tool_card(ui, "VSPAERO", "not_run", None, Vec::new());
        return;
    };
    let mut artifacts = vec![
        ("native geometry".to_owned(), vspaero.geometry_path.clone()),
        ("setup".to_owned(), vspaero.setup_path.clone()),
        ("polar".to_owned(), vspaero.polar_path.clone()),
        (
            "wake history".to_owned(),
            vspaero.case_path.with_extension("history"),
        ),
        (
            "native load distribution".to_owned(),
            vspaero.case_path.with_extension("lod"),
        ),
        ("stdout".to_owned(), vspaero.stdout_path.clone()),
        ("stderr".to_owned(), vspaero.stderr_path.clone()),
    ];
    if !artifacts.iter().any(|(_, path)| path.is_file()) {
        artifacts.clear();
    }
    tool_card(
        ui,
        "VSPAERO",
        vspaero.status.as_str(),
        vspaero.error.as_deref(),
        artifacts,
    );
}

fn show_avl(result: &PipelineResult, ui: &mut Ui) {
    let Some(avl) = result.avl_result.as_ref() else {
        tool_card(ui, "Athena AVL", "not_run", None, Vec::new());
        return;
    };
    let mut artifacts = vec![
        ("geometry deck".to_owned(), avl.geometry_path.clone()),
        ("session".to_owned(), avl.session_path.clone()),
        ("stdout".to_owned(), avl.stdout_path.clone()),
        ("stderr".to_owned(), avl.stderr_path.clone()),
    ];
    if let (Some(first), Some(last)) = (avl.force_paths.first(), avl.force_paths.last()) {
        artifacts.push((
            format!("force files ({} retained)", avl.force_paths.len()),
            first.clone(),
        ));
        if first != last {
            artifacts.push(("last force file".to_owned(), last.clone()));
        }
    }
    tool_card(
        ui,
        "Athena AVL",
        avl.status.as_str(),
        avl.error.as_deref(),
        artifacts,
    );
}

fn show_flowunsteady(result: &PipelineResult, ui: &mut Ui) {
    let Some(flow) = result.flowunsteady_result.as_ref() else {
        tool_card(ui, "FLOWUnsteady", "not_run", None, Vec::new());
        return;
    };
    tool_card(
        ui,
        "FLOWUnsteady",
        flow.status.as_str(),
        flow.error.as_deref(),
        vec![
            ("request".to_owned(), flow.request_path.clone()),
            ("result".to_owned(), flow.result_path.clone()),
            ("stdout".to_owned(), flow.stdout_path.clone()),
            ("stderr".to_owned(), flow.stderr_path.clone()),
        ],
    );
}

fn show_mses(result: &PipelineResult, ui: &mut Ui) {
    let Some(mses) = result.mses_result.as_ref() else {
        tool_card(ui, "MSES", "not_run", None, Vec::new());
        return;
    };
    let pressure = result.mses_pressure.as_ref();
    let pressure_ok = pressure.is_some_and(|value| {
        value.status.as_str() == "ok"
            && value.transition_model_is_valid()
            && value.has_convergence_evidence()
    });
    // Keep the polar status authoritative, but make a separately successful
    // fixed-point pressure solve visible. This is the common useful partial
    // result when a requested high-alpha polar leaves MSES's convergence
    // domain: the Mach/Cp contour figures remain backed by native mplot data.
    let status = if pressure_ok && mses.status.as_str() == "error" {
        "partial_convergence"
    } else {
        mses.status.as_str()
    };
    let mut detail = format!(
        "polar: {} converged of {} requested",
        mses.converged_alpha_count, mses.requested_alpha_count
    );
    if let Some(error) = mses.error.as_deref().filter(|error| !error.is_empty()) {
        detail.push_str(&format!("; {error}"));
    }
    if let Some(pressure) = pressure {
        detail.push_str(&format!(
            "; pressure: {} at alpha {:.3} deg (upper {}, lower {}, field {})",
            pressure.status.as_str(),
            pressure.alpha_deg,
            pressure.cp_upper.len(),
            pressure.cp_lower.len(),
            pressure.field_mach.len(),
        ));
        if let Some(error) = pressure.error.as_deref().filter(|error| !error.is_empty()) {
            detail.push_str(&format!(" ({error})"));
        }
        if !pressure.transition_model_is_valid() {
            detail.push_str(&format!(
                "; transition model unverified: {}",
                pressure
                    .osmap_diagnostic
                    .as_deref()
                    .unwrap_or("no compatible OSMAP resource was resolved")
            ));
        } else if pressure.status.as_str() == "ok" && !pressure.has_convergence_evidence() {
            detail.push_str("; native convergence evidence unavailable");
        }
    }
    tool_card(ui, "MSES", status, Some(&detail), Vec::new());
}

fn show_structures(result: &PipelineResult, ui: &mut Ui) {
    let Some(structures) = result.structural_result.as_ref() else {
        tool_card(ui, "Structures / Nastran", "not_run", None, Vec::new());
        return;
    };
    tool_card(
        ui,
        "Structures / Nastran",
        &structures.status,
        structures.error.as_deref(),
        Vec::new(),
    );
    if let Some(nastran) = structures.nastran.as_ref() {
        ui.label(
            RichText::new(format!(
                "MSC: static={}, modes={}, vibration={}",
                nastran.static_solve.status.as_str(),
                nastran.modes.status.as_str(),
                nastran.vibration.status.as_str()
            ))
            .weak()
            .small(),
        );
    }
    if let Some(nastran95) = structures.nastran95.as_ref() {
        ui.label(
            RichText::new(format!(
                "NASTRAN-95: static={}, modes={}, vibration={}",
                nastran95.static_solve.status.as_str(),
                nastran95.modes.status.as_str(),
                nastran95.vibration.status.as_str()
            ))
            .weak()
            .small(),
        );
    }
}

fn tool_card(
    ui: &mut Ui,
    title: &str,
    status: &str,
    detail: Option<&str>,
    artifacts: Vec<(String, PathBuf)>,
) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr(title)).strong().size(16.0));
            ui.label(RichText::new(status).strong().color(status_color(status)));
        });
        if let Some(detail) = detail.filter(|detail| !detail.trim().is_empty()) {
            ui.label(RichText::new(detail).weak().small());
        }
        for (role, path) in artifacts {
            artifact_row(ui, &role, &path);
        }
    });
}

fn artifact_row(ui: &mut Ui, role: &str, path: &Path) {
    let exists = path.is_file();
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(format!("{role}: {}", path.display()))
                .weak()
                .small(),
        );
        if ui
            .add_enabled(
                exists || path.parent().is_some_and(Path::is_dir),
                egui::Button::new(tr("Reveal")).small(),
            )
            .clicked()
        {
            if let Err(error) = reveal_path(path) {
                ui.colored_label(status_color("error"), error);
            }
        }
        if !exists {
            ui.label(RichText::new(tr("missing")).weak().small());
        }
    });
}

fn status_color(status: &str) -> Color32 {
    if matches!(status, "ok" | "completed_comparable" | "vsp3_materialized") {
        Color32::from_rgb(39, 174, 96)
    } else if matches!(status, "not_run" | "not_configured" | "disabled") {
        Color32::from_rgb(220, 160, 40)
    } else if status.contains("partial") || status.contains("not_comparable") {
        Color32::from_rgb(220, 125, 35)
    } else {
        Color32::from_rgb(214, 39, 40)
    }
}

fn reveal_path(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let mut command = std::process::Command::new("explorer.exe");
        if path.is_file() {
            command.arg(format!("/select,{}", path.display()));
        } else {
            command.arg(path);
        }
        command
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}
