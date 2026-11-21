// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The unified file library: every user object is a typed, versioned JSON
//! file in the per-user data folder.
//!
//! One file holds one object: `{"alas_object": {kind, version, name,
//! saved_unix}, "body": ...}`. Each kind has its own sub-folder. Saved
//! aircraft keep their established format, now in `library/my-aircraft` (see
//! [`crate::user_files`]) and are listed through the same [`LibraryEntry`]
//! so the Files page treats every kind alike. Any file may be exported to or
//! imported from any path; the library only decides where new files go.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use crate::file_library_recent::{
    autosave, autosave_path, autosave_sandbox, note_recent, read_autosave, read_sandbox_autosave,
    recent, sandbox_autosave_path, RecentFile, RECENT_LIMIT,
};

/// Current format version of a library file.
pub const FORMAT_VERSION: u32 = 1;
/// Header key of a library file.
const HEADER_KEY: &str = "alas_object";
/// Body key of a library file.
const BODY_KEY: &str = "body";

/// What a library file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectKind {
    /// A complete aircraft and mission configuration.
    Aircraft,
    /// An engine installation (cycle, rating and nacelle data).
    Engine,
    /// A custom airfoil section.
    Airfoil,
    /// A mission definition: route and profile.
    MissionPlan,
    /// A cabin and payload layout.
    PayloadLayout,
    /// A custom airport.
    Airport,
    /// The settings of installed add-ons.
    AddonSettings,
    /// A snapshot of the results of a run.
    Results,
}

impl ObjectKind {
    /// Every kind, in the order the Files page lists them.
    pub const ALL: [Self; 8] = [
        Self::Aircraft,
        Self::Engine,
        Self::Airfoil,
        Self::MissionPlan,
        Self::PayloadLayout,
        Self::Airport,
        Self::AddonSettings,
        Self::Results,
    ];

    /// The stable id written into files.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Aircraft => "aircraft",
            Self::Engine => "engine",
            Self::Airfoil => "airfoil",
            Self::MissionPlan => "mission_plan",
            Self::PayloadLayout => "payload_layout",
            Self::Airport => "airport",
            Self::AddonSettings => "addon_settings",
            Self::Results => "results",
        }
    }

    /// The sub-folder of the data folder holding this kind.
    pub const fn folder(self) -> &'static str {
        match self {
            Self::Aircraft => crate::user_files::MY_AIRCRAFT_DIR,
            Self::Engine => "library/my-engines",
            Self::Airfoil => "airfoils",
            Self::MissionPlan => "mission-plans",
            Self::PayloadLayout => "payload-layouts",
            Self::Airport => "airports",
            Self::AddonSettings => "addon-settings",
            Self::Results => "results",
        }
    }

    /// The singular name used in messages (English; passed through the catalogue).
    pub const fn name(self) -> &'static str {
        match self {
            Self::Aircraft => "Aircraft",
            Self::Engine => "Engine",
            Self::Airfoil => "Airfoil",
            Self::MissionPlan => "Mission Plan",
            Self::PayloadLayout => "Payload Layout",
            Self::Airport => "Airport",
            Self::AddonSettings => "Add-on Settings",
            Self::Results => "Results",
        }
    }

    /// The list title (English; passed through the translation catalogue).
    pub const fn title(self) -> &'static str {
        match self {
            Self::Aircraft => "Aircraft",
            Self::Engine => "Engines",
            Self::Airfoil => "Airfoils",
            Self::MissionPlan => "Mission Plans",
            Self::PayloadLayout => "Payload Layouts",
            Self::Airport => "Airports",
            Self::AddonSettings => "Add-on Settings",
            Self::Results => "Results",
        }
    }

    /// The kind with this id.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.id() == id)
    }
}

/// The header of a library file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObjectHeader {
    /// The [`ObjectKind::id`].
    pub kind: String,
    /// File format version.
    pub version: u32,
    /// User-chosen name.
    pub name: String,
    /// Save time, seconds since the Unix epoch (UTC).
    #[serde(default)]
    pub saved_unix: u64,
}

/// One listed file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryEntry {
    /// What the file holds.
    pub kind: ObjectKind,
    /// The file's name as the user chose it.
    pub name: String,
    /// Save time, seconds since the Unix epoch (UTC).
    pub saved_unix: u64,
    /// Where it is stored.
    pub path: PathBuf,
}

