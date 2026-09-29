// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Non-blocking ParaView rendering of the exact native case shown in Results.

use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
enum RenderStatus {
    #[default]
    Idle,
    Running,
    Finished(Result<(), String>),
}

pub(super) fn controls(
    ui: &mut egui::Ui,
    result: &alas_cfd::CfdResults,
    paraview: Option<&str>,
    missing: bool,
) -> bool {
    let id = egui::Id::new(("native-paraview-render", &result.case_dir));
    let state = ui.ctx().data_mut(|data| {
        data.get_temp_mut_or_default::<Arc<Mutex<RenderStatus>>>(id)
            .clone()
    });
    let status = state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    let running = matches!(status, RenderStatus::Running);
    let mut refresh = false;
    let mut requested = missing && matches!(status, RenderStatus::Idle) && paraview.is_some();
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(super::tr("Native ParaView fields")).strong());
        if ui
            .add_enabled(
                !running && paraview.is_some(),
                egui::Button::new(super::tr("Render / refresh fields")),
            )
            .clicked()
        {
            requested = true;
        }
        match &status {
            RenderStatus::Running => {
                ui.spinner();
                ui.label(super::tr("Rendering native fields with ParaView..."));
            }
            RenderStatus::Finished(Ok(())) => {
                refresh = true;
                ui.label(super::tr("ParaView render complete"));
            }
            RenderStatus::Finished(Err(error)) => {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            RenderStatus::Idle => {}
        }
        if paraview.is_none() {
            ui.label(super::tr(
                "Configure ParaView under External Tools to render these fields.",
            ));
        }
    });
    if refresh {
        *state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = RenderStatus::Idle;
    }
    if let Some(paraview) = paraview.filter(|_| requested) {
        let executable = std::path::Path::new(paraview).with_file_name(if cfg!(windows) {
            "pvpython.exe"
        } else {
            "pvpython"
        });
        let case = result.case_dir.clone();
        let config = result.provenance.config.clone();
        let context = ui.ctx().clone();
        *state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = RenderStatus::Running;
        std::thread::spawn(move || {
            let outcome = (|| -> Result<(), String> {
                if !executable.is_file() {
                    return Err(format!(
                        "ParaView pvpython was not found: {}",
                        executable.display()
                    ));
                }
                let directory = case.join("postProcessing/alas-field-figures");
                std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
                let script = directory.join("alas-render-fields.py");
                std::fs::write(
                    &script,
                    include_str!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../tools/openfoam_render_fields.py"
                    )),
                )
                .map_err(|e| e.to_string())?;
                let mut command = std::process::Command::new(executable);
                command
                    .arg(&script)
                    .arg(&case)
                    .arg(config.density_kg_m3.to_string())
                    .arg(config.freestream_temperature_k.to_string());
                if config.effective_simulation().compressible {
                    command
                        .arg("--compressible")
                        .arg("--pressure-reference-pa")
                        .arg(config.effective_static_pressure_pa().to_string());
                }
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    command.creation_flags(0x0800_0000);
                }
                // Stream diagnostics to disk rather than accumulating an
                // unbounded capture in memory while ParaView runs.
                command.stdout(
                    std::fs::File::create(directory.join("renderer.stdout.log"))
                        .map_err(|e| e.to_string())?,
                );
                command.stderr(
                    std::fs::File::create(directory.join("renderer.stderr.log"))
                        .map_err(|e| e.to_string())?,
                );
                command.stdin(std::process::Stdio::null());
                let mut child = command.spawn().map_err(|e| e.to_string())?;
                let started = std::time::Instant::now();
                let status = loop {
                    match child.try_wait() {
                        Ok(Some(status)) => break status,
                        Ok(None) if started.elapsed() < std::time::Duration::from_secs(120) => {
                            std::thread::sleep(std::time::Duration::from_millis(100));
                        }
                        result => {
                            let reason = match result {
                                Err(error) => format!("cannot poll ParaView: {error}"),
                                _ => "ParaView exceeded the 120 s rendering deadline".to_owned(),
                            };
                            let _ = child.kill();
                            let _ = child.wait();
                            return Err(format!(
                                "{reason}; see {}",
                                directory.join("renderer.stderr.log").display()
                            ));
                        }
                    }
                };
                if !status.success() {
                    return Err(format!(
                        "ParaView rendering failed ({}); see {}",
                        status,
                        directory.join("renderer.stderr.log").display()
                    ));
                }
                for filename in ["mach-contour.png", "pressure-contour.png"] {
                    if !directory.join(filename).is_file() {
                        return Err(format!(
                            "ParaView completed without {filename}; see {}",
                            directory.display()
                        ));
                    }
                }
                Ok(())
            })();
            *state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                RenderStatus::Finished(outcome);
            context.request_repaint();
        });
    }
    refresh
}
