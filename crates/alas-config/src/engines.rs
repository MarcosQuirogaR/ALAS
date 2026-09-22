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
//! **On provenance.** A value is only meaningful together with its engine
//! variant and operating condition.  The catalogue therefore keeps the
//! compatibility key used by presets separate from the evidence identity and
//! records the conditions and sources for ratings, cycle anchors, LTO fuel
//! flow and installed nacelle geometry.  Estimated cycle inputs remain useful
//! for conceptual design, but are not certified data or an OEM engine deck.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// Propulsive conversion technology selected by an engine catalogue row.
///
/// The enum is deliberately non-exhaustive so downstream orchestrators must
/// handle newly introduced technologies explicitly.
#[non_exhaustive]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropulsionTechnology {
    /// Gas turbine producing net thrust through fan and core exhaust streams.
    #[default]
    Turbofan,
    /// Free turbine delivering shaft power to a governed propeller.
    Turboprop,
}

/// Typed inputs consumed by the turbofan physics implementation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurbofanEngineSpec {
    /// Sea-level-static rated net thrust per engine, in kilonewtons.
    pub rated_thrust_kn: f64,
    /// Bypass-to-core mass-flow ratio.
    pub bypass_ratio: f64,
    /// Bypass ratio associated with the sea-level-static takeoff thrust deck.
    /// This is kept separate because cycle/design BPR and ICAO LTO BPR can
    /// refer to different engine conditions.
    #[serde(default)]
    pub takeoff_bypass_ratio: Option<f64>,
    /// Overall core compressor pressure ratio.
    pub overall_pressure_ratio: f64,
    /// Bypass fan total-pressure ratio.
    pub fan_pressure_ratio: f64,
    /// Combustor-exit stagnation temperature, in kelvin.
    pub turbine_inlet_temp_k: f64,
    /// Reference cruise TSFC in kg/(kgf hr).
    pub cruise_tsfc_kg_kgf_hr: f64,
    /// Fuel-flow ratios at ICAO LTO 7, 30, 85 and 100 percent thrust.
    pub part_power_fuel_flow_ratios: [f64; 4],
    /// ICAO LTO takeoff fuel flow per engine at sea-level static, kg/s.
    pub takeoff_fuel_flow_kg_s: f64,
    /// Evidence or proxy statement for the part-power schedule.
    pub part_power_source: String,
    /// Empirical maximum-climb thrust deck used by transport missions.
    pub off_design: TurbofanOffDesignSpec,
}

/// Engine-specific anchor and evidence for the transport turbofan lapse deck.
///
/// `cruise_reference_thrust_n` is per engine.  It is deliberately distinct
/// from the sea-level-static rating: conflating those conditions was the
/// principal source of the old mission solver's misleading linear lapse.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurbofanOffDesignSpec {
    /// Maximum-climb thrust per engine at the stated reference condition, N.
    pub cruise_reference_thrust_n: f64,
    /// Pressure altitude of the reference point, m.
    pub cruise_reference_altitude_m: f64,
    /// Mach number of the reference point.
    pub cruise_reference_mach: f64,
    /// Machine-readable evidence class: `direct-openap`,
    /// `openap-static-fallback`, or `family-proxy`.
    pub evidence: String,
    /// Exact source and, for estimates, the correlation used.
    pub source: String,
}

/// Typed PW127M/568F installation inputs consumed by the turboprop model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurbopropEngineSpec {
    /// Normal all-engines-operating takeoff shaft rating, per engine.
    pub takeoff_shaft_power_kw: f64,
    /// Maximum reserve/one-engine-inoperative shaft rating, per engine.
    pub maximum_reserve_shaft_power_kw: f64,
    /// Maximum continuous shaft rating, per engine.
    pub maximum_continuous_shaft_power_kw: f64,
    /// Maximum climb shaft-power installation limit, per engine.
    pub maximum_climb_shaft_power_kw: f64,
    /// Maximum cruise shaft-power installation limit, per engine.
    pub maximum_cruise_shaft_power_kw: f64,
    /// Published two-engine fuel flow at the maximum-cruise reference point.
    pub maximum_cruise_fuel_flow_kg_h: f64,
    /// Installed propeller designation.
    pub propeller_model: String,
    /// Propeller diameter, in metres.
    pub propeller_diameter_m: f64,
    /// Governed propeller rotational speed.
    pub governed_propeller_speed_rpm: f64,
    /// Engine-to-propeller speed reduction ratio.
    pub reduction_ratio: f64,
    /// Rating evidence and unit-conversion statement.
    pub rating_source: String,
    /// Propeller/installation geometry evidence.
    pub geometry_source: String,
}

impl TurbopropEngineSpec {
    /// Number of engines the catalogue's `maximum_cruise_fuel_flow_kg_h` is
    /// published for: the PW127-class data are quoted for the two-engine
    /// ATR installation, not per engine.
    pub const FUEL_FLOW_REFERENCE_ENGINES: usize = 2;

