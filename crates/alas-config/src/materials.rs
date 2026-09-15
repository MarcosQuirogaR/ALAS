// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/materials.py
// Reference: alas @ rust-port-baseline.

//! Structural materials the wingbox sizing selects from by name.
//!
//! The table itself is not Rust source. `alas/config/materials.py` is a
//! sequence of constructor calls registering immutable records of published
//! material properties: data written as code because a constructor is the
//! shortest thing to hand in Python, not because anything about it is
//! executable. It lives here as `data/materials.json`, embedded in the crate
//! and parsed the first time it is asked for, which keeps a table reviewable
//! as a table and makes a corrected figure a one-line data change.
//!
//! That embedded copy is deliberately distinct from
//! `golden/config/materials.json`, the parity fixture: a shipped binary must
//! not need `golden/` to exist on disk. `tests/parity_databases.rs` is what
//! keeps the two from drifting apart.
//!
//! `f_allow_pa` is a single design allowable rather than a tension and
//! compression pair, which is the same simplification the reference scripts
//! made. It is appropriate for a strength-based preliminary sizing pass and
//! is not a certified stress analysis.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

// `env!("CARGO_MANIFEST_DIR")` at compile time is what makes this path
// resolve the same way regardless of the caller's working directory:
// `include_str!` itself only accepts a path relative to this source file, so
// the manifest-dir prefix is for readers, not for the compiler.
const MATERIALS_JSON: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/materials.json"));

/// A material that was asked for and is not in the table.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown material '{name}'; available: {}", available.join(", "))]
pub struct UnknownMaterial {
    /// What was asked for.
    pub name: String,
    /// What there is, sorted.
    pub available: Vec<String>,
}

/// One structural material's properties.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialSpec {
    /// Display name, and the key the configuration selects it by.
    pub name: String,
    /// `metallic` or `composite`. Informational: nothing branches on it.
    pub category: String,
    /// Young's modulus, in pascals.
    pub e_pa: f64,
    /// Poisson's ratio.
    pub nu: f64,
    /// Density, in kilograms per cubic metre.
    pub rho_kg_m3: f64,
    /// Design allowable stress, in pascals, with no safety factor applied.
    ///
    /// `StructuresConfig`'s additional safety factor is applied on top of
    /// this, so folding one in here would apply it twice.
    pub f_allow_pa: f64,
}

impl MaterialSpec {
    /// Shear modulus, from the isotropic relation `E / (2 (1 + nu))`.
    ///
    /// Carbon composite is not isotropic, so this is not its true shear
    /// modulus. It is what the reference scripts used, treating even the
    /// unidirectional cap material as isotropic for the mesh, and the port
    /// reproduces that rather than silently improving it.
    pub fn g_pa(&self) -> f64 {
        self.e_pa / (2.0 * (1.0 + self.nu))
    }
}

/// Every registered material, in the order the table lists them.
pub fn database() -> &'static [MaterialSpec] {
    static DATABASE: OnceLock<Vec<MaterialSpec>> = OnceLock::new();
    DATABASE.get_or_init(|| parse().materials)
}

/// Look one material up by name.
///
/// # Errors
///
/// [`UnknownMaterial`], carrying what is available, when the name is not in
/// the table. Upstream raises for the same input.
pub fn get(name: &str) -> Result<&'static MaterialSpec, UnknownMaterial> {
    database()
        .iter()
        .find(|material| material.name == name)
        .ok_or_else(|| UnknownMaterial {
            name: name.to_owned(),
            available: available().iter().map(|&name| name.to_owned()).collect(),
        })
}

/// Every material's name, sorted.
pub fn available() -> &'static [&'static str] {
    static AVAILABLE: OnceLock<Vec<&'static str>> = OnceLock::new();
    AVAILABLE.get_or_init(|| {
        let mut names: Vec<&'static str> = database()
            .iter()
            .map(|material| material.name.as_str())
            .collect();
        names.sort_unstable();
        names
    })
}

#[derive(Deserialize)]
struct Table {
    materials: Vec<MaterialSpec>,
}

/// Parse the embedded table.
///
/// A parse failure is reported and degrades to an empty table rather than
/// panicking, matching how this crate's sibling embedded data behaves: the
/// file is generated and checked in, so this should never happen, and if it
/// ever does the material lookup reports "unknown material" rather than
/// taking the program down.
fn parse() -> Table {
    serde_json::from_str(MATERIALS_JSON).unwrap_or_else(|error| {
        tracing::error!(%error, "crates/alas-config/data/materials.json failed to parse");
        Table {
            materials: Vec::new(),
        }
    })
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_table_parses_into_the_materials_the_reference_registers() {
        assert_eq!(database().len(), 11);
        assert!(get("Al 7075-T6").is_ok());
    }

    #[test]
    fn an_unknown_material_is_an_error_that_says_what_there_is() {
        let error = get("Unobtainium").unwrap_err();
        assert_eq!(error.name, "Unobtainium");
        assert!(error.available.contains(&"Al 7075-T6".to_owned()));
        assert!(format!("{error}").contains("Al 7075-T6"));
    }

    #[test]
    fn the_available_list_is_sorted_and_complete() {
        let names = available();
        assert_eq!(names.len(), database().len());
        assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn the_shear_modulus_follows_the_isotropic_relation() {
        let aluminum = get("Al 7075-T6").unwrap();
        let expected = aluminum.e_pa / (2.0 * (1.0 + aluminum.nu));
        assert!((aluminum.g_pa() - expected).abs() < 1e-6);
        // Roughly E/2.7 for a Poisson's ratio near a third, which is the
        // sanity check a reader can do in their head.
        assert!(aluminum.g_pa() > 25.0e9 && aluminum.g_pa() < 30.0e9);
    }

    #[test]
    fn every_material_has_physically_possible_properties() {
        for material in database() {
            assert!(material.e_pa > 0.0, "{}: modulus", material.name);
            assert!(
                material.nu > 0.0 && material.nu < 0.5,
                "{}: Poisson's ratio {} is outside the isotropic range",
                material.name,
                material.nu
            );
            assert!(material.rho_kg_m3 > 0.0, "{}: density", material.name);
            assert!(
                material.f_allow_pa > 0.0 && material.f_allow_pa < material.e_pa,
                "{}: an allowable at or above the modulus implies yielding \
                 past 100% strain",
                material.name
            );
        }
    }
}
