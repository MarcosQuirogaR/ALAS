// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/engines.py
// Reference: alas @ rust-port-baseline.

//! Turbofans the presets select from, with the data to draw and size them.
//!
//! Each entry carries enough geometry to generate a correctly scaled nacelle
//! and enough cycle data to size a turbofan network for the mission. As with
//! [`crate::materials`], the table is data rather than code: it lives in
//! `data/engines.json`, embedded and parsed once, with
//! `golden/config/engines.json` as the parity copy.
//!
//! **On the provenance of these numbers.** Thrust, fan diameter and bypass
//! ratio are manufacturer-published, and overall pressure ratio is published
//! for most of these engines. Fan pressure ratio, turbine inlet temperature
//! and cruise specific fuel consumption are published for none of them; the
//! values here are engineering estimates, following the well-known inverse
//! relationship between bypass ratio and fan pressure ratio and each family's
//! typical efficiency class. They are appropriate for conceptual design and
//! are not certified data. That distinction is repeated here because a
//! results table showing a fuel burn to four figures invites the assumption
//! that the engine deck behind it was measured.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

// See `crate::materials` for why the manifest directory is spelled out.
const ENGINES_JSON: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/engines.json"));

/// Stations of the nacelle silhouette, as fractions of nacelle length paired
/// with the radius fraction there.
///
/// A blunt inlet lip, a rapid expansion to the fan cowl, a constant-section
/// core cowl, then a taper to the nozzle. These proportions describe a
/// generic high-bypass installation and are shared by every engine; only the
/// length and maximum radius differ.
const NACELLE_STATIONS: &[(f64, f64)] = &[
    (0.0, 0.40),
    (0.08, 0.92),
    (0.15, 1.00),
    (0.55, 1.00),
    (0.75, 0.82),
    (1.0, 0.45),
];

/// An engine that was asked for and is not in the table.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown engine '{name}'; available: {}", available.join(", "))]
pub struct UnknownEngine {
    /// What was asked for.
    pub name: String,
    /// What there is, sorted.
    pub available: Vec<String>,
}

/// One turbofan's published and estimated specification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineSpec {
    /// Display name, and the key the configuration selects it by.
    pub name: String,
    /// Who builds it.
    pub manufacturer: String,
    /// Maximum rated takeoff thrust per engine, in kilonewtons. Published.
    pub thrust_kn: f64,
    /// Fan diameter, in metres. Published.
    pub fan_diameter_m: f64,
    /// Bypass ratio. Published.
    pub bypass_ratio: f64,
    /// Approximate nacelle length, in metres.
    pub nacelle_length_m: f64,
    /// Maximum external nacelle radius, in metres.
    pub nacelle_max_radius_m: f64,
    /// Overall pressure ratio. Published for most of these engines.
    pub overall_pressure_ratio: f64,
    /// Turbine inlet temperature, in kelvin. Estimated; see the module doc.
    pub turbine_inlet_temp_k: f64,
    /// Fan pressure ratio. Estimated; see the module doc.
    pub fan_pressure_ratio: f64,
    /// Cruise thrust-specific fuel consumption, in kilograms of fuel per
    /// kilogram-force of thrust per hour. Estimated; see the module doc.
    pub cruise_tsfc_kg_kgf_hr: f64,
    /// Normalized fuel flow at 7%, 30%, 85%, and 100% rated net thrust.
    /// Values are from, or explicitly proxied to, the ICAO Engine Emissions
    /// Databank's sea-level-static LTO points.
    #[serde(default = "default_part_power_fuel_flow_ratios")]
    pub part_power_fuel_flow_ratios: [f64; 4],
    /// Databank UID/variant or an explicit family-proxy statement.
    #[serde(default = "default_part_power_source")]
    pub part_power_source: String,
}

fn default_part_power_fuel_flow_ratios() -> [f64; 4] {
    [0.089, 0.275, 0.822, 1.0]
}

fn default_part_power_source() -> String {
    "uncalibrated representative ICAO-LTO schedule".to_owned()
}

