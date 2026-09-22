// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Per-tool status cards for the Summary tab.
//!
//! One compact card per external analysis: the tool name, a colour-coded
//! class (success, warning, failure, skipped, unavailable) and a short status
//! label so colour is never the only signal. The full solver diagnostics stay
//! in the Run Log and the discipline figures; the card keeps only a hover
//! detail taken from the pipeline's own status and error strings.

use alas_pipeline::PipelineResult;
use egui::{Color32, RichText, Ui};

use crate::views::tr;

/// Outcome class shared by every external-analysis card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ToolStatusClass {
    /// The analysis ran and its result is usable.
    Success,
    /// The analysis ran but its result is partial or not comparable.
    Warning,
    /// The analysis was attempted and failed.
    Failure,
    /// The analysis was not requested for this run.
    Skipped,
    /// The analysis was requested but no usable installation was found.
    Unavailable,
}

impl ToolStatusClass {
    /// Short class word shown beside the colour marker.
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Success => "Success",
            Self::Warning => "Warning",
            Self::Failure => "Failure",
            Self::Skipped => "Skipped",
            Self::Unavailable => "Unavailable",
        }
    }

    fn color(self, ui: &Ui) -> Color32 {
        match self {
            Self::Success => crate::theme::success_color(ui.visuals()),
            Self::Warning => Color32::from_rgb(220, 125, 35),
            Self::Failure => ui.visuals().error_fg_color,
            Self::Skipped => ui.visuals().weak_text_color(),
            Self::Unavailable => Color32::from_rgb(220, 160, 40),
        }
    }
}

/// One card's content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ToolStatus {
    /// Tool or analysis name (catalogued literal).
    pub tool: &'static str,
    /// Outcome class.
    pub class: ToolStatusClass,
    /// Short status phrase (catalogued literal).
    pub label: &'static str,
    /// Retained diagnostic shown on hover, empty when there is none.
    pub detail: String,
}

/// Classify the shared runtime statuses of VSPAERO, AVL and FLOWUnsteady.
pub(super) fn classify_runtime(status: &str) -> (ToolStatusClass, &'static str) {
    match status {
        "completed_comparable" => (ToolStatusClass::Success, "Completed, comparable"),
        "completed_not_comparable" => (ToolStatusClass::Warning, "Completed, not comparable"),
        "not_configured" => (ToolStatusClass::Unavailable, "Executable not configured"),
        "timed_out" => (ToolStatusClass::Failure, "Timed out"),
        "launch_failed" => (ToolStatusClass::Failure, "Launch failed"),
        "solver_failed" => (ToolStatusClass::Failure, "Solver failed"),
        "output_missing" => (ToolStatusClass::Failure, "Output missing"),
        "parse_failed" => (ToolStatusClass::Failure, "Output not parseable"),
        "geometry_unavailable" => (ToolStatusClass::Failure, "Geometry unavailable"),
        _ => (ToolStatusClass::Failure, "Request rejected"),
    }
}

/// Classify an MSES polar status, with the fixed-point pressure solve as a
/// separately successful partial result.
pub(super) fn classify_mses(
    polar_status: &str,
    pressure_ok: bool,
) -> (ToolStatusClass, &'static str) {
    match polar_status {
        "ok" => (ToolStatusClass::Success, "Converged"),
        "partial_convergence" => (ToolStatusClass::Warning, "Partially converged"),
        "error" if pressure_ok => (ToolStatusClass::Warning, "Pressure solve only"),
        "not_run" | "disabled" => (ToolStatusClass::Skipped, "Not requested"),
        "absent" => (ToolStatusClass::Unavailable, "Installation not found"),
        "incomplete" => (ToolStatusClass::Unavailable, "Installation incomplete"),
        "launch_failure" => (ToolStatusClass::Failure, "Launch failed"),
        "timeout" => (ToolStatusClass::Failure, "Timed out"),
        "parse_failure" => (ToolStatusClass::Failure, "Output not parseable"),
        _ => (ToolStatusClass::Failure, "Did not converge"),
    }
}

/// Classify the OpenVSP geometry export.
pub(super) fn classify_openvsp(status: &str) -> (ToolStatusClass, &'static str) {
    match status {
        "vsp3_materialized" => (ToolStatusClass::Success, "Project written"),
        "script_written_runtime_unverified" => {
            (ToolStatusClass::Unavailable, "Executable not configured")
        }
        "runtime_launch_failed" => (ToolStatusClass::Failure, "Launch failed"),
        "runtime_timed_out" => (ToolStatusClass::Failure, "Timed out"),
        _ => (ToolStatusClass::Failure, "Runtime rejected the script"),
    }
}