/// The folder of one kind.
pub fn kind_dir(data_dir: &Path, kind: ObjectKind) -> PathBuf {
    data_dir.join(kind.folder())
}

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
    let lowered = stem.to_ascii_lowercase();
    let device = ["con", "prn", "aux", "nul"].contains(&lowered.as_str())
        || ((lowered.starts_with("com") || lowered.starts_with("lpt"))
            && lowered.len() == 4
            && lowered.ends_with(|c: char| c.is_ascii_digit()));
    if stem.is_empty() || device {
        stem.push_str("object");
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

pub(crate) fn read_document(path: &Path) -> Result<(ObjectHeader, Value), String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let mut value: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let header = value
        .get(HEADER_KEY)
        .ok_or_else(|| "not an ALAS library file".to_owned())?;
    let header: ObjectHeader =
        serde_json::from_value(header.clone()).map_err(|error| error.to_string())?;
    if header.version > FORMAT_VERSION {
        return Err(format!(
            "file version {} is newer than this build supports ({FORMAT_VERSION})",
            header.version
        ));
    }
    let body = value
        .get_mut(BODY_KEY)
        .map(Value::take)
        .ok_or_else(|| "the file has no content".to_owned())?;
    Ok((header, body))
}

pub(crate) fn write_document(
    path: &Path,
    header: &ObjectHeader,
    body: &Value,
) -> Result<(), String> {
    let mut document = serde_json::Map::new();
    document.insert(
        HEADER_KEY.to_owned(),
        serde_json::to_value(header).map_err(|error| error.to_string())?,
    );
    document.insert(BODY_KEY.to_owned(), body.clone());
    let text = serde_json::to_string_pretty(&Value::Object(document))
        .map_err(|error| error.to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    crate::fs_atomic::write_atomic(path, text)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))
}

/// The files of one kind, sorted by name. Unreadable files are skipped so
/// one damaged file never hides the others.
pub fn list(data_dir: &Path, kind: ObjectKind) -> Vec<LibraryEntry> {
    if kind == ObjectKind::Aircraft {
        return crate::user_files::list_aircraft(data_dir)
            .into_iter()
            .map(|aircraft| LibraryEntry {
                kind,
                name: aircraft.header.name,
                saved_unix: aircraft.header.saved_unix,
                path: aircraft.path,
            })
            .collect();
    }
    let mut dirs = vec![kind_dir(data_dir, kind)];
    if kind == ObjectKind::Engine {
        // Engines saved before the library folder existed.
        dirs.push(data_dir.join("engines"));
    }
    let mut found: Vec<LibraryEntry> = dirs
        .into_iter()
        .filter_map(|dir| fs::read_dir(dir).ok())
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .filter_map(|path| {
            let (header, _) = read_document(&path).ok()?;
            (header.kind == kind.id()).then_some(LibraryEntry {
                kind,
                name: header.name,
                saved_unix: header.saved_unix,
                path,
            })
        })
        .collect();
    found.sort_by_key(|entry| entry.name.to_lowercase());
    found
}

/// Store `body` under `name`; a file of the same kind and name
/// (case-insensitive) is replaced. Aircraft are saved through
/// [`crate::user_files::save_aircraft`].
pub fn save(
    data_dir: &Path,
    kind: ObjectKind,
    name: &str,
    body: &Value,
    saved_unix: u64,
) -> Result<PathBuf, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Enter a name.".to_owned());
    }
    if kind == ObjectKind::Aircraft {
        return crate::user_files::save_aircraft(data_dir, name, "", body, saved_unix);
    }
    let existing = list(data_dir, kind)
        .into_iter()
        .find(|entry| entry.name.eq_ignore_ascii_case(name));
    let path = match existing {
        Some(entry) => entry.path,
        None => unique_path(&kind_dir(data_dir, kind), &sanitize_stem(name)),
    };
    let header = ObjectHeader {
        kind: kind.id().to_owned(),
        version: FORMAT_VERSION,
        name: name.to_owned(),
        saved_unix,
    };
    write_document(&path, &header, body)?;
    Ok(path)
}

/// Read one file of `kind`.
pub fn read(kind: ObjectKind, path: &Path) -> Result<(String, Value), String> {
    if kind == ObjectKind::Aircraft {
        let (header, workspace) = crate::user_files::read_aircraft(path)?;
        return Ok((header.name, workspace));
    }
    let (header, body) = read_document(path)?;
    if header.kind != kind.id() {
        return Err(format!(
            "this file holds {} data, not {}",
            header.kind,
            kind.id()
        ));
    }
    Ok((header.name, body))
}

