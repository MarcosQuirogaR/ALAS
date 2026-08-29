// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Model-specific context for the whole-aircraft comparison result.
//!
//! The chart is kept for quantitative comparison. Method scope, optional
//! solver status, and retained diagnostics live here so they stay readable
//! and do not consume the plotting canvas.

use alas_pipeline::{AvlAnalysisResult, PipelineResult, VspaeroAnalysisStatus};
use egui::{Id, RichText, Ui};

use crate::theme::{card_content_width, card_frame, selectable_button};
use crate::views::tr;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum ModelTab {
    #[default]
    AlasVlm,
    FourierLiftingLine,
    PrandtlLiftingLine,
    Helmbold,
    Vspaero,
    AthenaAvl,
    Mses,
}

impl ModelTab {
    const ALL: [Self; 7] = [
        Self::AlasVlm,
        Self::FourierLiftingLine,
        Self::PrandtlLiftingLine,
        Self::Helmbold,
        Self::Vspaero,
        Self::AthenaAvl,
        Self::Mses,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::AlasVlm => "ALAS VLM",
            Self::FourierLiftingLine => "Fourier lifting-line",
            Self::PrandtlLiftingLine => "Prandtl lifting-line",
            Self::Helmbold => "Helmbold",
            Self::Vspaero => "VSPAERO",
            Self::AthenaAvl => "Athena AVL",
            Self::Mses => "MSES 2-D section",
        }
    }
}

struct ModelDetail {
    title: &'static str,
    summary: String,
    diagnostic: Option<String>,
}

fn selection_id(run_identity: u64) -> Id {
    Id::new(("alas_model_comparison_detail", run_identity))
}

fn selected_tab(ctx: &egui::Context, run_identity: u64) -> ModelTab {
    ctx.data(|data| data.get_temp::<ModelTab>(selection_id(run_identity)))
        .unwrap_or_default()
}

fn set_selected_tab(ctx: &egui::Context, run_identity: u64, tab: ModelTab) {
    ctx.data_mut(|data| data.insert_temp(selection_id(run_identity), tab));
}

/// Show the compact method and status panel associated with Model Comparison.
pub(super) fn show_model_details(
    ui: &mut Ui,
    run_identity: u64,
    result: &PipelineResult,
    outer_width: f32,
) {
    let selected = selected_tab(ui.ctx(), run_identity);
    let mut next = selected;
    card_frame(ui).show(ui, |ui| {
        let content_width = card_content_width(outer_width);
        ui.set_min_width(content_width);
        ui.set_max_width(content_width);
        ui.label(RichText::new(tr("Model details")).strong());
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for tab in ModelTab::ALL {
                if ui
                    .add(selectable_button(tr(tab.label()), selected == tab))
                    .clicked()
                {
                    next = tab;
                }
            }
        });
        ui.add_space(4.0);
        ui.separator();

        let detail = model_detail(next, result);
        ui.label(RichText::new(tr(detail.title)).strong());
        ui.label(detail.summary);
        if let Some(diagnostic) = detail.diagnostic {
            ui.add_space(2.0);
            ui.collapsing(tr("Show solver detail"), |ui| {
                ui.label(RichText::new(diagnostic).weak().small());
            });
        }
    });
    if next != selected {
        set_selected_tab(ui.ctx(), run_identity, next);
    }
}

fn model_detail(tab: ModelTab, result: &PipelineResult) -> ModelDetail {
    match tab {
        ModelTab::AlasVlm => ModelDetail {
            title: "ALAS VLM",
            summary: tr(
                "The primary whole-aircraft aerodynamic solution. Its solid blue curves provide the common reference for the comparison.",
            ),
            diagnostic: None,
        },
        ModelTab::FourierLiftingLine => ModelDetail {
            title: "ALAS local Fourier lifting-line",
            summary: tr(
                "An in-process Rust lifting-line model using the aircraft chord, twist, and airfoil camber. It represents isolated incompressible lifting surfaces and reports CL only; its CDi is never overlaid on total CD.",
            ),
            diagnostic: None,
        },
        ModelTab::PrandtlLiftingLine => ModelDetail {
            title: "Prandtl lifting-line",
            summary: tr(
                "A classical lifting-line estimate with a 2pi/rad section lift slope. Its zero-lift angle comes from planform-weighted airfoil camber and twist; CD0 and span efficiency come from the aircraft fit.",
            ),
            diagnostic: None,
        },
        ModelTab::Helmbold => ModelDetail {
            title: "Helmbold lift slope",
            summary: tr(
                "An aspect-ratio-only lift-slope estimate. It is intentionally shown only on the lift panel, where that limited scope remains clear.",
            ),
            diagnostic: None,
        },
        ModelTab::Vspaero => vspaero_detail(result),
        ModelTab::AthenaAvl => avl_detail(result),
        ModelTab::Mses => mses_detail(result),
    }
}

