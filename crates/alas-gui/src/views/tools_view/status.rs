// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Resolved availability rows for the External Tools page.

use super::layout::{chip_label, chip_width, TWO_COLUMN_WIDTH};
use super::*;

use alas_exec::{ExecutableDiscovery, RunEnvironment};

#[derive(Clone, PartialEq)]
struct ExternalToolStatus {
    fingerprint: String,
    environment: RunEnvironment,
    nastran: ExecutableDiscovery,
    nastran95: (Tone, String),
    patran: ExecutableDiscovery,
    openvsp: ExecutableDiscovery,
    vspaero: ExecutableDiscovery,
    avl: ExecutableDiscovery,
    flowunsteady: ExecutableDiscovery,
    parafoam: Option<std::path::PathBuf>,
    /// The navigation-data and saved-routes folders a run reads (see
    /// [`resolve_folder`]).
    navdata: Option<(std::path::PathBuf, bool)>,
    routes: Option<(std::path::PathBuf, bool)>,
}

/// Background tool-discovery state shared between the UI thread and the scan
/// thread. Discovery walks conventional install roots and reads manifests, so
/// it never runs on the paint loop: a scan starts only when the fingerprint
/// (configured paths, install state) changes, and the last result is shown
/// meanwhile.
#[derive(Default)]
struct StatusSlot {
    snapshot: Option<ExternalToolStatus>,
    /// Fingerprint of the scan in flight, if any, and when it started.
    scanning: Option<(String, std::time::Instant)>,
}

/// Everything a scan needs, owned so it can cross to the worker thread.
struct ScanInput {
    locator: alas_exec::ToolLocator,
    config: alas_config::AlasConfig,
    openvsp_dir: String,
    avl_exe: String,
    flowunsteady_exe: String,
    native_project_dir: Option<String>,
    native_bin_dir: Option<String>,
}