/// Rename a file. The new name must be non-empty and unused within its kind.
pub fn rename(
    data_dir: &Path,
    kind: ObjectKind,
    path: &Path,
    new_name: &str,
) -> Result<PathBuf, String> {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        return Err("Enter a name.".to_owned());
    }
    if kind == ObjectKind::Aircraft {
        return crate::user_files::rename_aircraft(data_dir, path, new_name);
    }
    if list(data_dir, kind)
        .iter()
        .any(|entry| entry.name.eq_ignore_ascii_case(new_name) && entry.path != path)
    {
        return Err(format!("\"{new_name}\" already exists."));
    }
    let (_, body) = read(kind, path)?;
    let target = unique_path(&kind_dir(data_dir, kind), &sanitize_stem(new_name));
    let header = ObjectHeader {
        kind: kind.id().to_owned(),
        version: FORMAT_VERSION,
        name: new_name.to_owned(),
        saved_unix: crate::user_files::now_unix(),
    };
    write_document(&target, &header, &body)?;
    fs::remove_file(path).map_err(|error| format!("cannot remove {}: {error}", path.display()))?;
    Ok(target)
}

/// A first free "<name> Copy", "<name> Copy 2", ... for `kind`.
pub fn copy_name(data_dir: &Path, kind: ObjectKind, name: &str) -> String {
    let taken: Vec<String> = list(data_dir, kind)
        .into_iter()
        .map(|entry| entry.name.to_lowercase())
        .collect();
    let mut candidate = format!("{name} Copy");
    let mut index = 2;
    while taken.contains(&candidate.to_lowercase()) {
        candidate = format!("{name} Copy {index}");
        index += 1;
    }
    candidate
}

/// Duplicate a file under a free copy name.
pub fn duplicate(data_dir: &Path, kind: ObjectKind, path: &Path) -> Result<PathBuf, String> {
    let (name, body) = read(kind, path)?;
    let copy = copy_name(data_dir, kind, &name);
    save(data_dir, kind, &copy, &body, crate::user_files::now_unix())
}

/// Delete a file.
pub fn delete(path: &Path) -> Result<(), String> {
    fs::remove_file(path).map_err(|error| format!("cannot delete {}: {error}", path.display()))
}

