// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Runtime registry for imported airfoils.

use std::sync::{OnceLock, RwLock};

use crate::aircraft::airfoil::Airfoil;

use super::{validate_coordinates, AirfoilImportError, AirfoilRecord, ImportedAirfoil};

/// Register one validated custom airfoil for geometry and preview lookup.
pub fn register(imported: ImportedAirfoil) -> Result<(), AirfoilImportError> {
    validate_coordinates(&imported.airfoil.name, &imported.airfoil.coordinates)?;
    let name = imported.airfoil.name.trim();
    if built_in(name) {
        return Err(AirfoilImportError::BuiltInName(name.to_owned()));
    }
    let mut registry = registry()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if registry
        .iter()
        .any(|existing| existing.imported.airfoil.name.eq_ignore_ascii_case(name))
    {
        return Err(AirfoilImportError::DuplicateName(name.to_owned()));
    }
    let name = Box::leak(name.to_owned().into_boxed_str());
    registry.push(RegisteredAirfoil { imported, name });
    Ok(())
}

/// Return one registered custom airfoil by case-insensitive name.
pub fn get(name: &str) -> Option<Airfoil> {
    let registry = registry()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    registry
        .iter()
        .find(|registered| {
            registered
                .imported
                .airfoil
                .name
                .eq_ignore_ascii_case(name.trim())
        })
        .map(|registered| registered.imported.airfoil.clone())
}

/// Return all registered custom airfoils with provenance.
pub fn registered() -> Vec<ImportedAirfoil> {
    registry()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .map(|registered| registered.imported.clone())
        .collect()
}

/// Return the names of all registered custom airfoils with a stable lifetime
/// suitable for the existing GUI/library listing API.
pub fn names() -> Vec<&'static str> {
    registry()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .map(|registered| registered.name)
        .collect()
}

/// Return serializable records for all registered custom airfoils.
pub fn records() -> Vec<AirfoilRecord> {
    registered().iter().map(AirfoilRecord::from).collect()
}

/// Replace the registry atomically from validated workspace records.
pub fn replace_records(records: Vec<AirfoilRecord>) -> Result<(), AirfoilImportError> {
    let mut replacement = Vec::with_capacity(records.len());
    for record in records {
        validate_coordinates(&record.name, &record.coordinates)?;
        if built_in(&record.name) {
            return Err(AirfoilImportError::BuiltInName(record.name));
        }
        if replacement.iter().any(|existing: &RegisteredAirfoil| {
            existing
                .imported
                .airfoil
                .name
                .eq_ignore_ascii_case(&record.name)
        }) {
            return Err(AirfoilImportError::DuplicateName(record.name));
        }
        let name = Box::leak(record.name.clone().into_boxed_str());
        replacement.push(RegisteredAirfoil {
            imported: ImportedAirfoil {
                airfoil: Airfoil::from_coordinates(record.name, record.coordinates),
                provenance: record.provenance,
            },
            name,
        });
    }
    *registry()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = replacement;
    Ok(())
}

struct RegisteredAirfoil {
    imported: ImportedAirfoil,
    name: &'static str,
}

fn registry() -> &'static RwLock<Vec<RegisteredAirfoil>> {
    static REGISTRY: OnceLock<RwLock<Vec<RegisteredAirfoil>>> = OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(Vec::new()))
}

fn built_in(name: &str) -> bool {
    selig_or_data(name) || Airfoil::from_name(name).is_some()
}

fn selig_or_data(name: &str) -> bool {
    crate::selig::get(name).is_some() || crate::airfoil_data::get(name).is_some()
}
