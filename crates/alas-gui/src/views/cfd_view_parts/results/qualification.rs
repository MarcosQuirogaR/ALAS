// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mesh-qualification and physical-plausibility reporting for the Results tab.
//!
//! Both are read from typed `alas-cfd` records; no classification is repeated
//! here.

use super::super::widgets::count_value;
use super::{tr, tr_fields};
use egui::{RichText, Ui};

/// A record written before `mesh_qualification` and `numerical_convergence`
/// existed carries neither, and both `#[serde(default)]` values would read as a
/// verdict that was never reached.
///
/// `qualify_mesh` always emits at least the `require_check_mesh_ok` check, so a
/// populated record can never have an empty check list. An empty list is
/// therefore the discriminator for "this predates the contract", and both
/// fields are reported as not recorded rather than as the defaults
/// (`passed: false` and `Failed`), which would invent a failure.
fn qualification_recorded(qualification: &alas_cfd::MeshQualification) -> bool {
    !qualification.checks.is_empty()
}

/// The three verdicts, kept apart: the solver's own convergence, the declared
/// mesh contract, and the combined outcome the record reports.
///
/// None of them is computed here. `numerical_convergence`, `mesh_qualification`
/// and `outcome` are read from the typed record, and the summary text comes
/// from `MeshQualification::summary`.
pub(super) fn show_qualification_notes(result: &alas_cfd::CfdResults, ui: &mut Ui) {
    let qualification = &result.mesh_qualification;
    let recorded = qualification_recorded(qualification);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(tr("Solver convergence")).strong().small())
            .on_hover_text(tr("The numerical verdict on its own, before the declared mesh contract is applied. It stays visible even when a mesh failure overrides the combined outcome."));
        if recorded {
            let (color, label) = outcome_style(result.numerical_convergence, ui);
            ui.colored_label(color, tr(label));
        } else {
            ui.label(RichText::new(tr("Not recorded")).weak())
                .on_hover_text(tr("This result predates the separated verdicts; the record carries only its combined outcome."));
        }
        ui.separator();
        ui.label(RichText::new(tr("Mesh qualification")).strong().small())
            .on_hover_text(tr("The declared numeric mesh limits, checked against what was measured. This is ALAS's own contract, not checkMesh's verdict."));
        if recorded {
            // `passed` is what the combined outcome uses, but it is true for a
            // mesh whose contract is merely unproven. Only `standing`
            // distinguishes that, so only `standing` may colour this chip:
            // "passed on measured checks only" must never render green.
            let (color, label) = match qualification.standing {
                alas_cfd::MeshQualificationStanding::FullyQualified => (
                    crate::theme::success_color(ui.visuals()),
                    "Fully qualified",
                ),
                alas_cfd::MeshQualificationStanding::PassedOnMeasuredChecksOnly => (
                    ui.visuals().warn_fg_color,
                    "Passed on measured checks only (unproven)",
                ),
                alas_cfd::MeshQualificationStanding::Failed => {
                    (ui.visuals().error_fg_color, "FAILED")
                }
            };
            ui.colored_label(color, tr(label));
        } else {
            ui.label(RichText::new(tr("Not recorded")).weak())
                .on_hover_text(tr("This result predates the declared-limit contract, so no check was evaluated. Not recorded is not a pass."));
        }
        ui.separator();
        ui.label(RichText::new(tr("Combined outcome")).strong().small())
            .on_hover_text(tr("The worse of the two verdicts, as recorded by the run."));
        let (color, label) = outcome_style(result.outcome, ui);
        ui.colored_label(color, tr(label));
    });
    if recorded {
        show_qualification_findings(qualification, ui);
    }
    show_severe_faces(qualification, &result.mesh_quality, ui);
    ui.label(
        RichText::new(tr(
            "checkMesh is the generator's own check, not this study's acceptance.",
        ))
        .weak()
        .small(),
    )
    .on_hover_text(tr("Mesh OK reports that checkMesh found no failed check. The declared mesh limits and the residual, force-stability and continuity gates are reported separately; none implies another."));
}

