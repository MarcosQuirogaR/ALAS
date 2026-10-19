// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! User-saved aircraft ("My aircraft") and the GUI state of the file features.
//!
//! A saved aircraft is the complete current aircraft + mission configuration
//! (the same workspace document the File menu writes) with a small header that
//! gives it a user-chosen name. Files live in `my-aircraft/` under the per-user
//! data folder, one file per saved variant, so one airframe can be stored
//! several times for different missions. The files are plain JSON and can be
//! exported to, or imported from, any path.
//!
//! Loading a saved aircraft replaces the active case but keeps this machine's
//! tool locations, exactly as choosing a built-in preset does.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::file_dialog::FileDialogState;

/// Sub-folder of the data folder holding saved aircraft.
pub const MY_AIRCRAFT_DIR: &str = "my-aircraft";
/// Header key marking a file as a saved aircraft.
const HEADER_KEY: &str = "alas_my_aircraft";
/// Body key holding the workspace document.
const BODY_KEY: &str = "workspace";
/// Current file format version.
const FORMAT_VERSION: u32 = 1;

/// The header stored beside the configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AircraftHeader {
    /// File format version.
    pub version: u32,
    /// User-chosen name; the identity shown in the selector.
    pub name: String,
    /// The built-in preset this aircraft was derived from (may be empty).
    #[serde(default)]
    pub base_preset: String,
    /// Save time, seconds since the Unix epoch (UTC).
    #[serde(default)]
    pub saved_unix: u64,
}

/// One saved aircraft as listed in the selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserAircraft {
    /// Header of the file.
    pub header: AircraftHeader,
    /// Where it is stored.
    pub path: PathBuf,
}

/// GUI state of the file features (dialogs, saved aircraft, import notes).
#[derive(Debug, Default)]
pub struct UserFilesState {
    /// The single in-flight native file dialog.
    pub dialog: FileDialogState,
    /// Saved aircraft, refreshed when the folder changes.
    pub aircraft: Vec<UserAircraft>,
    /// Whether `aircraft` has been read from disk yet.
    pub aircraft_loaded: bool,
    /// Name of the saved aircraft currently loaded, shown in the selector.
    pub active_aircraft: Option<String>,
    /// Whether the "Save as my aircraft" window is open.
    pub save_open: bool,
    /// Name typed in the save window.
    pub save_name: String,
    /// Whether the manager window is open.
    pub manager_open: bool,
    /// Saved aircraft being renamed and the text typed so far.
    pub rename: Option<(PathBuf, String)>,
    /// Saved aircraft awaiting delete confirmation.
    pub confirm_delete: Option<PathBuf>,
    /// Outcome of the last save/rename/delete/import, shown in the windows.
    pub status: Option<String>,
    /// "Imported from <file> at <time>" note for the airport editor.
    pub airport_import_note: Option<String>,
}

/// Seconds since the Unix epoch, or 0 if the clock is before it.
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// `YYYY-MM-DD HH:MM UTC` for a Unix timestamp (proleptic Gregorian).
pub fn format_utc(unix: u64) -> String {
    let days = (unix / 86_400) as i64;
    let secs = unix % 86_400;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        secs / 3_600,
        (secs % 3_600) / 60
    )
}

/// The folder saved aircraft live in.
pub fn aircraft_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(MY_AIRCRAFT_DIR)
}

/// A file-system-safe stem for a user name: ASCII letters, digits, `-`, `_`.
fn sanitize_stem(name: &str) -> String {
    let mut stem = String::new();
    for character in name.trim().chars() {
        if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
            stem.push(character);
        } else if !stem.ends_with('-') {
            stem.push('-');
        }
    }
    let mut stem = stem.trim_matches('-').to_owned();
    stem.truncate(80);
    let reserved = ["con", "prn", "aux", "nul"];
    let lowered = stem.to_ascii_lowercase();
    let is_device = reserved.contains(&lowered.as_str())
        || ((lowered.starts_with("com") || lowered.starts_with("lpt"))
            && lowered.len() == 4
            && lowered.ends_with(|c: char| c.is_ascii_digit()));
    if stem.is_empty() || is_device {
        stem.push_str("aircraft");
    }
    stem
}

fn unique_path(dir: &Path, stem: &str) -> PathBuf {
    let mut candidate = dir.join(format!("{stem}.json"));
    let mut index = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{stem}-{index}.json"));
        index += 1;
    }
    candidate
}

fn read_header(path: &Path) -> Result<AircraftHeader, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let value: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let header = value
        .get(HEADER_KEY)
        .ok_or_else(|| "not a saved ALAS aircraft file".to_owned())?;
    serde_json::from_value(header.clone()).map_err(|error| error.to_string())
}