/// The page's status overview: one row per tool with a status chip.
pub(super) fn overview(state: &AppState, ui: &mut Ui) {
    let Some(config) = state.typed_config() else {
        return;
    };
    let fingerprint = status_fingerprint(state, &config);
    let slot_id = egui::Id::new("alas_external_tools_status_slot");
    let slot: std::sync::Arc<std::sync::Mutex<StatusSlot>> = ui.ctx().data_mut(|data| {
        data.get_temp_mut_or_insert_with(slot_id, || {
            std::sync::Arc::new(std::sync::Mutex::new(StatusSlot::default()))
        })
        .clone()
    });
    let snapshot = {
        let Ok(mut guard) = slot.lock() else {
            return;
        };
        let current = guard
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.fingerprint == fingerprint);
        if !current && guard.scanning.is_none() {
            guard.scanning = Some((fingerprint.clone(), std::time::Instant::now()));
            let input = ScanInput {
                locator: state.tool_locator.clone(),
                config: config.clone(),
                openvsp_dir: state
                    .tool_preferences
                    .openvsp_dir
                    .clone()
                    .unwrap_or_default(),
                avl_exe: state.tool_preferences.avl_exe.clone().unwrap_or_default(),
                flowunsteady_exe: state
                    .tool_preferences
                    .flowunsteady_exe
                    .clone()
                    .unwrap_or_default(),
                native_project_dir: state.cfd.openfoam_preferences.native_project_dir.clone(),
                native_bin_dir: state.cfd.openfoam_preferences.native_bin_dir.clone(),
            };
            let worker_slot = slot.clone();
            let ctx = ui.ctx().clone();
            let worker_fingerprint = fingerprint.clone();
            std::thread::spawn(move || {
                let snapshot = build_status_snapshot(&input, worker_fingerprint);
                if let Ok(mut guard) = worker_slot.lock() {
                    guard.snapshot = Some(snapshot);
                    guard.scanning = None;
                }
                ctx.request_repaint();
            });
        }
        guard.snapshot.clone()
    };
    // A check in progress counts its seconds; its result replaces the line.
    let scanning = slot
        .lock()
        .ok()
        .and_then(|guard| guard.scanning.as_ref().map(|(_, since)| *since));
    let checking =
        crate::theme::in_progress(ui.ctx(), &tr("Checking installed tools..."), scanning);
    let Some(snapshot) = snapshot else {
        ui.label(RichText::new(checking).weak());
        return;
    };
    if scanning.is_some() {
        ui.label(RichText::new(checking).weak().small());
    }
    let environment = &snapshot.environment;
    let mut entries = vec![
        Entry::new(
            "mses",
            "MSES",
            environment.mses_dir.as_deref().map_or_else(
                || (Tone::Missing, tr("not found")),
                |path| (Tone::Ready, path.display().to_string()),
            ),
        ),
        Entry::new(
            "navdata",
            "Local navigation data",
            folder_status(snapshot.navdata.as_ref(), |path| {
                tr_fields("not downloaded: {path}", &[("path", path)])
            }),
        ),
        Entry::new(
            "routes",
            "Saved routes",
            folder_status(snapshot.routes.as_ref(), |path| {
                tr_fields("folder missing: {path}", &[("path", path)])
            }),
        ),
        Entry::new(
            "simbrief",
            "SimBrief flight plan",
            if config.mission.simbrief_username.trim().is_empty() {
                (Tone::Neutral, tr("not configured"))
            } else {
                (Tone::Ready, config.mission.simbrief_username.clone())
            },
        )
        .with_chip_text(if config.mission.simbrief_username.trim().is_empty() {
            "Not set"
        } else {
            "Configured"
        }),
        Entry::new(
            "nastran",
            "NASTRAN",
            discovery_status(&snapshot.nastran, describe_executable),
        ),
        Entry::new("nastran95", "NASTRAN-95", snapshot.nastran95.clone()),
        Entry::new(
            "nastran_solver",
            "MSC solver override",
            (
                if environment.nastran_solver.is_some() {
                    Tone::Ready
                } else if config.structures.nastran_solver_path.trim().is_empty() {
                    Tone::Neutral
                } else {
                    Tone::Warn
                },
                describe_nastran_solver(
                    &config.structures.nastran_solver_path,
                    environment.nastran_solver.as_deref(),
                ),
            ),
        ),
        Entry::new(
            "patran",
            "Patran",
            discovery_status(&snapshot.patran, describe_executable),
        ),
        Entry::new(
            "openvsp",
            "OpenVSP script runner",
            discovery_status(&snapshot.openvsp, describe_executable),
        ),
        Entry::new(
            "vspaero",
            "VSPAERO solver",
            discovery_status(&snapshot.vspaero, describe_executable),
        ),
        Entry::new(
            "avl",
            "Athena AVL",
            discovery_status(&snapshot.avl, describe_executable),
        ),
        Entry::new(
            "flowunsteady",
            "FLOWUnsteady / Julia",
            discovery_status(&snapshot.flowunsteady, describe_flowunsteady),
        ),
    ];
    // Say whether a ready tool was found by discovery or is the configured one.
    for entry in &mut entries {
        let configured = match entry.key {
            "mses" => Some(config.mses.mses_dir.as_str()),
            "nastran" => Some(config.structures.nastran_exe_path.as_str()),
            "nastran_solver" => Some(config.structures.nastran_solver_path.as_str()),
            "patran" => Some(config.structures.patran_exe_path.as_str()),
            _ => None,
        };
        if let (Some(value), Tone::Ready) = (configured, entry.chip.tone) {
            let text = if value.trim().is_empty() {
                "Ready (found)"
            } else {
                "Ready (set)"
            };
            entry.chip.text = tr(text);
        }
    }
    entries.extend(cfd::status_entries(state, snapshot.parafoam.as_deref()));
    ui.ctx()
        .data_mut(|data| data.insert_temp(entries_id(), entries.clone()));
    overview_rows(ui, &entries);
}

fn entries_id() -> egui::Id {
    egui::Id::new("alas_external_tools_entries")
}

/// The status chip of tool `key` from the latest scan, if there is one.
pub(super) fn chip_of(ctx: &egui::Context, key: &str) -> Option<Chip> {
    ctx.data(|data| data.get_temp::<Vec<Entry>>(entries_id()))
        .and_then(|entries| entries.into_iter().find(|entry| entry.key == key))
        .map(|entry| entry.chip)
}

/// The auto-discovered location of tool `key` from the latest scan, when it
/// is ready and the detail is an existing path.
pub(super) fn found_path(ctx: &egui::Context, key: &str) -> Option<String> {
    let entries = ctx.data(|data| data.get_temp::<Vec<Entry>>(entries_id()))?;
    let entry = entries.into_iter().find(|entry| entry.key == key)?;
    let path = entry
        .detail
        .split(" (automatic")
        .next()
        .unwrap_or_default()
        .to_owned();
    (entry.chip.tone == Tone::Ready && std::path::Path::new(&path).exists())
        .then(|| super::field_row::display_path(&path))
}

/// How a tool stands, which sets its chip's colour.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Tone {
    Ready,
    Missing,
    Warn,
    Neutral,
}

/// A status chip: short text and its tone.
#[derive(Clone, PartialEq, Debug)]
pub(super) struct Chip {
    pub text: String,
    pub tone: Tone,
}

/// One row of the overview: a tool, its chip and the path or detail.
#[derive(Clone, PartialEq, Debug)]
pub(super) struct Entry {
    pub key: &'static str,
    pub name: &'static str,
    pub chip: Chip,
    pub detail: String,
}