/// Actionable findings first, the full declared table behind a named
/// expander, so a clean result stays one line and a failure is never buried.
fn show_qualification_findings(qualification: &alas_cfd::MeshQualification, ui: &mut Ui) {
    let failures = qualification.failures();
    let unmeasured = qualification.unmeasured();
    if !failures.is_empty() {
        ui.colored_label(ui.visuals().error_fg_color, qualification.summary());
    } else if !unmeasured.is_empty() {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            tr_fields(
                "{count} declared check(s) are NOT MEASURED and therefore unproven.",
                &[("count", count_value(unmeasured.len() as u64))],
            ),
        )
        .on_hover_text(qualification.summary());
    }
    super::super::widgets::details(
        ui,
        "airfoil_cfd_mesh_checks",
        "Declared mesh checks",
        |ui| {
            egui::Grid::new("airfoil_cfd_mesh_check_grid")
                .num_columns(5)
                .striped(true)
                .spacing([10.0, 3.0])
                .show(ui, |ui| {
                    for heading in ["Check", "Status", "Declared limit", "Measured", "Evidence"] {
                        ui.label(RichText::new(tr(heading)).strong().small());
                    }
                    ui.end_row();
                    for check in &qualification.checks {
                        ui.monospace(&check.name);
                        let (color, label) = check_style(check.status, ui);
                        ui.colored_label(color, tr(label));
                        ui.monospace(limit_text(check.limit));
                        ui.monospace(limit_text(check.measured));
                        ui.label(RichText::new(&check.provenance).weak().small());
                        ui.end_row();
                    }
                });
        },
    );
}

/// The severe-face warning, with the prevalence fraction when the record
/// carries it.
fn show_severe_faces(
    qualification: &alas_cfd::MeshQualification,
    quality: &alas_cfd::MeshQuality,
    ui: &mut Ui,
) {
    let faces = qualification
        .severely_non_orthogonal_faces
        .or(quality.severely_non_orthogonal_faces)
        .filter(|count| *count > 0);
    let Some(faces) = faces else {
        return;
    };
    // A face count over a cell count is not a fraction of the mesh's faces, so
    // it is never shown as a percentage. The record names it per cell and so
    // does this line.
    let prevalence = qualification
        .severely_non_orthogonal_faces_per_cell
        .map(|ratio| format!(", {ratio:.3e} per cell"))
        .unwrap_or_default();
    ui.colored_label(
        ui.visuals().warn_fg_color,
        tr_fields(
            "checkMesh counted {count} face(s) past its 70 deg severe line{prevalence}, alongside Mesh OK.",
            &[("count", count_value(faces)), ("prevalence", prevalence)],
        ),
    )
    .on_hover_text(tr("The maximum angle alone cannot be read without this count: one outlier face in a large mesh is a different mesh from one where thousands are. The fraction is prevalence, not influence: neither the outlier's location nor its effect on the integrated loads was measured."));
}

fn outcome_style(outcome: alas_cfd::CfdOutcome, ui: &Ui) -> (egui::Color32, &'static str) {
    match outcome {
        alas_cfd::CfdOutcome::NumericallyConverged => (
            crate::theme::success_color(ui.visuals()),
            "Numerically converged",
        ),
        alas_cfd::CfdOutcome::Unconverged => (ui.visuals().warn_fg_color, "Unconverged"),
        alas_cfd::CfdOutcome::Cancelled => (ui.visuals().warn_fg_color, "Cancelled"),
        alas_cfd::CfdOutcome::Failed => (ui.visuals().error_fg_color, "Failed"),
    }
}

fn check_style(status: alas_cfd::MeshCheckStatus, ui: &Ui) -> (egui::Color32, &'static str) {
    match status {
        alas_cfd::MeshCheckStatus::Passed => (crate::theme::success_color(ui.visuals()), "passed"),
        alas_cfd::MeshCheckStatus::Failed => (ui.visuals().error_fg_color, "FAILED"),
        alas_cfd::MeshCheckStatus::NotMeasured => (ui.visuals().warn_fg_color, "not measured"),
    }
}