/// Classify the structural stage from its overall status and the FEM solves.
pub(super) fn classify_structures(
    status: &str,
    fem_solves: &[&str],
) -> (ToolStatusClass, &'static str) {
    let fem_solves: Vec<&str> = fem_solves
        .iter()
        .copied()
        .filter(|solve| *solve != "not_run")
        .collect();
    match status {
        "not_run" => (ToolStatusClass::Skipped, "Not requested"),
        "ok" if fem_solves.is_empty() => (ToolStatusClass::Success, "Analytical sizing"),
        "ok" if fem_solves.iter().all(|solve| *solve == "ok") => {
            (ToolStatusClass::Success, "FEM solves completed")
        }
        "ok" => (ToolStatusClass::Warning, "FEM solve incomplete"),
        _ => (ToolStatusClass::Failure, "Structural stage failed"),
    }
}

/// Classify the Patran deformation-render export.
pub(super) fn classify_patran(status: &str) -> (ToolStatusClass, &'static str) {
    match status {
        "ok" => (ToolStatusClass::Success, "Renders exported"),
        "not_run" => (ToolStatusClass::Skipped, "Not requested"),
        _ => (ToolStatusClass::Failure, "Export failed"),
    }
}

fn skipped(tool: &'static str) -> ToolStatus {
    ToolStatus {
        tool,
        class: ToolStatusClass::Skipped,
        label: "Not requested",
        detail: String::new(),
    }
}

fn runtime_status(tool: &'static str, status: &str, error: Option<&str>) -> ToolStatus {
    let (class, label) = classify_runtime(status);
    ToolStatus {
        tool,
        class,
        label,
        detail: error.unwrap_or_default().to_owned(),
    }
}

/// Every external analysis of the run, in display order.
pub(super) fn tool_statuses(result: &PipelineResult) -> Vec<ToolStatus> {
    let openvsp = result.openvsp_export.as_ref().map_or_else(
        || skipped("OpenVSP geometry"),
        |export| {
            let (class, label) = classify_openvsp(export.status.as_str());
            let mut detail = export.runtime_error.clone().unwrap_or_default();
            if !export.preview_available {
                if !detail.is_empty() {
                    detail.push_str("; ");
                }
                detail.push_str(
                    export
                        .preview_error
                        .as_deref()
                        .unwrap_or("OpenVSP did not produce a fresh PNG"),
                );
            }
            ToolStatus {
                tool: "OpenVSP geometry",
                class,
                label,
                detail,
            }
        },
    );
    let vspaero = result.vspaero_result.as_ref().map_or_else(
        || skipped("VSPAERO"),
        |value| runtime_status("VSPAERO", value.status.as_str(), value.error.as_deref()),
    );
    let avl = result.avl_result.as_ref().map_or_else(
        || skipped("Athena AVL"),
        |value| runtime_status("Athena AVL", value.status.as_str(), value.error.as_deref()),
    );
    let flowunsteady = result.flowunsteady_result.as_ref().map_or_else(
        || skipped("FLOWUnsteady"),
        |value| {
            runtime_status(
                "FLOWUnsteady",
                value.status.as_str(),
                value.error.as_deref(),
            )
        },
    );
    let mses = result.mses_result.as_ref().map_or_else(
        || skipped("MSES"),
        |polar| {
            let pressure = result.mses_pressure.as_ref();
            let pressure_ok = pressure.is_some_and(|value| {
                value.status.as_str() == "ok"
                    && value.transition_model_is_valid()
                    && value.has_convergence_evidence()
            });
            let (class, label) = classify_mses(polar.status.as_str(), pressure_ok);
            let mut detail = format!(
                "{} of {} polar points converged",
                polar.converged_alpha_count, polar.requested_alpha_count
            );
            if let Some(error) = polar.error.as_deref().filter(|error| !error.is_empty()) {
                detail.push_str("; ");
                detail.push_str(error);
            }
            if let Some(error) = pressure
                .and_then(|value| value.error.as_deref())
                .filter(|error| !error.is_empty())
            {
                detail.push_str("; pressure: ");
                detail.push_str(error);
            }
            ToolStatus {
                tool: "MSES",
                class,
                label,
                detail,
            }
        },
    );
    let (structures, patran) = result.structural_result.as_ref().map_or_else(
        || (skipped("Structures / Nastran"), skipped("Patran renders")),
        |structural| {
            let mut fem = Vec::new();
            for solves in [structural.nastran.as_ref(), structural.nastran95.as_ref()]
                .into_iter()
                .flatten()
            {
                fem.push(solves.static_solve.status.as_str());
                fem.push(solves.modes.status.as_str());
                fem.push(solves.vibration.status.as_str());
            }
            let (class, label) = classify_structures(&structural.status, &fem);
            let patran = structural.patran.as_ref().map_or_else(
                || skipped("Patran renders"),
                |export| {
                    let (class, label) = classify_patran(&export.status);
                    ToolStatus {
                        tool: "Patran renders",
                        class,
                        label,
                        detail: export.error.clone().unwrap_or_default(),
                    }
                },
            );
            (
                ToolStatus {
                    tool: "Structures / Nastran",
                    class,
                    label,
                    detail: structural.error.clone().unwrap_or_default(),
                },
                patran,
            )
        },
    );
    vec![
        openvsp,
        vspaero,
        avl,
        flowunsteady,
        mses,
        structures,
        patran,
    ]
}