impl EngineSpec {
    /// The nacelle silhouette, as `(x station in metres, radius fraction)`
    /// pairs from the inlet face to the nozzle exit.
    ///
    /// The radius fraction is relative to [`EngineSpec::nacelle_max_radius_m`],
    /// so the pair is what the geometry builder revolves.
    pub fn nacelle_profile(&self) -> Vec<(f64, f64)> {
        NACELLE_STATIONS
            .iter()
            .map(|&(fraction, radius)| (fraction * self.nacelle_length_m, radius))
            .collect()
    }
}

/// Every registered engine, in the order the table lists them.
pub fn database() -> &'static [EngineSpec] {
    static DATABASE: OnceLock<Vec<EngineSpec>> = OnceLock::new();
    DATABASE.get_or_init(|| parse().engines)
}

/// Look one engine up by name.
///
/// # Errors
///
/// [`UnknownEngine`], carrying what is available, when the name is not in the
/// table. Upstream raises for the same input.
pub fn get(name: &str) -> Result<&'static EngineSpec, UnknownEngine> {
    database()
        .iter()
        .find(|engine| engine.name == name)
        .ok_or_else(|| UnknownEngine {
            name: name.to_owned(),
            available: available().iter().map(|&name| name.to_owned()).collect(),
        })
}

/// Every engine's name, sorted.
pub fn available() -> &'static [&'static str] {
    static AVAILABLE: OnceLock<Vec<&'static str>> = OnceLock::new();
    AVAILABLE.get_or_init(|| {
        let mut names: Vec<&'static str> = database()
            .iter()
            .map(|engine| engine.name.as_str())
            .collect();
        names.sort_unstable();
        names
    })
}

#[derive(Deserialize)]
struct Table {
    engines: Vec<EngineSpec>,
}

/// Parse the embedded table. See [`crate::materials`] for why a parse failure
/// degrades rather than panicking.
fn parse() -> Table {
    serde_json::from_str(ENGINES_JSON).unwrap_or_else(|error| {
        tracing::error!(%error, "crates/alas-config/data/engines.json failed to parse");
        Table {
            engines: Vec::new(),
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
    fn the_embedded_table_keeps_the_reference_engines_and_adds_certified_preset_variants() {
        assert_eq!(database().len(), 10);
        assert!(get("GE9X").is_ok());
        assert!(get("CFM56-5B4/3").is_ok());
        assert!(get("CFM56-5C3/F").is_ok());
        assert!(get("Trent 970-84").is_ok());
    }

    #[test]
    fn an_unknown_engine_is_an_error_that_says_what_there_is() {
        let error = get("Merlin").unwrap_err();
        assert!(format!("{error}").contains("GE9X"));
    }

    #[test]
    fn the_nacelle_profile_runs_from_the_inlet_face_to_the_nozzle() {
        let engine = get("GE9X").unwrap();
        let profile = engine.nacelle_profile();
        assert_eq!(profile.first().unwrap().0, 0.0);
        assert_eq!(profile.last().unwrap().0, engine.nacelle_length_m);
        assert!(profile.windows(2).all(|pair| pair[0].0 < pair[1].0));
    }

    #[test]
    fn the_nacelle_reaches_its_full_radius_only_over_the_fan_cowl() {
        // The radius fraction is relative to the maximum, so a station above
        // one would put the drawn nacelle outside the radius everything else
        // is sized from.
        let profile = get("GE9X").unwrap().nacelle_profile();
        assert!(profile.iter().all(|&(_, radius)| radius <= 1.0));
        assert_eq!(profile.iter().filter(|&&(_, r)| r == 1.0).count(), 2);
    }

    #[test]
    fn a_higher_bypass_engine_is_estimated_with_a_lower_fan_pressure_ratio() {
        // The inverse relationship between the two is what the estimated
        // values were built on, so a table that violated it would mean an
        // entry was transposed.
        let mut engines: Vec<&EngineSpec> = database().iter().collect();
        engines.sort_by(|a, b| a.bypass_ratio.total_cmp(&b.bypass_ratio));
        let lowest = engines.first().unwrap();
        let highest = engines.last().unwrap();
        assert!(
            highest.fan_pressure_ratio < lowest.fan_pressure_ratio,
            "{} (BPR {}) has FPR {}, {} (BPR {}) has FPR {}",
            highest.name,
            highest.bypass_ratio,
            highest.fan_pressure_ratio,
            lowest.name,
            lowest.bypass_ratio,
            lowest.fan_pressure_ratio
        );
    }
}