fn vspaero_detail(result: &PipelineResult) -> ModelDetail {
    let Some(vspaero) = result.vspaero_result.as_ref() else {
        return ModelDetail {
            title: "VSPAERO",
            summary: tr(
                "This optional three-dimensional VLM run was not requested for this result.",
            ),
            diagnostic: None,
        };
    };
    if let Some(polar) = vspaero.comparable_polar() {
        return ModelDetail {
            title: "VSPAERO",
            summary: tr(&format!(
                "Completed at {} alpha values. CL and Cm passed the reference and frame checks and are shown as solid vermilion curves; induced drag remains separate from total CD.",
                polar.points.len()
            )),
            diagnostic: None,
        };
    }
    let summary = if vspaero.status == VspaeroAnalysisStatus::GeometryUnavailable {
        tr(
            "No comparable VSPAERO result is available because OpenVSP did not supply the required lifting-surface mesh.",
        )
    } else {
        tr(&format!(
            "No VSPAERO values are shown. Solver status: {}.",
            vspaero.status.as_str()
        ))
    };
    ModelDetail {
        title: "VSPAERO",
        summary,
        diagnostic: vspaero.error.clone(),
    }
}

fn retained_avl(result: &PipelineResult) -> Option<&AvlAnalysisResult> {
    result.avl_result.as_ref().or_else(|| {
        result
            .solver_optimizations
            .as_ref()
            .and_then(|solutions| solutions.avl.avl_result.as_ref())
    })
}

fn avl_detail(result: &PipelineResult) -> ModelDetail {
    let Some(avl) = retained_avl(result) else {
        return ModelDetail {
            title: "Athena AVL",
            summary: tr(
                "This optional whole-aircraft cross-check was not requested for this result.",
            ),
            diagnostic: None,
        };
    };
    if let Some(polar) = avl.comparable_polar() {
        let condition = avl.comparison_reference.as_ref().map_or_else(
            || tr("the retained comparison condition"),
            |reference| {
                tr(&format!(
                    "{} (M={:.3}, h={:.0} m)",
                    reference.phase, reference.mach, reference.altitude_m
                ))
            },
        );
        return ModelDetail {
            title: "Athena AVL",
            summary: tr(&format!(
                "Completed at {} alpha values under {condition}. The dedicated cross-check panels compare CL, Cm, CDi, and span efficiency with ALAS VLM; AVL total drag is not compared with ALAS total CD.",
                polar.points.len()
            )),
            diagnostic: None,
        };
    }
    let summary = if avl.status == alas_pipeline::AvlAnalysisStatus::NotConfigured {
        tr(&format!(
            "AVL is not configured. The inspectable deck was exported to {}.",
            avl.geometry_path.display()
        ))
    } else {
        tr(&format!(
            "No AVL values are shown. Solver status: {}.",
            avl.status.as_str()
        ))
    };
    ModelDetail {
        title: "Athena AVL",
        summary,
        diagnostic: avl.error.clone(),
    }
}

fn mses_detail(result: &PipelineResult) -> ModelDetail {
    let Some(mses) = result.mses_result.as_ref() else {
        return ModelDetail {
            title: "MSES 2-D section",
            summary: tr(
                "This optional two-dimensional section analysis was not requested for this result.",
            ),
            diagnostic: None,
        };
    };
    if mses.has_usable_data() {
        let convergence = if mses.is_complete() {
            tr("The requested sweep fully converged.")
        } else {
            let nonconverged = mses
                .nonconverged_alpha_deg()
                .iter()
                .map(|alpha| format!("{alpha:.4}"))
                .collect::<Vec<_>>()
                .join(", ");
            if nonconverged.is_empty() {
                tr(&format!(
                    "{} of {} requested points converged.",
                    mses.converged_alpha_count, mses.requested_alpha_count
                ))
            } else {
                tr(&format!(
                    "{} of {} requested points converged; no convergence at alpha = {nonconverged} deg.",
                    mses.converged_alpha_count, mses.requested_alpha_count
                ))
            }
        };
        return ModelDetail {
            title: "MSES 2-D section",
            summary: tr(&format!(
                "{} at M={:.3} and Re={:.3e}. {convergence} The section coefficients remain in dedicated MSES figures and are never overlaid on whole-aircraft coefficients.",
                mses.airfoil_name, mses.mach, mses.reynolds
            )),
            diagnostic: mses.error.clone(),
        };
    }
    ModelDetail {
        title: "MSES 2-D section",
        summary: tr(&format!(
            "No usable MSES section polar is available. Solver status: {}. Section coefficients are never overlaid on whole-aircraft coefficients.",
            mses.status.as_str()
        )),
        diagnostic: mses.error.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::{selected_tab, set_selected_tab, ModelTab};
    use egui::Context;

    #[test]
    fn model_detail_selection_is_isolated_by_run_identity() {
        let context = Context::default();
        set_selected_tab(&context, 11, ModelTab::AthenaAvl);
        set_selected_tab(&context, 12, ModelTab::Mses);

        assert_eq!(selected_tab(&context, 11), ModelTab::AthenaAvl);
        assert_eq!(selected_tab(&context, 12), ModelTab::Mses);
        assert_eq!(selected_tab(&context, 13), ModelTab::AlasVlm);
    }
}