impl Entry {
    pub(super) fn new(key: &'static str, name: &'static str, status: (Tone, String)) -> Self {
        let text = match status.0 {
            Tone::Ready => "Ready",
            Tone::Missing => "Not found",
            Tone::Warn => "Check path",
            Tone::Neutral => "Not set",
        };
        Self {
            key,
            name,
            chip: Chip {
                text: tr(text),
                tone: status.0,
            },
            detail: status.1,
        }
    }

    /// The entry with its chip text replaced.
    pub(super) fn with_chip_text(mut self, text: &str) -> Self {
        self.chip.text = tr(text);
        self
    }
}

fn discovery_status(
    discovery: &ExecutableDiscovery,
    describe: fn(&ExecutableDiscovery) -> String,
) -> (Tone, String) {
    let tone = match discovery {
        ExecutableDiscovery::Ready(_) => Tone::Ready,
        ExecutableDiscovery::Absent => Tone::Missing,
        ExecutableDiscovery::Incomplete { .. } => Tone::Warn,
    };
    (tone, describe(discovery))
}

/// A configured data folder resolved as a run resolves it (against the
/// application folder, then the user-data folder) and whether it exists;
/// `None` when no folder is configured. Runs on the scan thread.
fn resolve_folder(
    locator: &alas_exec::ToolLocator,
    configured: &str,
) -> Option<(std::path::PathBuf, bool)> {
    let configured = configured.trim();
    (!configured.is_empty()).then(|| {
        let folder = locator.resolve_data_path(std::path::Path::new(configured));
        let exists = folder.is_dir();
        (folder, exists)
    })
}

/// The status of a folder from [`resolve_folder`]: the absolute folder used
/// while it exists, else a warning worded by `missing`. Its text is worded
/// here, on the thread that shows it, in the language of the page.
fn folder_status(
    folder: Option<&(std::path::PathBuf, bool)>,
    missing: impl FnOnce(String) -> String,
) -> (Tone, String) {
    let Some((path, exists)) = folder else {
        return (Tone::Neutral, tr("not configured"));
    };
    let shown = field_row::display_path(&path.display().to_string());
    if *exists {
        (Tone::Ready, shown)
    } else {
        (Tone::Warn, missing(shown))
    }
}

/// Overview rows: name, chip and detail of each entry, as cells of the form
/// grid with the page's column count (two on a wide card, like the cards
/// below). Two entries of a row stand the column gutter apart and level; every
/// text keeps to one line, elided, with the full text on hover. The name
/// column is as wide as the widest name of the whole list and the chips share
/// one width, so every cell has the same inner grid.
fn overview_rows(ui: &mut Ui, entries: &[Entry]) {
    let columns = if ui.available_width() >= TWO_COLUMN_WIDTH - 40.0 {
        2
    } else {
        1
    };
    let font = egui::TextStyle::Body.resolve(ui.style());
    let widest = entries
        .iter()
        .map(|entry| {
            ui.painter()
                .layout_no_wrap(tr(entry.name), font.clone(), ui.visuals().text_color())
                .size()
                .x
        })
        .fold(0.0_f32, f32::max);
    // Bold names are a little wider than the regular face measured above.
    let name_width = (widest * 1.08 + 6.0).ceil();
    let mut grid = crate::views::form::CellGrid::with_columns(ui, columns, "tools_overview");
    for entry in entries {
        grid.cell(ui, |ui, _| overview_row(ui, entry, name_width));
    }
    grid.finish(ui);
}

fn overview_row(ui: &mut Ui, entry: &Entry, name_width: f32) {
    let height = crate::theme::control_height(ui).min(24.0);
    let name_width = name_width.min(ui.available_width() * 0.45);
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(name_width, height),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                // The minimum size keeps every row's name column one width.
                ui.set_min_size(egui::vec2(name_width, height));
                ui.set_max_width(name_width);
                ui.add(egui::Label::new(RichText::new(tr(entry.name)).strong()).truncate());
            },
        );
        ui.allocate_ui_with_layout(
            egui::vec2(chip_width(ui), height),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_size(egui::vec2(chip_width(ui), height));
                chip_label(ui, &entry.chip)
            },
        );
        field_row::elided_weak_label(ui, entry.detail.as_str());
    });
}

fn status_fingerprint(state: &AppState, config: &alas_config::AlasConfig) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}:{}",
        config.mses.mses_dir,
        config.structures.nastran_exe_path,
        config.structures.nastran_solver_path,
        config.structures.nastran95_dir_path,
        config.structures.nastran95_runtime_path,
        config.structures.nastran95_rf_stage_path,
        config.structures.nastran95_open_core_words,
        config.structures.patran_exe_path,
        config.mission.navdata_dir,
        config.mission.routes_dir,
        // A finished navigation-data download fills its folder.
        state.navdata_download_in_progress,
        state.tool_preferences.openvsp_dir.as_deref().unwrap_or(""),
        state.tool_preferences.avl_exe.as_deref().unwrap_or(""),
        state
            .cfd
            .openfoam_preferences
            .native_project_dir
            .as_deref()
            .unwrap_or(""),
        state
            .cfd
            .openfoam_preferences
            .native_bin_dir
            .as_deref()
            .unwrap_or(""),
        state
            .tool_preferences
            .flowunsteady_exe
            .as_deref()
            .unwrap_or(""),
        // A finished (un)install changes what discovery can find: this is the
        // only trigger for a re-scan besides an edited path.
        state.tool_installer.running,
        state.tool_installer.reports.len(),
    )
}

