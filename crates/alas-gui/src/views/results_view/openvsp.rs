// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Interactive launch of the full CAD model, independent of screenshot availability.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::state::AppState;
use crate::views::tr;

fn gui_next_to(location: &Path) -> Option<PathBuf> {
    let directory = if location.is_dir() {
        location
    } else {
        location.parent()?
    };
    let executable = directory.join(if cfg!(windows) { "vsp.exe" } else { "vsp" });
    executable.is_file().then_some(executable)
}

fn launch_target(state: &AppState) -> Result<(PathBuf, PathBuf), String> {
    let export = state
        .pipeline_result
        .as_ref()
        .and_then(|result| result.openvsp_export.as_ref())
        .ok_or_else(|| tr("Run the analysis to generate an OpenVSP model."))?;
    if export.status != alas_pipeline::openvsp::OpenVspExportStatus::Vsp3Materialized {
        return Err(tr(
            "The current run did not produce a verified OpenVSP model.",
        ));
    }
    // Never open the solver-only model or another run's output as a fallback.
    // Keep ordinary absolute paths: some native libraries cannot read Windows
    // verbatim (\\?\) paths returned by canonicalize.
    let model = std::path::absolute(&export.cad_preview_vsp3_path)
        .map_err(|_| tr("The full OpenVSP CAD model is unavailable. Run the analysis again."))?;
    if !model.is_file() {
        return Err(tr(
            "The full OpenVSP CAD model is unavailable. Run the analysis again.",
        ));
    }
    let configured = Path::new(state.tool_preferences.openvsp_dir.as_deref().unwrap_or(""));
    let executable = export
        .runtime_executable
        .as_deref()
        .and_then(gui_next_to)
        .or_else(|| {
            (!configured.as_os_str().is_empty())
                .then(|| gui_next_to(configured))
                .flatten()
        })
        .or_else(|| match state.tool_locator.discover_openvsp(configured) {
            alas_exec::tools::ExecutableDiscovery::Ready(runner) => gui_next_to(&runner),
            _ => None,
        })
        .ok_or_else(|| {
            tr("OpenVSP GUI is unavailable. Select its installation in External Tools.")
        })?;
    let executable = std::path::absolute(executable).map_err(|error| error.to_string())?;
    Ok((executable, model))
}

/// Center the explicit interactive action on the geometry canvas.
pub(super) fn show_launch_button(state: &mut AppState, ui: &mut egui::Ui, canvas: egui::Rect) {
    let target = launch_target(state);
    let button = egui::Rect::from_center_size(canvas.center(), egui::vec2(200.0, 36.0));
    let response = ui
        .add_enabled_ui(target.is_ok(), |ui| {
            ui.put(button, egui::Button::new(tr("Explore in OpenVSP")))
        })
        .inner;
    let error_id = ui.id().with("openvsp_launch_error");
    match target {
        Ok((executable, model)) => {
            if response
                .on_hover_text(tr(
                    "Open the full aircraft model in OpenVSP for interactive exploration.",
                ))
                .clicked()
            {
                // This is an explicit user action: show the native interactive GUI.
                match Command::new(executable)
                    .arg(&model)
                    .current_dir(model.parent().unwrap_or(Path::new(".")))
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                {
                    Ok(mut child) => {
                        std::thread::spawn(move || {
                            let _ = child.wait();
                        });
                        ui.ctx().data_mut(|data| data.remove::<String>(error_id));
                        state.status_message = tr("Opened the aircraft model in OpenVSP.");
                    }
                    Err(error) => {
                        let message = format!("{}: {error}", tr("Could not open OpenVSP"));
                        state.status_message = message.clone();
                        ui.ctx()
                            .data_mut(|data| data.insert_temp(error_id, message));
                    }
                }
            }
        }
        Err(reason) => {
            response.on_disabled_hover_text(reason);
        }
    }
    if let Some(error) = ui.ctx().data(|data| data.get_temp::<String>(error_id)) {
        let error_rect = egui::Rect::from_center_size(
            button.center() + egui::vec2(0.0, 46.0),
            egui::vec2(canvas.width().min(480.0), 48.0),
        );
        ui.put(
            error_rect,
            egui::Label::new(egui::RichText::new(error).color(ui.visuals().error_fg_color)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headless_runner_is_never_used_as_interactive_gui() {
        let root = std::env::temp_dir().join(format!("alas-openvsp-button-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let runner = root.join("vspscript.exe");
        std::fs::write(&runner, b"runner").unwrap();
        assert_eq!(gui_next_to(&runner), None);
        let gui = root.join(if cfg!(windows) { "vsp.exe" } else { "vsp" });
        std::fs::write(&gui, b"GUI").unwrap();
        assert_eq!(gui_next_to(&runner), Some(gui.clone()));
        assert_eq!(gui_next_to(&root), Some(gui));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn no_run_disables_model_launch() {
        assert!(launch_target(&AppState::default()).is_err());
    }
}