/// All saved aircraft in `data_dir`, sorted by name. Unreadable files are
/// skipped so one damaged file never hides the others.
pub fn list_aircraft(data_dir: &Path) -> Vec<UserAircraft> {
    let Ok(entries) = fs::read_dir(aircraft_dir(data_dir)) else {
        return Vec::new();
    };
    let mut found: Vec<UserAircraft> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .filter_map(|path| {
            read_header(&path)
                .ok()
                .map(|header| UserAircraft { header, path })
        })
        .collect();
    found.sort_by_key(|aircraft| aircraft.header.name.to_lowercase());
    found
}

fn write_aircraft(path: &Path, header: &AircraftHeader, workspace: &Value) -> Result<(), String> {
    let mut document = serde_json::Map::new();
    document.insert(
        HEADER_KEY.to_owned(),
        serde_json::to_value(header).map_err(|error| error.to_string())?,
    );
    document.insert(BODY_KEY.to_owned(), workspace.clone());
    let text = serde_json::to_string_pretty(&Value::Object(document))
        .map_err(|error| error.to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    crate::fs_atomic::write_atomic(path, text)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))
}

fn name_taken(data_dir: &Path, name: &str, except: Option<&Path>) -> bool {
    list_aircraft(data_dir).iter().any(|aircraft| {
        aircraft.header.name.eq_ignore_ascii_case(name.trim())
            && except.is_none_or(|path| path != aircraft.path)
    })
}

/// Store `workspace` under `name`. A saved aircraft with the same name
/// (case-insensitive) is replaced; any other name gets its own file, so
/// several variants of one airframe can coexist.
pub fn save_aircraft(
    data_dir: &Path,
    name: &str,
    base_preset: &str,
    workspace: &Value,
    saved_unix: u64,
) -> Result<PathBuf, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Enter a name for the aircraft.".to_owned());
    }
    let existing = list_aircraft(data_dir)
        .into_iter()
        .find(|aircraft| aircraft.header.name.eq_ignore_ascii_case(name));
    let path = match existing {
        Some(aircraft) => aircraft.path,
        None => unique_path(&aircraft_dir(data_dir), &sanitize_stem(name)),
    };
    let header = AircraftHeader {
        version: FORMAT_VERSION,
        name: name.to_owned(),
        base_preset: base_preset.to_owned(),
        saved_unix,
    };
    write_aircraft(&path, &header, workspace)?;
    Ok(path)
}

/// Read a saved aircraft: its header and workspace document.
pub fn read_aircraft(path: &Path) -> Result<(AircraftHeader, Value), String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let mut value: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let header = value
        .get(HEADER_KEY)
        .ok_or_else(|| "not a saved ALAS aircraft file".to_owned())?;
    let header: AircraftHeader =
        serde_json::from_value(header.clone()).map_err(|error| error.to_string())?;
    if header.version > FORMAT_VERSION {
        return Err(format!(
            "aircraft file version {} is newer than this build supports ({FORMAT_VERSION})",
            header.version
        ));
    }
    let workspace = value
        .get_mut(BODY_KEY)
        .map(Value::take)
        .ok_or_else(|| "the aircraft file has no configuration".to_owned())?;
    Ok((header, workspace))
}

/// Rename a saved aircraft. The new name must be non-empty and unused.
pub fn rename_aircraft(data_dir: &Path, path: &Path, new_name: &str) -> Result<PathBuf, String> {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        return Err("Enter a name for the aircraft.".to_owned());
    }
    if name_taken(data_dir, new_name, Some(path)) {
        return Err(format!("An aircraft named \"{new_name}\" already exists."));
    }
    let (mut header, workspace) = read_aircraft(path)?;
    header.name = new_name.to_owned();
    let target = unique_path(&aircraft_dir(data_dir), &sanitize_stem(new_name));
    write_aircraft(&target, &header, &workspace)?;
    fs::remove_file(path).map_err(|error| format!("cannot remove {}: {error}", path.display()))?;
    Ok(target)
}

/// Delete a saved aircraft file.
pub fn delete_aircraft(path: &Path) -> Result<(), String> {
    fs::remove_file(path).map_err(|error| format!("cannot delete {}: {error}", path.display()))
}

/// Copy a saved aircraft file to any path.
pub fn export_aircraft_file(source: &Path, destination: &Path) -> Result<(), String> {
    if let Some(parent) = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    fs::copy(source, destination)
        .map(|_| ())
        .map_err(|error| format!("cannot write {}: {error}", destination.display()))
}