fn build_status_snapshot(input: &ScanInput, fingerprint: String) -> ExternalToolStatus {
    let config = &input.config;
    let locator = &input.locator;
    let mses_dir = std::path::Path::new(&config.mses.mses_dir);
    let nastran = std::path::Path::new(&config.structures.nastran_exe_path);
    let patran = std::path::Path::new(&config.structures.patran_exe_path);
    let openvsp = std::path::Path::new(&input.openvsp_dir);
    let avl = std::path::Path::new(&input.avl_exe);
    let flowunsteady = std::path::Path::new(&input.flowunsteady_exe);
    // `resolve_environment` performs the same discovery calls that the
    // status card needs.  Calling it and then discovering every executable a
    // second time made the page scan all conventional installation roots
    // twice per refresh.  Resolve each candidate once and derive both the
    // displayed discoveries and the run environment from those values.
    let nastran_status = locator.discover_nastran(nastran);
    let nastran_solver_status = locator.discover_nastran_solver(nastran);
    let patran_status = locator.discover_patran(patran);
    let openvsp_status = locator.discover_openvsp(openvsp);
    let vspaero_status = locator.discover_vspaero(openvsp);
    let avl_status = locator.discover_avl(avl);
    let flowunsteady_status = locator.discover_flowunsteady(flowunsteady);
    let ready_path = |discovery: &ExecutableDiscovery| match discovery {
        ExecutableDiscovery::Ready(path) => Some(path.clone()),
        ExecutableDiscovery::Absent | ExecutableDiscovery::Incomplete { .. } => None,
    };
    let environment = RunEnvironment {
        mses_dir: locator.resolve_mses_dir(mses_dir),
        nastran_exe: ready_path(&nastran_status),
        nastran_solver: ready_path(&nastran_solver_status),
        patran_exe: ready_path(&patran_status),
        openvsp_exe: ready_path(&openvsp_status),
        vspaero_exe: ready_path(&vspaero_status),
        avl_exe: ready_path(&avl_status),
        flowunsteady_exe: ready_path(&flowunsteady_status),
    };
    ExternalToolStatus {
        fingerprint,
        environment,
        nastran: nastran_status,
        nastran95: describe_nastran95(&config.structures),
        patran: patran_status,
        openvsp: openvsp_status,
        vspaero: vspaero_status,
        avl: avl_status,
        flowunsteady: flowunsteady_status,
        parafoam: cfd::detect_parafoam(
            input.native_project_dir.as_deref(),
            input.native_bin_dir.as_deref(),
        ),
        navdata: resolve_folder(locator, &config.mission.navdata_dir),
        routes: resolve_folder(locator, &config.mission.routes_dir),
    }
}

#[cfg(test)]
mod tests {
    use super::status_fingerprint;
    use crate::state::AppState;

    #[test]
    fn status_cache_fingerprint_tracks_configured_tool_locations() {
        let mut state = AppState::default();
        let config = state.typed_config().expect("default config");
        let initial = status_fingerprint(&state, &config);

        state.tool_preferences.openvsp_dir = Some("C:\\OpenVSP-test".to_owned());
        assert_ne!(
            status_fingerprint(&state, &config),
            initial,
            "changing an expensive discovery root must invalidate the cached status"
        );
    }

    #[test]
    fn status_cache_fingerprint_tracks_nastran95_paths() {
        let mut state = AppState::default();
        let initial = status_fingerprint(&state, &state.typed_config().expect("default config"));
        state.config_values["structures"]["nastran95_dir_path"] =
            serde_json::Value::String("C:/nastran95-test".to_owned());
        let changed = status_fingerprint(&state, &state.typed_config().expect("updated config"));
        assert_ne!(initial, changed);
    }

    #[test]
    fn status_cache_fingerprint_tracks_the_configured_flowunsteady_location() {
        let mut state = AppState::default();
        let config = state.typed_config().expect("default config");
        let initial = status_fingerprint(&state, &config);

        state.tool_preferences.flowunsteady_exe = Some("C:\\FLOWUnsteady-test.exe".to_owned());
        assert_ne!(
            status_fingerprint(&state, &config),
            initial,
            "a configured FLOWUnsteady path must invalidate the cached status the same way openvsp_dir already does"
        );
    }
}
