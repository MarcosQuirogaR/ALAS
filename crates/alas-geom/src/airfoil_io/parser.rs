// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Selig DAT parsing and file I/O for custom airfoils.

use std::path::Path;

use crate::aircraft::airfoil::Airfoil;

use super::{validate_coordinates, AirfoilImportError, AirfoilProvenance, ImportedAirfoil};

/// Parse a custom Selig `.dat` string and return its validated geometry.
pub fn parse_dat(text: &str, source_label: &str) -> Result<Airfoil, AirfoilImportError> {
    Ok(parse_dat_with_provenance(text, source_label)?.airfoil)
}

/// Parse a custom Selig `.dat` string and retain its source label.
pub fn parse_dat_with_provenance(
    text: &str,
    source_label: &str,
) -> Result<ImportedAirfoil, AirfoilImportError> {
    let source = source_label.trim();
    let fallback_name = fallback_name(source);
    let mut meaningful = text.lines().enumerate().filter_map(|(index, raw)| {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            None
        } else {
            Some((index + 1, line))
        }
    });
    let Some((first_line, first)) = meaningful.next() else {
        return Err(AirfoilImportError::Empty);
    };

    let first_fields: Vec<&str> = first.split_whitespace().collect();
    let headerless = first_fields.len() == 2
        && first_fields
            .iter()
            .all(|field| field.parse::<f64>().is_ok());
    let (name, first_coordinate) = if headerless {
        (fallback_name, Some((first_line, first_fields)))
    } else {
        (first.to_owned(), None)
    };
    validate_name(&name)?;

    let mut coordinates = Vec::new();
    if let Some((line, fields)) = first_coordinate {
        coordinates.push(parse_coordinate(line, &fields)?);
    }
    for (line, raw) in meaningful {
        let fields: Vec<&str> = raw.split_whitespace().collect();
        coordinates.push(parse_coordinate(line, &fields)?);
    }
    validate_coordinates(&name, &coordinates)?;

    Ok(ImportedAirfoil {
        airfoil: Airfoil::from_coordinates(name, coordinates),
        provenance: AirfoilProvenance {
            source: if source.is_empty() {
                "user".to_owned()
            } else {
                source.to_owned()
            },
            format: "dat".to_owned(),
        },
    })
}

/// Read and validate a custom `.dat` file.
pub fn read_dat(path: impl AsRef<Path>) -> Result<Airfoil, AirfoilImportError> {
    Ok(import_dat(path)?.airfoil)
}

/// Read, validate, and annotate a custom `.dat` file.
pub fn import_dat(path: impl AsRef<Path>) -> Result<ImportedAirfoil, AirfoilImportError> {
    let path = path.as_ref();
    require_dat_extension(path)?;
    let display = path.display().to_string();
    let text = std::fs::read_to_string(path).map_err(|source| AirfoilImportError::Read {
        path: display.clone(),
        detail: source.to_string(),
    })?;
    parse_dat_with_provenance(&text, &display)
}

/// Write a validated airfoil in Selig `.dat` format.
pub fn write_dat(path: impl AsRef<Path>, airfoil: &Airfoil) -> Result<(), AirfoilImportError> {
    let path = path.as_ref();
    require_dat_extension(path)?;
    validate_coordinates(&airfoil.name, &airfoil.coordinates)?;
    let display = path.display().to_string();
    std::fs::write(path, airfoil.write_dat()).map_err(|source| AirfoilImportError::Write {
        path: display,
        detail: source.to_string(),
    })
}

fn fallback_name(source: &str) -> String {
    Path::new(source)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.trim().is_empty())
        .unwrap_or("custom-airfoil")
        .to_owned()
}

fn validate_name(name: &str) -> Result<(), AirfoilImportError> {
    if name.trim().is_empty() || name.chars().any(char::is_control) {
        Err(AirfoilImportError::InvalidName)
    } else {
        Ok(())
    }
}

fn parse_coordinate(line: usize, fields: &[&str]) -> Result<(f64, f64), AirfoilImportError> {
    if fields.len() != 2 {
        return Err(AirfoilImportError::InvalidCoordinate { line });
    }
    let x = fields[0]
        .parse::<f64>()
        .map_err(|_| AirfoilImportError::InvalidCoordinate { line })?;
    let y = fields[1]
        .parse::<f64>()
        .map_err(|_| AirfoilImportError::InvalidCoordinate { line })?;
    if !x.is_finite() || !y.is_finite() {
        return Err(AirfoilImportError::NonFinite { line });
    }
    Ok((x, y))
}

fn require_dat_extension(path: &Path) -> Result<(), AirfoilImportError> {
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("dat"))
    {
        Ok(())
    } else {
        Err(AirfoilImportError::UnsupportedExtension(
            path.display().to_string(),
        ))
    }
}