/// Import an aircraft file from any path into the data folder. Accepts a
/// saved-aircraft file or a plain ALAS configuration/workspace file (named
/// after its file name). A name clash gets a numeric suffix.
pub fn import_aircraft(data_dir: &Path, source: &Path) -> Result<PathBuf, String> {
    let text = fs::read_to_string(source)
        .map_err(|error| format!("cannot read {}: {error}", source.display()))?;
    let value: Value = if source
        .extension()
        .is_some_and(|ext| ext == "yaml" || ext == "yml")
    {
        serde_yaml::from_str(&text).map_err(|error| error.to_string())?
    } else {
        serde_json::from_str(&text).map_err(|error| error.to_string())?
    };
    let (mut header, workspace) = if value.get(HEADER_KEY).is_some() {
        read_aircraft(source)?
    } else {
        let stem = source
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Imported aircraft".to_owned());
        let preset = value
            .get("preset")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        (
            AircraftHeader {
                version: FORMAT_VERSION,
                name: stem,
                base_preset: preset,
                saved_unix: now_unix(),
            },
            value,
        )
    };
    alas_config::AlasConfig::from_value(&workspace)
        .map_err(|error| format!("not a valid ALAS configuration: {error}"))?;
    let base = header.name.trim().to_owned();
    let mut name = base.clone();
    let mut index = 2;
    while name_taken(data_dir, &name, None) {
        name = format!("{base} ({index})");
        index += 1;
    }
    header.name = name.clone();
    let path = unique_path(&aircraft_dir(data_dir), &sanitize_stem(&name));
    write_aircraft(&path, &header, &workspace)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use serde_json::json;

    fn data_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("alas-user-aircraft-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn utc_formatting_matches_known_instants() {
        assert_eq!(format_utc(0), "1970-01-01 00:00 UTC");
        assert_eq!(format_utc(951_782_400), "2000-02-29 00:00 UTC");
        assert_eq!(format_utc(1_792_368_000 + 3_660), "2026-10-19 01:01 UTC");
    }

    #[test]
    fn stems_are_safe_and_never_device_names() {
        assert_eq!(sanitize_stem("A320 LEMD/LEBL"), "A320-LEMD-LEBL");
        assert_eq!(sanitize_stem("  "), "aircraft");
        assert_eq!(sanitize_stem("CON"), "CONaircraft");
        assert_eq!(sanitize_stem("../x"), "x");
    }

    #[test]
    fn variants_of_one_airframe_coexist_and_same_name_replaces() {
        let dir = data_dir("variants");
        let doc = json!({"preset": "A320-200"});
        let a = save_aircraft(&dir, "A320 short field", "A320-200", &doc, 1).unwrap();
        let b = save_aircraft(&dir, "A320 long range", "A320-200", &doc, 2).unwrap();
        assert_ne!(a, b);
        let again = save_aircraft(&dir, "a320 SHORT field", "A320-200", &doc, 3).unwrap();
        assert_eq!(again, a, "same name (any case) replaces the file");
        let listed = list_aircraft(&dir);
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].header.name, "A320 long range");
        assert_eq!(listed[1].header.saved_unix, 3);
        assert!(save_aircraft(&dir, "  ", "", &doc, 0).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_delete_export_and_import_round_trip() {
        let dir = data_dir("lifecycle");
        let state_doc = AppState::default().workspace_document();
        let path = save_aircraft(&dir, "First", "", &state_doc, 5).unwrap();
        let other = save_aircraft(&dir, "Second", "", &state_doc, 6).unwrap();

        let renamed = rename_aircraft(&dir, &path, "Renamed").unwrap();
        assert!(!path.exists() && renamed.exists());
        assert!(
            rename_aircraft(&dir, &renamed, "second").is_err(),
            "duplicate name"
        );
        assert!(rename_aircraft(&dir, &renamed, " ").is_err());

        let exported = dir.join("elsewhere").join("out.json");
        export_aircraft_file(&renamed, &exported).unwrap();
        let imported = import_aircraft(&dir, &exported).unwrap();
        let names: Vec<String> = list_aircraft(&dir)
            .into_iter()
            .map(|a| a.header.name)
            .collect();
        assert_eq!(names, vec!["Renamed", "Renamed (2)", "Second"]);
        assert!(imported.exists());

        delete_aircraft(&other).unwrap();
        assert_eq!(list_aircraft(&dir).len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_rejects_files_that_are_not_aircraft() {
        let dir = data_dir("reject");
        let bad = dir.join("bad.json");
        fs::write(&bad, "{ not json").unwrap();
        assert!(import_aircraft(&dir, &bad).is_err());
        let wrong = dir.join("wrong.json");
        fs::write(&wrong, r#"{"requirements": {"mtow_kg": "heavy"}}"#).unwrap();
        assert!(import_aircraft(&dir, &wrong).is_err());
        assert!(list_aircraft(&dir).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_saved_aircraft_restores_the_mission_configuration() {
        let dir = data_dir("restore");
        let mut state = AppState::default();
        state.load_preset("A320-200");
        let preset_values = state.config_values.clone();
        state.config_values["requirements"]["cruise_mach"] = Value::from(0.71);
        let saved = state.config_values.clone();
        let path = save_aircraft(
            &dir,
            "Short hop",
            "A320-200",
            &state.workspace_document(),
            1,
        )
        .unwrap();

        state.load_preset("A320-200");
        assert_eq!(state.config_values, preset_values);
        state.load_user_aircraft(&path);
        assert_eq!(
            state.config_values["requirements"]["cruise_mach"],
            saved["requirements"]["cruise_mach"]
        );
        assert_eq!(state.files.active_aircraft.as_deref(), Some("Short hop"));
        let _ = fs::remove_dir_all(&dir);
    }
}