/// Copy a library file to any path.
pub fn export(source: &Path, destination: &Path) -> Result<(), String> {
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

/// Import a file from any path into the library. The file's own header names
/// its kind (an aircraft file or an ALAS configuration is an aircraft); a name
/// clash gets a numeric suffix. Returns the kind and the new path.
pub fn import(data_dir: &Path, source: &Path) -> Result<(ObjectKind, PathBuf), String> {
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
    if let Some(header) = value.get(HEADER_KEY) {
        let header: ObjectHeader =
            serde_json::from_value(header.clone()).map_err(|error| error.to_string())?;
        let kind = ObjectKind::from_id(&header.kind)
            .ok_or_else(|| format!("unknown object kind \"{}\"", header.kind))?;
        let (name, body) = read(kind, source)?;
        let name = unique_name(data_dir, kind, &name);
        let path = save(data_dir, kind, &name, &body, header.saved_unix)?;
        return Ok((kind, path));
    }
    let path = crate::user_files::import_aircraft(data_dir, source)?;
    Ok((ObjectKind::Aircraft, path))
}

fn unique_name(data_dir: &Path, kind: ObjectKind, name: &str) -> String {
    let taken: Vec<String> = list(data_dir, kind)
        .into_iter()
        .map(|entry| entry.name.to_lowercase())
        .collect();
    if !taken.contains(&name.to_lowercase()) {
        return name.to_owned();
    }
    let mut index = 2;
    loop {
        let candidate = format!("{name} {index}");
        if !taken.contains(&candidate.to_lowercase()) {
            return candidate;
        }
        index += 1;
    }
}

/// The kind of library file at `path`, when it is one (an aircraft file counts).
pub fn file_kind(path: &Path) -> Option<ObjectKind> {
    let text = fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    if value.get("alas_my_aircraft").is_some() {
        return Some(ObjectKind::Aircraft);
    }
    let header: ObjectHeader = serde_json::from_value(value.get(HEADER_KEY)?.clone()).ok()?;
    ObjectKind::from_id(&header.kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn data_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "alas-file-library-{tag}-{}-{}",
            std::process::id(),
            crate::user_files::now_unix()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn every_kind_saves_lists_reads_renames_duplicates_and_deletes() {
        let dir = data_dir("roundtrip");
        for kind in ObjectKind::ALL {
            let body = json!({"kind": kind.id(), "value": 1.5});
            let path = save(&dir, kind, "Case A", &body, 100).unwrap();
            let listed = list(&dir, kind);
            assert_eq!(listed.len(), 1, "{kind:?}");
            assert_eq!(listed[0].name, "Case A");
            let (name, read_back) = read(kind, &path).unwrap();
            assert_eq!(name, "Case A");
            if kind != ObjectKind::Aircraft {
                assert_eq!(read_back, body);
            }
            let renamed = rename(&dir, kind, &path, "Case B").unwrap();
            assert_eq!(list(&dir, kind)[0].name, "Case B");
            let copy = duplicate(&dir, kind, &renamed).unwrap();
            assert_eq!(list(&dir, kind).len(), 2);
            assert_ne!(copy, renamed);
            delete(&copy).unwrap();
            delete(&renamed).unwrap();
            assert!(list(&dir, kind).is_empty());
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn saving_the_same_name_replaces_and_an_unknown_kind_or_newer_version_is_refused() {
        let dir = data_dir("replace");
        let first = save(&dir, ObjectKind::Engine, "My Engine", &json!({"a": 1}), 1).unwrap();
        let second = save(&dir, ObjectKind::Engine, "my engine", &json!({"a": 2}), 2).unwrap();
        assert_eq!(first, second);
        assert_eq!(list(&dir, ObjectKind::Engine).len(), 1);
        assert!(read(ObjectKind::Airfoil, &first).is_err());
        let newer = dir.join("newer.json");
        fs::write(
            &newer,
            r#"{"alas_object":{"kind":"engine","version":99,"name":"x"},"body":{}}"#,
        )
        .unwrap();
        assert!(read(ObjectKind::Engine, &newer)
            .unwrap_err()
            .contains("newer"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_then_import_restores_the_object_from_any_path() {
        let dir = data_dir("export");
        let other = data_dir("export-other");
        let path = save(&dir, ObjectKind::MissionPlan, "Plan", &json!({"leg": 3}), 5).unwrap();
        let elsewhere = other.join("nested").join("plan.json");
        export(&path, &elsewhere).unwrap();
        let (kind, imported) = import(&other, &elsewhere).unwrap();
        assert_eq!(kind, ObjectKind::MissionPlan);
        let (name, body) = read(kind, &imported).unwrap();
        assert_eq!(name, "Plan");
        assert_eq!(body, json!({"leg": 3}));
        // A second import of the same file does not overwrite the first.
        let (_, again) = import(&other, &elsewhere).unwrap();
        assert_ne!(again, imported);
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&other);
    }

    #[test]
    fn recent_files_are_newest_first_unique_bounded_and_skip_missing_files() {
        let dir = data_dir("recent");
        let mut paths = Vec::new();
        for index in 0..(RECENT_LIMIT + 3) {
            let path = save(
                &dir,
                ObjectKind::Airport,
                &format!("Airport {index}"),
                &json!({}),
                index as u64,
            )
            .unwrap();
            note_recent(&dir, ObjectKind::Airport, &path, index as u64).unwrap();
            paths.push(path);
        }
        let recent_files = recent(&dir);
        assert_eq!(recent_files.len(), RECENT_LIMIT);
        assert_eq!(recent_files[0].path, *paths.last().unwrap());
        delete(paths.last().unwrap()).unwrap();
        assert_eq!(recent(&dir).len(), RECENT_LIMIT - 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_autosave_slot_round_trips_and_is_overwritten() {
        let dir = data_dir("autosave");
        assert!(read_autosave(&dir, ObjectKind::PayloadLayout).is_none());
        autosave(&dir, ObjectKind::PayloadLayout, &json!({"n": 1}), 10).unwrap();
        autosave(&dir, ObjectKind::PayloadLayout, &json!({"n": 2}), 20).unwrap();
        let (when, body) = read_autosave(&dir, ObjectKind::PayloadLayout).unwrap();
        assert_eq!((when, body), (20, json!({"n": 2})));
        let _ = fs::remove_dir_all(&dir);
    }
}