    /// The cruise fuel-flow anchor per installed engine, kg/h.
    pub fn cruise_fuel_flow_per_engine_kg_h(&self) -> f64 {
        self.maximum_cruise_fuel_flow_kg_h / Self::FUEL_FLOW_REFERENCE_ENGINES as f64
    }

    /// The constant maximum-cruise fuel flow of an installation of
    /// `installed_engines` identical engines, kg/h.
    ///
    /// Scales the published two-engine anchor linearly with the count: each
    /// engine is assumed to run at the same cruise rating and specific fuel
    /// consumption as in the reference installation. `None` when no engine
    /// is installed, so a caller cannot publish a range for an aircraft
    /// without propulsion.
    pub fn installed_cruise_fuel_flow_kg_h(&self, installed_engines: usize) -> Option<f64> {
        (installed_engines > 0)
            .then(|| self.cruise_fuel_flow_per_engine_kg_h() * installed_engines as f64)
    }
}

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
    /// Physics family. Legacy rows omit this and deserialize as turbofans.
    #[serde(default)]
    pub technology: PropulsionTechnology,
    /// Technology-specific data. Turbofans are materialized from the legacy
    /// flat mirror when this is absent; turboprops must provide it explicitly.
    #[serde(default)]
    pub turboprop: Option<TurbopropEngineSpec>,
    /// Turbofan-only off-design thrust anchor and its evidence status.
    #[serde(default)]
    pub off_design: Option<TurbofanOffDesignSpec>,
    /// Human-facing identity. This may be more specific than the stable
    /// compatibility key in [`EngineSpec::name`].
    #[serde(default)]
    pub display_name: String,
    /// Exact certified or emissions-databank variant represented by the
    /// physical anchors. Empty only for legacy external tables.
    #[serde(default)]
    pub evidence_variant: String,
    /// Additional compatibility lookup names. Aliases never create a second
    /// physical engine record.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Who builds it.
    pub manufacturer: String,
    /// Maximum rated takeoff thrust per engine, in kilonewtons. Published.
    pub thrust_kn: f64,
    /// Definition and ambient/operating condition of `thrust_kn`.
    #[serde(default)]
    pub thrust_rating_condition: String,
    /// Primary evidence for `thrust_kn`.
    #[serde(default)]
    pub thrust_source: String,
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
    /// Condition represented by bypass and overall pressure ratios.
    #[serde(default)]
    pub cycle_anchor_condition: String,
    /// Evidence for the bypass/overall-pressure-ratio anchors.
    #[serde(default)]
    pub cycle_anchor_source: String,
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
    /// Absolute ICAO LTO takeoff fuel flow per engine, kg/s. Zero only for
    /// non-turbofan technologies.
    #[serde(default)]
    pub takeoff_fuel_flow_kg_s: f64,
    /// Databank UID/variant or an explicit family-proxy statement.
    #[serde(default = "default_part_power_source")]
    pub part_power_source: String,
    /// Test condition of the part-power schedule.
    #[serde(default)]
    pub part_power_condition: String,
    /// Declares whether the schedule is a direct match, a rating/family proxy,
    /// or unavailable. This prevents provenance text from being interpreted.
    #[serde(default)]
    pub part_power_evidence: String,
    /// ICAO Engine Emissions Databank UID supplying the LTO anchors.
    #[serde(default)]
    pub lto_uid: Option<String>,
    /// Exact emissions-engine variant associated with `lto_uid`.
    #[serde(default)]
    pub lto_evidence_variant: Option<String>,
    /// ICAO LTO rated thrust, in kilonewtons. This is deliberately separate
    /// from the current conceptual-design input [`EngineSpec::thrust_kn`].
    #[serde(default)]
    pub lto_rated_thrust_kn: Option<f64>,
    /// Bypass ratio reported for the ICAO LTO record.
    #[serde(default)]
    pub lto_bypass_ratio: Option<f64>,
    /// Overall pressure ratio reported for the ICAO LTO record.
    #[serde(default)]
    pub lto_overall_pressure_ratio: Option<f64>,
    /// What the nacelle dimensions describe (installed envelope versus basic
    /// engine dimensions).
    #[serde(default)]
    pub geometry_basis: String,
    /// Evidence or modelling status of the nacelle dimensions.
    #[serde(default)]
    pub geometry_source: String,
}

fn default_part_power_fuel_flow_ratios() -> [f64; 4] {
    [0.089, 0.275, 0.822, 1.0]
}

fn default_part_power_source() -> String {
    "uncalibrated representative ICAO-LTO schedule".to_owned()
}