/// A declared limit or a measured value; a check with neither is not numeric.
fn limit_text(value: Option<f64>) -> String {
    value.map_or_else(|| tr("n/a"), super::super::widgets::physical_value)
}

/// Field-update evidence: which solved fields actually moved between the last
/// two written times.
///
/// The record distinguishes a converged equation from an abandoned one, which
/// a residual history cannot. It is displayed, not interpreted: the classifier
/// that consumes it lives in `alas-cfd` and its conclusion already arrives as
/// the run outcome. The one distinction this must not blur is the contract's
/// own: an absent comparison reads as **not observed**, never as "not
/// updated".
pub(super) fn show_field_updates(evidence: &alas_cfd::FieldUpdateEvidence, ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(tr("Written-field change")).strong().small())
            .on_hover_text(tr("Whether each solved field's written values differ between the last two written times, at the precision they were written with. This is an observation about the stored representation, not a verdict: a change smaller than the write precision, or one confined to a boundary patch, looks the same as no change."));
        if evidence.samples.is_empty() && evidence.unavailable_reason.is_none() {
            ui.label(RichText::new(tr("Not recorded")).weak().small())
                .on_hover_text(tr("This result predates the field-update comparison. Not recorded is not an observation of no change."));
            return;
        }
        for sample in &evidence.samples {
            // "unchanged" would read as a stopped equation. What was actually
            // observed is that nothing was persisted at the write precision,
            // which the contract's own `observation()` states exactly.
            let (color, state) = if sample.updated() {
                (ui.visuals().text_color(), tr("differs"))
            } else {
                (
                    ui.visuals().warn_fg_color,
                    match sample.write_precision.or(evidence.write_precision) {
                        Some(digits) => tr_fields(
                            "no change at {digits}-digit precision",
                            &[("digits", digits.to_string())],
                        ),
                        None => tr("no change at the written precision"),
                    },
                )
            };
            ui.label(RichText::new(&sample.field).monospace().small());
            ui.colored_label(color, RichText::new(state).small())
                .on_hover_text(sample.observation());
        }
        if evidence.write_pair_regular == Some(false) {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                RichText::new(tr("irregular write pair")).small(),
            )
            .on_hover_text(tr("The two compared times are not one regular write interval apart, so the comparison spans an unknown amount of solver progress."));
        }
        if let Some(reason) = evidence.unavailable_reason.as_deref() {
            ui.label(RichText::new(tr("not observed")).weak().small())
                .on_hover_text(reason.to_owned());
        }
    });
}

/// The typed physical-plausibility screen, consumed from `alas-cfd`.
///
/// The classifier lives in the solver crate and is called here; the verdict is
/// never recomputed in the UI. It screens reported coefficients and wall state
/// against what a two-dimensional section can produce, and is explicitly not a
/// substitute for the numerical gates.
pub(super) fn show_plausibility(result: &alas_cfd::CfdResults, ui: &mut Ui) {
    use alas_cfd::PlausibilityVerdict;
    let screen = alas_cfd::assess_physical_plausibility(&result.forces, &result.mesh_quality);
    let (color, label) = match screen.verdict {
        PlausibilityVerdict::Plausible => (
            crate::theme::success_color(ui.visuals()),
            "Inside the section envelope",
        ),
        PlausibilityVerdict::Implausible => (ui.visuals().error_fg_color, "Outside the envelope"),
        PlausibilityVerdict::NotEvaluated => (ui.visuals().weak_text_color(), "Not evaluated"),
    };
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(tr("Physical plausibility")).strong().small())
            .on_hover_text(tr("A screen against the envelope a two-dimensional section can produce. It refuses impossible numbers; it is never a substitute for the residual, force-stability or continuity gates."));
        ui.colored_label(color, tr(label));
        if screen.verdict != PlausibilityVerdict::Plausible {
            ui.label(RichText::new(screen.detail()).weak().small());
        }
    });
}