/// Render the per-tool status cards in a responsive grid.
pub(super) fn show_tool_status_cards(ui: &mut Ui, result: &PipelineResult) {
    let statuses = tool_statuses(result);
    let columns = ((ui.available_width() / 245.0).floor() as usize)
        .clamp(1, 4)
        .min(statuses.len().max(1));
    for row in statuses.chunks(columns) {
        ui.columns(columns, |column_uis| {
            for (index, status) in row.iter().enumerate() {
                show_tool_status_card(&mut column_uis[index], status);
            }
        });
        ui.add_space(8.0);
    }
}

fn show_tool_status_card(ui: &mut Ui, status: &ToolStatus) {
    let color = status.class.color(ui);
    let response = crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.vertical(|ui| {
            ui.label(RichText::new(tr(status.tool)).strong());
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(">").strong().color(color));
                ui.label(
                    RichText::new(tr(status.class.label()))
                        .strong()
                        .color(color),
                );
            });
            ui.add(egui::Label::new(RichText::new(tr(status.label)).small()).wrap());
        });
    });
    if !status.detail.trim().is_empty() {
        response.response.on_hover_text(tr(&status.detail));
    }
}

#[cfg(test)]
mod tests {
    use super::{
        classify_mses, classify_openvsp, classify_patran, classify_runtime, classify_structures,
        ToolStatusClass,
    };

    #[test]
    fn runtime_statuses_distinguish_success_warning_failure_and_unavailable() {
        assert_eq!(
            classify_runtime("completed_comparable").0,
            ToolStatusClass::Success
        );
        assert_eq!(
            classify_runtime("completed_not_comparable").0,
            ToolStatusClass::Warning
        );
        assert_eq!(
            classify_runtime("not_configured").0,
            ToolStatusClass::Unavailable
        );
        for failure in [
            "timed_out",
            "launch_failed",
            "solver_failed",
            "output_missing",
            "parse_failed",
            "geometry_unavailable",
            "deck_rejected",
            "setup_rejected",
            "request_rejected",
        ] {
            assert_eq!(
                classify_runtime(failure).0,
                ToolStatusClass::Failure,
                "{failure}"
            );
        }
    }

    #[test]
    fn mses_polar_error_with_a_valid_pressure_solve_is_a_warning_not_a_failure() {
        assert_eq!(classify_mses("error", true).0, ToolStatusClass::Warning);
        assert_eq!(classify_mses("error", false).0, ToolStatusClass::Failure);
        assert_eq!(classify_mses("disabled", false).0, ToolStatusClass::Skipped);
        assert_eq!(
            classify_mses("absent", false).0,
            ToolStatusClass::Unavailable
        );
        assert_eq!(classify_mses("ok", false).0, ToolStatusClass::Success);
    }

    #[test]
    fn openvsp_without_a_runtime_is_unavailable_rather_than_failed() {
        assert_eq!(
            classify_openvsp("script_written_runtime_unverified").0,
            ToolStatusClass::Unavailable
        );
        assert_eq!(
            classify_openvsp("vsp3_materialized").0,
            ToolStatusClass::Success
        );
        assert_eq!(
            classify_openvsp("runtime_rejected").0,
            ToolStatusClass::Failure
        );
    }

    #[test]
    fn structures_and_patran_statuses_follow_the_fem_solves() {
        assert_eq!(classify_structures("ok", &[]).1, "Analytical sizing");
        assert_eq!(
            classify_structures("ok", &["ok", "ok", "ok"]).0,
            ToolStatusClass::Success
        );
        assert_eq!(
            classify_structures("ok", &["ok", "error", "ok"]).0,
            ToolStatusClass::Warning
        );
        assert_eq!(
            classify_structures("error", &[]).0,
            ToolStatusClass::Failure
        );
        assert_eq!(
            classify_structures("not_run", &[]).0,
            ToolStatusClass::Skipped
        );
        assert_eq!(classify_patran("not_run").0, ToolStatusClass::Skipped);
        assert_eq!(classify_patran("error").0, ToolStatusClass::Failure);
    }
}