impl EngineSpec {
    /// Materialize the typed turbofan payload without changing the stable
    /// on-disk representation of the ten legacy rows.
    pub fn turbofan_spec(&self) -> Option<TurbofanEngineSpec> {
        let off_design = self.off_design.clone()?;
        // The legacy design scalars are retained for configuration parity,
        // but a matched ICAO record is the authoritative sea-level-static
        // rating and takeoff BPR for the empirical mission deck. Never apply
        // a family proxy to an unidentified engine such as the generic GE9X
        // or Trent 900 entries.
        let identity_qualified_lto = matches!(
            self.part_power_evidence.as_str(),
            "direct" | "direct-rating-match" | "rating-proxy" | "family-match"
        );
        let rated_thrust_kn = if identity_qualified_lto {
            self.lto_rated_thrust_kn.unwrap_or(self.thrust_kn)
        } else {
            self.thrust_kn
        };
        let takeoff_bypass_ratio = if identity_qualified_lto {
            self.lto_bypass_ratio.unwrap_or(self.bypass_ratio)
        } else {
            self.bypass_ratio
        };
        (self.technology == PropulsionTechnology::Turbofan).then(|| TurbofanEngineSpec {
            rated_thrust_kn,
            bypass_ratio: self.bypass_ratio,
            takeoff_bypass_ratio: Some(takeoff_bypass_ratio),
            overall_pressure_ratio: self.overall_pressure_ratio,
            fan_pressure_ratio: self.fan_pressure_ratio,
            turbine_inlet_temp_k: self.turbine_inlet_temp_k,
            cruise_tsfc_kg_kgf_hr: self.cruise_tsfc_kg_kgf_hr,
            part_power_fuel_flow_ratios: self.part_power_fuel_flow_ratios,
            takeoff_fuel_flow_kg_s: self.takeoff_fuel_flow_kg_s,
            part_power_source: self.part_power_source.clone(),
            off_design,
        })
    }

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
        .find(|engine| engine.name == name || engine.aliases.iter().any(|alias| alias == name))
        .ok_or_else(|| UnknownEngine {
            name: name.to_owned(),
            available: available().iter().map(|&name| name.to_owned()).collect(),
        })
}

