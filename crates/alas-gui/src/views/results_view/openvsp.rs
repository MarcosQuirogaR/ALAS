// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Interactive launch of the full CAD model, independent of screenshot availability.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::state::AppState;
use crate::views::tr;

/// How long a resolved (or failed) launch target stays valid.
///
/// `launch_target` probes the filesystem (`is_file`, tool discovery) and is
/// evaluated once per figure per frame while the OpenVSP card is visible.
/// Its inputs, an installed run and the configured tool locations, change on
/// user action rather than every repaint, so a short TTL avoids redoing that
/// I/O on idle frames while still picking up an install or a new run
/// promptly.
const LAUNCH_TARGET_TTL: Duration = Duration::from_secs(5);

#[derive(Clone)]
struct CachedLaunchTarget {
    key: String,
    computed_at: Instant,
    result: Result<(PathBuf, PathBuf), String>,
}

/// Identity of everything `launch_target` reads, cheap to recompute every
/// frame from data already resident in `state`.
fn launch_target_cache_key(state: &AppState) -> String {
    let export = state
        .pipeline_result
        .as_ref()
        .and_then(|result| result.openvsp_export.as_ref());
    match export {
        Some(export) => format!(
            "{:?}|{}|{}|{}",
            export.status,
            export.cad_preview_vsp3_path.display(),
            export
                .runtime_executable
                .as_deref()
                .map(Path::display)
                .map(|path| path.to_string())
                .unwrap_or_default(),
            state.tool_preferences.openvsp_dir.as_deref().unwrap_or(""),
        ),
        None => "none".to_owned(),
    }
}

/// `launch_target`, memoized behind [`LAUNCH_TARGET_TTL`] and the inputs it
/// reads so an idle repaint reuses last frame's answer instead of probing
/// the filesystem again.
fn cached_launch_target(
    ctx: &egui::Context,
    state: &AppState,
) -> Result<(PathBuf, PathBuf), String> {
    let id = egui::Id::new("openvsp_launch_target_cache");
    let key = launch_target_cache_key(state);
    if let Some(cached) = ctx.data(|data| data.get_temp::<CachedLaunchTarget>(id)) {
        if cached.key == key && cached.computed_at.elapsed() < LAUNCH_TARGET_TTL {
            return cached.result;
        }
    }
    let result = launch_target(state);
    ctx.data_mut(|data| {
        data.insert_temp(
            id,
            CachedLaunchTarget {
                key,
                computed_at: Instant::now(),
                result: result.clone(),
            },
        )
    });
    result
}

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

/// Height the launch row takes below the geometry canvas, in points.
pub(super) const LAUNCH_ROW_HEIGHT: f32 = 44.0;

/// The explicit interactive action, in its own row centred below the
/// geometry canvas: over the canvas it would cover the preview image or the
/// status message drawn when no preview exists. Returns the button's rect.
pub(super) fn show_launch_button(state: &mut AppState, ui: &mut egui::Ui) -> egui::Rect {
    let target = cached_launch_target(ui.ctx(), state);
    ui.add_space(LAUNCH_ROW_HEIGHT - 36.0);
    let response = ui
        .vertical_centered(|ui| {
            ui.add_enabled_ui(target.is_ok(), |ui| {
                ui.add_sized([200.0, 36.0], egui::Button::new(tr("Explore in OpenVSP")))
            })
            .inner
        })
        .inner;
    let button = response.rect;
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
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new(error).color(ui.visuals().error_fg_color));
        });
    }
    button
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

    #[test]
    fn an_expired_entry_is_recomputed() {
        let ctx = egui::Context::default();
        let state = AppState::default();
        let id = egui::Id::new("openvsp_launch_target_cache");
        ctx.data_mut(|data| {
            data.insert_temp(
                id,
                CachedLaunchTarget {
                    key: launch_target_cache_key(&state),
                    computed_at: Instant::now() - Duration::from_secs(6),
                    result: Ok((PathBuf::from("stale-exe"), PathBuf::from("stale-model"))),
                },
            )
        });

        let refreshed = cached_launch_target(&ctx, &state);

        assert_ne!(
            refreshed,
            Ok((PathBuf::from("stale-exe"), PathBuf::from("stale-model")))
        );
    }
}
