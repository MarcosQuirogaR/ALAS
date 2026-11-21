// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Per-airport storage of the custom airports: one `<ICAO>.json` file in
//! `<user data root>/library/my-airports/`, with a one-time migration from the
//! single `custom-airports.json` of earlier versions.
//!
//! A file holds the same JSON array the old store held (one element), so the
//! reader and the exporter of `alas_config::airport_io` apply unchanged.

use std::fs;
use std::path::{Path, PathBuf};

use alas_config::airport_io::CustomAirport;

use crate::config_edit::read_store_file;
use crate::config_edit::write_custom_airports;

/// The file of the airport with this ICAO code in `dir`.
pub fn airport_path(dir: &Path, icao: &str) -> PathBuf {
    let stem: String = icao
        .trim()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_uppercase();
    dir.join(format!("{stem}.json"))
}

/// Write (or replace) the file of one airport.
pub fn write_airport(dir: &Path, airport: &CustomAirport) -> Result<PathBuf, String> {
    let path = airport_path(dir, &airport.icao);
    write_custom_airports(&path, std::slice::from_ref(airport))?;
    Ok(path)
}

/// Delete the file of one airport; a missing file is not an error.
pub fn delete_airport(dir: &Path, icao: &str) -> Result<(), String> {
    let path = airport_path(dir, icao);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("cannot delete {}: {error}", path.display())),
    }
}

/// Every airport file of `dir`. A damaged file is reported in the second
/// element and skipped, so one bad file never hides the others.
pub fn read_airport_dir(dir: &Path) -> (Vec<CustomAirport>, Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return (Vec::new(), Vec::new());
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    let mut airports = Vec::new();
    let mut problems = Vec::new();
    for path in paths {
        match read_store_file(&path) {
            Ok(Some(mut found)) => airports.append(&mut found),
            Ok(None) => {}
            Err(error) => problems.push(format!("{}: {error}", path.display())),
        }
    }
    (airports, problems)
}

/// Move the airports of the legacy single-file store into `dir`, one file
/// each, then set the legacy file aside as `custom-airports.json.migrated`.
/// Returns how many airports were migrated; `Ok(0)` when there is no legacy
/// file. The legacy file is only set aside after every airport was written.
pub fn migrate_legacy(legacy: &Path, dir: &Path) -> Result<usize, String> {
    let Some(airports) = read_store_file(legacy)? else {
        return Ok(0);
    };
    for airport in &airports {
        write_airport(dir, airport)?;
    }
    let mut renamed = legacy.as_os_str().to_owned();
    renamed.push(".migrated");
    fs::rename(legacy, PathBuf::from(renamed))
        .map_err(|error| format!("cannot set aside {}: {error}", legacy.display()))?;
    Ok(airports.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::airport_io::{AirportProvenance, AirportProvenanceKind};

    fn airport(icao: &str) -> CustomAirport {
        CustomAirport {
            icao: icao.to_owned(),
            name: format!("Field {icao}"),
            latitude_deg: 40.0,
            longitude_deg: -3.0,
            isa_delta_c: 0.0,
            altitude_m: 600.0,
            runway_lengths_m: vec![3200.0],
            declared_toda_m: None,
            declared_lda_m: None,
            departure_surveys: Vec::new(),
            provenance: AirportProvenance {
                kind: AirportProvenanceKind::UserEntered,
                source: None,
                note: None,
            },
        }
    }

    #[test]
    fn legacy_store_migrates_to_one_file_per_airport() {
        let root =
            std::env::temp_dir().join(format!("alas-airport-store-mig-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let legacy = root.join("custom-airports.json");
        let dir = root.join("library").join("my-airports");
        write_custom_airports(&legacy, &[airport("LEXA"), airport("LEXB")]).unwrap();

        assert_eq!(migrate_legacy(&legacy, &dir).unwrap(), 2);
        assert!(!legacy.exists() && root.join("custom-airports.json.migrated").exists());
        assert!(dir.join("LEXA.json").exists() && dir.join("LEXB.json").exists());
        let (read, problems) = read_airport_dir(&dir);
        assert_eq!(read.len(), 2);
        assert!(problems.is_empty());
        assert_eq!(migrate_legacy(&legacy, &dir).unwrap(), 0, "runs once");

        delete_airport(&dir, "lexa").unwrap();
        assert_eq!(read_airport_dir(&dir).0.len(), 1);
        let _ = fs::remove_dir_all(&root);
    }
}
