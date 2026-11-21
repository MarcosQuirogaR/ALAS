// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! "Install all" card: one consent, hash-pinned downloads, progress, health
//! checks, and uninstall. The engine is `alas_exec::installer`.

use alas_exec::installer::{PlanItemState, ToolResult};

use super::*;
use crate::tool_installer::{describe_result, ToolInstaller};

fn megabytes(bytes: u64) -> String {
    format!("{:.0} MB", bytes as f64 / 1.0e6)
}

fn state_label(item: &alas_exec::installer::PlanItem) -> &'static str {
    match &item.state {
        PlanItemState::Install if item.builds_from_source => "Will Download and Compile",
        PlanItemState::Install => "Will Install",
        PlanItemState::AlreadyInstalled => "Installed",
        PlanItemState::PendingReview => "Not Offered",
        PlanItemState::MissingPin => "Not Offered",
        PlanItemState::UnsupportedPlatform => "Unsupported Platform",
        PlanItemState::Guided => "Manual Install",
        PlanItemState::MissingSystemTools => "Install System Tools First",
    }
}

pub(super) fn installer_card(state: &mut AppState, ui: &mut Ui) {
    card(
        ui,
        "Install External Tools",
        "https://github.com/MarcosQuirogaR/ALAS",
        &[],
        |ui| {
            let data_root = state.tool_locator.user_data_root().to_path_buf();
            let running = state.tool_installer.running;
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!running, egui::Button::new(tr("Install All Tools")))
                    .clicked()
                {
                    state.tool_installer.prepare(data_root.clone());
                }
                if running && ui.button(tr("Cancel")).clicked() {
                    state.tool_installer.cancel();
                }
            });
            if let Some(error) = state.tool_installer.error.clone() {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            consent_panel(state, ui);
            progress_rows(state, ui);
            result_rows(state, ui, &data_root);
        },
    );
}

fn consent_panel(state: &mut AppState, ui: &mut Ui) {
    let Some(plan) = state.tool_installer.plan.clone() else {
        return;
    };
    ui.add_space(6.0);
    ui.separator();
    for item in &plan.items {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(&item.name).strong());
            ui.label(RichText::new(format!("({})", item.licence)).weak());
        });
        ui.label(RichText::new(tr(state_label(item))).small());
        if item.downloads.is_empty() {
            if let (Some(url), Some(bytes)) = (&item.url, item.download_bytes) {
                ui.label(
                    RichText::new(format!("{url}  ~{}", megabytes(bytes)))
                        .small()
                        .weak(),
                );
            }
        } else {
            for download in &item.downloads {
                ui.label(
                    RichText::new(download.describe_with(&|text| tr(text)))
                        .small()
                        .weak(),
                );
            }
            if let Some(bytes) = item.build_disk_bytes {
                ui.label(
                    RichText::new(format!(
                        "{} ~{}",
                        tr("Disk needed while compiling"),
                        megabytes(bytes)
                    ))
                    .small()
                    .weak(),
                );
            }
        }
        for (number, step) in item.steps.iter().enumerate() {
            ui.label(RichText::new(format!("{}. {step}", number + 1)).small());
        }
        ui.hyperlink_to(
            RichText::new(tr("Publisher Page")).small(),
            &item.publisher_url,
        );
        ui.add_space(4.0);
    }
    let downloads = plan.downloads().count();
    ui.horizontal(|ui| {
        if downloads > 0 {
            let label = format!(
                "{} ({}, {})",
                tr("Download and Install"),
                downloads,
                megabytes(plan.total_download_bytes())
            );
            if ui.button(label).clicked() {
                state.tool_installer.confirm();
            }
        } else {
            ui.label(RichText::new(tr("Nothing to Download")).weak());
        }
        if ui.button(tr("Close")).clicked() {
            state.tool_installer.dismiss();
        }
    });
}

fn progress_rows(state: &AppState, ui: &mut Ui) {
    for (id, progress) in &state.tool_installer.progress {
        if state.tool_installer.running || state.tool_installer.reports.is_empty() {
            let fraction = if progress.total > 0 {
                (progress.done as f32 / progress.total as f32).min(1.0)
            } else {
                0.0
            };
            ui.label(format!("{id}: {}", tr(&progress.stage)));
            if progress.stage == "Downloading" {
                ui.add(egui::ProgressBar::new(fraction).text(format!(
                    "{} / {}",
                    megabytes(progress.done),
                    megabytes(progress.total)
                )));
            }
        }
    }
}

fn result_rows(state: &mut AppState, ui: &mut Ui, data_root: &std::path::Path) {
    let reports = state.tool_installer.reports.clone();
    if reports.is_empty() || state.tool_installer.running {
        return;
    }
    ui.add_space(6.0);
    for report in &reports {
        let (label, detail) = describe_result(&report.result);
        let mut line = format!("{}: {}", report.id, tr(label));
        if let Some(detail) = detail {
            line.push_str(&format!(" - {detail}"));
        }
        ui.label(line);
        if let ToolResult::Guided(steps) = &report.result {
            for (number, step) in steps.iter().enumerate() {
                ui.label(RichText::new(format!("{}. {step}", number + 1)).small());
            }
        }
    }
    for report in &reports {
        let installed = ToolInstaller::installed_executable(data_root, &report.id).is_some();
        if installed
            && crate::theme::danger_button(ui, format!("{} {}", tr("Uninstall"), report.id))
                .clicked()
        {
            match state
                .tool_installer
                .uninstall(data_root.to_path_buf(), &report.id)
            {
                Ok(_) => {
                    state.log(
                        format!("Removed {}.", report.id),
                        crate::state::LogKind::Info,
                    );
                    if let Some(note) = alas_exec::installer::toolchain_leftover_note(data_root) {
                        state.log(note, crate::state::LogKind::Info);
                    }
                }
                Err(error) => state.log(error, crate::state::LogKind::Warn),
            }
            state.tool_installer.reports.retain(|r| r.id != report.id);
        }
    }
}
