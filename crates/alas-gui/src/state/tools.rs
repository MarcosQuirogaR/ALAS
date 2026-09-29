// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Machine-tool preferences and background downloads and installs of optional tools.

use std::sync::Arc;

use super::types::LogKind;
use super::AppState;
use alas_config::AlasConfig;
use alas_exec::ToolPreferences;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, TryRecvError};
use std::thread;

/// Apply only the user-owned machine locations to a freshly selected design.
/// These locations describe the local environment, not an aircraft, so a
/// preset must never replace a person's solver, navigation-data, or route
/// folders with a checkout-relative default.
pub(super) fn apply_tool_preferences(config: &mut AlasConfig, preferences: &ToolPreferences) {
    if let Some(path) = &preferences.mses_dir {
        config.mses.mses_dir.clone_from(path);
    }
    if let Some(path) = &preferences.nastran_exe {
        config.structures.nastran_exe_path.clone_from(path);
    }
    if let Some(path) = &preferences.nastran_solver {
        config.structures.nastran_solver_path.clone_from(path);
    }
    if let Some(path) = &preferences.nastran95_dir {
        config.structures.nastran95_dir_path.clone_from(path);
    }
    if let Some(path) = &preferences.nastran95_runtime {
        config.structures.nastran95_runtime_path.clone_from(path);
    }
    if let Some(path) = &preferences.nastran95_rf_stage {
        config.structures.nastran95_rf_stage_path.clone_from(path);
    }
    if let Some(words) = &preferences.nastran95_open_core_words {
        config
            .structures
            .nastran95_open_core_words
            .clone_from(words);
    }
    if let Some(path) = &preferences.patran_exe {
        config.structures.patran_exe_path.clone_from(path);
    }
    if let Some(path) = &preferences.navdata_dir {
        config.mission.navdata_dir.clone_from(path);
    }
    if let Some(path) = &preferences.routes_dir {
        config.mission.routes_dir.clone_from(path);
    }
}

impl AppState {
    /// Start the shared navdata downloader without blocking an egui frame.
    pub fn start_navdata_download(&mut self, configured: &str) {
        if self.navdata_download_in_progress {
            return;
        }
        let configured = if configured.trim().is_empty() {
            alas_route::assets::NAVDATA_REL.to_owned()
        } else {
            configured.trim().to_owned()
        };
        let target = self
            .tool_locator
            .resolve_data_path(std::path::Path::new(&configured));
        let specs = alas_route::assets::NAVDATA_FILES
            .iter()
            .map(|file| {
                let spec = alas_exec::download::DownloadSpec::new(
                    file.name,
                    alas_route::assets::navdata_file_url(file),
                    file.min_bytes,
                );
                match file.expected_sha256 {
                    Some(hash) => spec.with_reviewed_sha256(hash),
                    None => spec,
                }
            })
            .collect::<Vec<_>>();
        let (sender, receiver) = channel();
        self.navdata_download_rx = Some(receiver);
        self.navdata_download_in_progress = true;
        self.navdata_download_feedback = None;
        self.navdata_download_cancel.store(false, Ordering::Relaxed);
        let cancel = Arc::clone(&self.navdata_download_cancel);
        self.log(
            format!("Downloading navigation data into {}...", target.display()),
            LogKind::Info,
        );
        thread::spawn(move || {
            let result = alas_exec::download::download_files(&specs, &target, 120.0, &cancel);
            let _ = sender.send(result);
        });
    }

    /// Ask a running navigation-data download to stop.
    ///
    /// This flips a distinct signal from the pipeline's `cancel_flag` (see
    /// its doc comment): the two downloads share no lifecycle. The download
    /// worker checks it between files, and between short waits on the file
    /// currently transferring, so this returns long before the whole
    /// transfer would otherwise finish.
    pub fn cancel_navdata_download(&mut self) {
        if !self.navdata_download_in_progress {
            return;
        }
        self.navdata_download_cancel.store(true, Ordering::Relaxed);
        self.log("Cancelling navigation-data download...", LogKind::Warn);
    }

    /// Drain a completed navdata transfer without blocking the UI.
    pub fn poll_navdata_download(&mut self) {
        let Some(receiver) = &self.navdata_download_rx else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                Err("navigation-data downloader stopped unexpectedly".to_owned())
            }
        };
        self.navdata_download_rx = None;
        self.navdata_download_in_progress = false;
        let (message, kind) = match result {
            Ok(alas_exec::download::DownloadOutcome::Completed(summary)) => (
                format!(
                    "Navigation data ready at {} (downloaded {}, skipped {}).",
                    summary.target_dir.display(),
                    summary.downloaded.len(),
                    summary.skipped.len()
                ),
                LogKind::Info,
            ),
            Ok(alas_exec::download::DownloadOutcome::Cancelled(summary)) => (
                format!(
                    "Navigation-data download cancelled (kept {} file(s) already installed before stopping).",
                    summary.downloaded.len()
                ),
                LogKind::Warn,
            ),
            Err(error) => (
                format!("Navigation-data download failed: {error}"),
                LogKind::Error,
            ),
        };
        self.navdata_download_feedback = Some((message.clone(), kind));
        self.log(message, kind);
    }

    /// Start the optional OpenVSP preview-runtime installer without blocking
    /// the egui frame.
    ///
    /// `force` doubles as reinstall/repair: the only affordance in the GUI is
    /// this one button, and the underlying script never deletes an existing
    /// runtime under `-Force`, it backs it up first, so passing `true`
    /// unconditionally is safe whether or not a runtime is already installed.
    pub fn start_openvsp_runtime_setup(&mut self) {
        if self.openvsp_runtime_setup.running {
            return;
        }
        let destination = crate::openvsp_runtime_setup::resolve_destination(
            self.tool_preferences.openvsp_dir.as_deref(),
        );
        self.log("Starting OpenVSP preview-runtime setup...", LogKind::Info);
        self.openvsp_runtime_setup.install(destination, true);
    }

    /// Ask a running OpenVSP preview-runtime setup to stop before it moves
    /// any staged files into the destination.
    pub fn cancel_openvsp_runtime_setup(&mut self) {
        if !self.openvsp_runtime_setup.running {
            return;
        }
        self.openvsp_runtime_setup.cancel();
        self.log("Cancelling OpenVSP preview-runtime setup...", LogKind::Warn);
    }

    /// Drain preview-runtime setup stage messages and log its terminal
    /// outcome, if any, without blocking the UI.
    pub fn poll_openvsp_runtime_setup(&mut self) {
        use crate::openvsp_runtime_setup::PreviewInstallEvent;
        match self.openvsp_runtime_setup.poll() {
            None => {}
            Some(PreviewInstallEvent::Completed) => {
                self.log("OpenVSP preview runtime installed.", LogKind::Info);
            }
            Some(PreviewInstallEvent::Cancelled) => {
                self.log(
                    "OpenVSP preview-runtime setup cancelled; nothing already installed was touched.",
                    LogKind::Warn,
                );
            }
            Some(PreviewInstallEvent::Failed(error)) => {
                self.log(
                    format!("OpenVSP preview-runtime setup failed: {error}"),
                    LogKind::Error,
                );
            }
        }
    }
}