/// Every canonical and compatibility lookup name, sorted.
pub fn available() -> &'static [&'static str] {
    static AVAILABLE: OnceLock<Vec<&'static str>> = OnceLock::new();
    AVAILABLE.get_or_init(|| {
        let mut names: Vec<&'static str> = database()
            .iter()
            .flat_map(|engine| {
                std::iter::once(engine.name.as_str())
                    .chain(engine.aliases.iter().map(String::as_str))
            })
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
        assert_eq!(database().len(), 11);
        assert!(get("GE9X").is_ok());
        assert!(get("CFM56-5B4/3").is_ok());
        assert!(get("CFM56-5C3/F").is_ok());
        assert!(get("Trent 970-84").is_ok());
        assert!(get("PW127M").is_ok());
    }

    #[test]
    fn catalogue_anchors_identify_exact_variants_and_conditions() {
        let cfm = get("CFM56-5B4/3").unwrap();
        assert_eq!(cfm.evidence_variant, "CFM56-5B4/3");
        assert_eq!(cfm.thrust_kn, 117.77);
        assert_eq!(cfm.bypass_ratio, 5.5);
        assert_eq!(cfm.overall_pressure_ratio, 32.6);
        assert_eq!(cfm.lto_rated_thrust_kn, Some(120.10));
        assert_eq!(cfm.lto_bypass_ratio, Some(5.70));
        assert_eq!(cfm.lto_overall_pressure_ratio, Some(27.30));
        assert_eq!(cfm.part_power_evidence, "direct");
        assert!(cfm.part_power_condition.contains("sea-level static"));

        let leap = get("LEAP-1A").unwrap();
        assert_eq!(leap.evidence_variant, "LEAP-1A26/26E1");
        assert_eq!(leap.thrust_kn, 120.64);
        assert_eq!(leap.bypass_ratio, 11.0);
        assert_eq!(leap.overall_pressure_ratio, 40.0);
        assert_eq!(leap.lto_rated_thrust_kn, Some(120.636));
        assert_eq!(leap.lto_bypass_ratio, Some(10.706));
        assert_eq!(leap.lto_overall_pressure_ratio, Some(33.177));
    }

    #[test]
    fn generic_and_exact_trent_records_remain_distinct_compatibility_inputs() {
        let generic = get("Trent 900").unwrap();
        let exact = get("Trent 970-84").unwrap();
        assert!(!std::ptr::eq(generic, exact));
        assert_eq!(generic.thrust_kn, 374.0);
        assert_eq!(exact.thrust_kn, 334.29);
        assert_eq!(generic.part_power_evidence, "family-proxy");
        assert_eq!(
            generic.lto_evidence_variant.as_deref(),
            Some("Trent 970-84")
        );
    }

    #[test]
    fn genx_mission_anchor_declares_aircraft_level_calibration_scope() {
        let genx = get("GEnx-1B").unwrap();
        let off_design = genx.off_design.as_ref().unwrap();
        assert_eq!(off_design.evidence, "aircraft-kinematic-calibration");
        assert_eq!(off_design.cruise_reference_thrust_n, 85_000.0);
        assert!(off_design
            .source
            .contains("not a measured engine thrust deck"));
    }

    #[test]
    fn corrected_lto_anchors_do_not_overwrite_live_design_inputs() {
        let anchors = [
            ("CFM56-5C", "2CM015", 151.25, 6.60, 31.15),
            ("CFM56-5C3/F", "1CM011", 144.56, 6.60, 29.90),
            ("Trent 970-84", "18RR081", 338.7, 8.45, 38.0),
            ("LEAP-1A", "08P28CM155", 120.636, 10.706, 33.177),
            ("CF6-50", "3GE070", 224.2, 4.30, 27.76),
        ];
        for (name, uid, thrust, bypass_ratio, pressure_ratio) in anchors {
            let engine = get(name).unwrap();
            assert_eq!(engine.lto_uid.as_deref(), Some(uid), "{name}");
            assert_eq!(engine.lto_rated_thrust_kn, Some(thrust), "{name}");
            assert_eq!(engine.lto_bypass_ratio, Some(bypass_ratio), "{name}");
            assert_eq!(
                engine.lto_overall_pressure_ratio,
                Some(pressure_ratio),
                "{name}"
            );
        }

        let pw1521g = get("PW1500G").unwrap();
        assert_eq!(pw1521g.thrust_kn, 97.73);
        assert_eq!(pw1521g.lto_rated_thrust_kn, Some(97.72));
        assert_eq!(pw1521g.evidence_variant, "PW1521G/PW1521G-3");
    }

    #[test]
    fn typed_empirical_payload_uses_identity_qualified_lto_ratings() {
        for name in [
            "CFM56-5B4/3",
            "CFM56-5C",
            "CFM56-5C3/F",
            "Trent 970-84",
            "LEAP-1A",
            "PW1500G",
            "CF6-50",
        ] {
            let engine = get(name).unwrap();
            let typed = engine
                .turbofan_spec()
                .unwrap_or_else(|| panic!("{name} typed payload"));
            assert_eq!(
                typed.rated_thrust_kn,
                engine.lto_rated_thrust_kn.unwrap(),
                "{name}"
            );
            if let Some(lto_bypass_ratio) = engine.lto_bypass_ratio {
                assert_eq!(typed.takeoff_bypass_ratio, Some(lto_bypass_ratio), "{name}");
                assert_eq!(typed.bypass_ratio, engine.bypass_ratio, "{name}");
            }
        }

        for name in ["GE9X", "Trent 900"] {
            let engine = get(name).unwrap();
            let typed = engine.turbofan_spec().unwrap();
            assert_eq!(typed.rated_thrust_kn, engine.thrust_kn, "{name}");
            assert_eq!(typed.bypass_ratio, engine.bypass_ratio, "{name}");
            assert_eq!(
                typed.takeoff_bypass_ratio,
                Some(engine.bypass_ratio),
                "{name}"
            );
        }
    }

    #[test]
    fn proxies_are_machine_readable_and_not_claimed_as_validation() {
        let ge9x = get("GE9X").unwrap();
        assert_eq!(ge9x.part_power_evidence, "unvalidated-family-proxy");
        let off_design = ge9x.off_design.as_ref().expect("GE9X mission anchor");
        assert_eq!(off_design.evidence, "aircraft-requirement-calibration");
        assert!(off_design.source.contains("synthetic requirement"));
        assert!(off_design.source.contains("not an OEM GE9X thrust deck"));
        assert!(ge9x.part_power_source.contains("GEnx-1B74/75/P2"));
        assert!(!ge9x.part_power_source.contains("GE9X measured"));
        assert_eq!(ge9x.geometry_basis, "installed-nacelle-envelope");
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
        let mut engines: Vec<&EngineSpec> = database()
            .iter()
            .filter(|engine| engine.technology == PropulsionTechnology::Turbofan)
            .collect();
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

    #[test]
    fn pw127m_is_a_typed_power_rated_engine_not_a_zero_thrust_turbofan() {
        let engine = get("PW127M").unwrap();
        assert_eq!(engine.technology, PropulsionTechnology::Turboprop);
        assert!(engine.turbofan_spec().is_none());
        let prop = engine.turboprop.as_ref().unwrap();
        assert_eq!(prop.propeller_model, "Hamilton Sundstrand 568F-1");
        assert_eq!(prop.governed_propeller_speed_rpm, 1200.0);
        assert!(prop.takeoff_shaft_power_kw > 1_800.0);
    }
}
