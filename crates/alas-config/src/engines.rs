// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/engines.py

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

mod altitude_datum;
pub use altitude_datum::{ALTITUDE_DATUM_CLOSING_DOCUMENT, ALTITUDE_DATUM_GAP_MARKER};
mod nacelle_shape;
pub use nacelle_shape::{
    NacelleDrawnOutline, NacelleParamSource, NacelleShape, NacelleShapeV2, CORE_COWL_START_FRACTION,
};

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
    // Needs data: size dependence of TSFC/PSFC needs same-family TCDS pairs (not on disk); the flat per-engine value stays.
    pub cruise_tsfc_kg_kgf_hr: f64,
    /// Fuel-flow ratios at ICAO LTO 7, 30, 85 and 100 percent thrust.
    pub part_power_fuel_flow_ratios: [f64; 4],
    /// ICAO LTO takeoff fuel flow per engine at sea-level static, kg/s.
    pub takeoff_fuel_flow_kg_s: f64,
    /// Evidence or proxy statement for the part-power schedule.
    pub part_power_source: String,
    /// Empirical maximum-climb thrust deck used by transport missions.
    pub off_design: TurbofanOffDesignSpec,
    /// Ambient static temperature at sea-level static up to which the thrust
    /// rating is flat (the engine certificate's flat-rating temperature), in
    /// kelvin. `None` (every catalogue row today: no per-engine datum has been
    /// retrieved) means the rating is not derated on a hot day.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flat_rating_temperature_k: Option<f64>,
}

/// Cruise reference point and evidence for the transport turbofan deck.
///
/// `cruise_reference_thrust_n` is per engine, a value of the rating `rating`
/// names. An engine datum scales Jenkinson's maximum-climb charts onto it;
/// without one the charts run unscaled ([`Self::altitude_datum_gap`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurbofanOffDesignSpec {
    /// Cruise thrust per engine at the stated reference condition, at which
    /// the catalogue cruise TSFC is quoted, N.
    pub cruise_reference_thrust_n: f64,
    /// Pressure altitude of the reference point, m.
    pub cruise_reference_altitude_m: f64,
    /// Mach number of the reference point.
    pub cruise_reference_mach: f64,
    /// Machine-readable evidence class: `direct-openap`,
    /// `openap-static-fallback`, `family-proxy`, or `secondary-simulation`
    /// (a grade C open-source engine-simulation table, cross-checked).
    pub evidence: String,
    /// Exact source and, for estimates, the correlation used.
    pub source: String,
    /// Which rating the cruise reference thrust is a value of. Absent means
    /// a maximum-cruise value (all catalogue anchors but LEAP-1A and LEAP-1C);
    /// a `maximum-climb` datum scales the deck's maximum-climb chart directly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rating: Option<String>,
}

/// Openings of sources stating the class statistic `0.2 F_TO + 890 N` (Svoboda
/// 2000, OpenAP's thrust fallback), as saved rows under another evidence label.
pub const CLASS_STATISTIC_SOURCE_PREFIXES: [&str; 2] =
    ["OpenAP thrust.py Svoboda fallback", "ASSUMED: Svoboda form"];

impl TurbofanOffDesignSpec {
    /// Whether the cruise point is a published datum of the engine (a
    /// manufacturer-class entry of the engine or of its declared family), as
    /// opposed to the OpenAP/Svoboda class statistic `0.2 F_TO + 890 N`,
    /// which is computed from the rating and says nothing about the engine.
    /// Only the former may anchor the engine's thrust scale in the deck.
    pub fn is_engine_datum(&self) -> bool {
        matches!(
            self.evidence.as_str(),
            "direct-openap" | "family-proxy" | "secondary-simulation"
        ) && !self.is_class_statistic()
    }

    /// Whether the cruise point is the class statistic `0.2 F_TO + 890 N`:
    /// declared by the `openap-static-fallback` evidence, or by a source that
    /// opens with one of [`CLASS_STATISTIC_SOURCE_PREFIXES`] (rows, and
    /// configurations saved from them, that state the statistic under another
    /// evidence label). A datum's source may name the statistic it replaced
    /// without being one.
    pub fn is_class_statistic(&self) -> bool {
        self.evidence == "openap-static-fallback"
            || CLASS_STATISTIC_SOURCE_PREFIXES
                .iter()
                .any(|prefix| self.source.trim_start().starts_with(prefix))
    }

    /// Whether the datum is a maximum-climb thrust (`rating` is
    /// `maximum-climb`) rather than a maximum-cruise one.
    pub fn is_maximum_climb_datum(&self) -> bool {
        self.rating.as_deref() == Some("maximum-climb")
    }
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
    /// Sea-level ambient temperature, K, to which the take-off ratings are
    /// flat-rated: the take-off corner of the shaft-power lapse
    /// (`rho_corner = 1.225 kg/m^3 x 288.15 K / T`). Absent in a saved
    /// configuration from before the field existed: the PW127M value.
    #[serde(default = "default_takeoff_flat_rating_temperature_k")]
    pub takeoff_flat_rating_temperature_k: f64,
    /// Sea-level ambient temperature, K, to which the maximum-continuous
    /// rating is flat-rated; the climb, cruise and flight-idle ratings share
    /// this corner. Absent in an older saved configuration: the PW127M value.
    #[serde(default = "default_maximum_continuous_flat_rating_temperature_k")]
    pub maximum_continuous_flat_rating_temperature_k: f64,
    /// Exponent `n` of the shaft-power lapse `(rho / rho_corner)^n` below the
    /// flat-rating corner. Absent in an older saved configuration: the PW127M
    /// value.
    #[serde(default = "default_power_lapse_density_exponent")]
    pub power_lapse_density_exponent: f64,
    /// Evidence for the flat-rating temperatures and the lapse exponent.
    #[serde(default)]
    pub power_lapse_source: String,
    /// Exponent `k` of the ram factor `(1 + 0.2 M^2)^k` on the lapsed shaft
    /// power; 0 is no ram factor. Absent in an older saved configuration: 0,
    /// the PW127M value, whose lapse fit already holds its cruise schedule.
    #[serde(default)]
    pub ram_power_exponent: f64,
    /// True airspeed, m/s, of the speed schedule the lapse exponent was fitted
    /// on, so the ram factor does not count that schedule's ram twice; 0 for
    /// a static fit. Absent in an older saved configuration: 0.
    #[serde(default)]
    pub power_lapse_fit_reference_true_airspeed_m_s: f64,
    /// Evidence for the ram exponent and the fit's reference speed.
    #[serde(default)]
    pub ram_power_source: String,
    /// Ambient temperature, K, above which no rating is held at its full
    /// value, for an engine certificated at one temperature with no
    /// flat-rating temperature; 0 is no limit. Absent in an older saved
    /// configuration: 0, the PW127M value, whose corners are sea-level
    /// temperature limits already.
    #[serde(default)]
    pub rating_temperature_limit_k: f64,
    /// Thermodynamic (gas-generator) shaft power at sea level, kW, when the
    /// engine's documents state one; 0 is none. Stated, it sets each
    /// rating's flat-rating corner (see `power_lapse_source`). Absent in an
    /// older saved configuration: 0.
    #[serde(default)]
    pub thermodynamic_shaft_power_kw: f64,
    /// Power-specific fuel consumption the model burns on shaft power,
    /// kg/(kW h), at [`Self::psfc_reference_temperature_k`]. Absent in an
    /// older saved configuration: the PW127M value (Majeed 2009, PW120A
    /// flight record, 0.2945).
    #[serde(default = "default_psfc_reference_kg_kwh")]
    pub psfc_reference_kg_kwh: f64,
    /// Ambient static temperature, K, of `psfc_reference_kg_kwh`. Absent in
    /// an older saved configuration: the PW127M value, 249.65 K.
    #[serde(default = "default_psfc_reference_temperature_k")]
    pub psfc_reference_temperature_k: f64,
    /// Evidence for the PSFC and its temperature.
    #[serde(default)]
    pub psfc_source: String,
    /// Installed propeller designation.
    pub propeller_model: String,
    /// Propeller diameter, in metres.
    pub propeller_diameter_m: f64,
    /// Governed propeller rotational speed.
    pub governed_propeller_speed_rpm: f64,
    /// Engine-to-propeller speed reduction ratio.
    pub reduction_ratio: f64,
    /// Blade count of the installed propeller, from its type-certificate
    /// data sheet. Absent for a custom engine record, in which case a
    /// clean-sheet mass evaluation falls back to the shaft-power statistic of
    /// `FlopsTurbopropConfig::resolved_for_engine`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub propeller_blade_count: Option<u32>,
    /// Coefficient `c` of the propeller's helical-tip-Mach compressibility
    /// loss `1 - c (M_h - M_crit)^2` (see `alas_prop`'s
    /// `Pw127m568fModel::tip_mach_loss_coefficient`). The loss is a property
    /// of the blade design (thickness and sweep), so a propeller with its own
    /// measured class states it here; absent, the generic class value of the
    /// propeller model applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub propeller_tip_mach_loss_coefficient: Option<f64>,
    /// Evidence for `propeller_tip_mach_loss_coefficient`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub propeller_compressibility_source: String,
    /// Rating evidence and unit-conversion statement.
    pub rating_source: String,
    /// Propeller/installation geometry evidence.
    pub geometry_source: String,
}

/// EASA TCDS IM.E.041 section 5, PW127M row: take-off flat-rated to 39 C.
fn default_takeoff_flat_rating_temperature_k() -> f64 {
    312.15
}

/// EASA TCDS IM.E.041 section 5, PW127M row: maximum continuous to 48 C.
fn default_maximum_continuous_flat_rating_temperature_k() -> f64 {
    321.15
}

/// Nita (2008) eq. 3.5.11, the PW120 maximum-cruise power fit.
fn default_power_lapse_density_exponent() -> f64 {
    0.728
}

/// Majeed (2009) SRS-TSD-002 Tab. 3.3, PW120A flight record mean PSFC.
fn default_psfc_reference_kg_kwh() -> f64 {
    0.2945
}

/// Majeed (2009) Tab. 3.3: SAT -23.5 C.
fn default_psfc_reference_temperature_k() -> f64 {
    249.65
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
    /// Turbofan flat-rating temperature, K; absent when no datum is known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flat_rating_temperature_k: Option<f64>,
    /// Take-off mass of the aircraft the engine was sized for, kg, for the
    /// engine/closed-take-off-mass mismatch warning; absent
    /// when no datum is known (no catalogue row carries one yet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design_takeoff_mass_kg: Option<f64>,
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
    /// `thrust_kn` is the take-off rating the aircraft is operated at, as
    /// its own document prints it, rather than the engine certification
    /// rating of a matched ICAO databank row. The mission deck then flies
    /// `thrust_kn`, and re-reads the databank's LTO table at that rating.
    /// Absent means the databank row's rating governs.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub aircraft_installed_rating: bool,
    /// Take-off rating, kN, at which the thrust-scaled mass relations price
    /// this physical engine; absent means `thrust_kn`. Declared only when
    /// `thrust_kn` is a re-rating of hardware the evidence also lists under
    /// another rating, so that a rating change alone never sizes a heavier
    /// engine: a dry mass belongs to the hardware, not to the rating. See
    /// [`crate::EngineConfig::mass_basis_thrust_kn`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mass_basis_thrust_kn: Option<f64>,
    /// Evidence that `mass_basis_thrust_kn` rates the same hardware.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mass_basis_source: String,
    /// Fan diameter, in metres. Published.
    pub fan_diameter_m: f64,
    /// Bypass ratio. Published.
    pub bypass_ratio: f64,
    /// Approximate nacelle length, in metres.
    pub nacelle_length_m: f64,
    /// Maximum external nacelle radius, in metres.
    pub nacelle_max_radius_m: f64,
    /// Drawing-fitted outline parameters. When present,
    /// [`EngineSpec::nacelle_length_m`] is the inlet face to the plug tip and
    /// the silhouette comes from [`NacelleShape::profile`]; when absent the
    /// generic six-station profile is scaled to the length and radius.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nacelle_shape: Option<NacelleShape>,
    /// The G2 outline of the same installation: the six
    /// numbers of [`EngineSpec::nacelle_shape`] plus lip, boat-tail, trailing
    /// edge, plug tip, ellipticity and droop. Opt-in: the geometry builder
    /// reads it only when asked; the legacy silhouette is unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nacelle_shape_v2: Option<NacelleShapeV2>,
    /// Plan and side outlines and the inlet scarf a revolve cannot make.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nacelle_drawn_outline: Option<NacelleDrawnOutline>,
    /// An explicit silhouette, `(station in metres, radius fraction)` pairs
    /// from the nose to the aft end, for an installation that is not a
    /// turbofan pod (a spinner and a long nacelle behind a propeller). Used
    /// when no valid [`EngineSpec::nacelle_shape`] applies and the last station
    /// equals [`EngineSpec::nacelle_length_m`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nacelle_stations: Option<Vec<(f64, f64)>>,
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
    /// Source and page of `cruise_tsfc_kg_kgf_hr`. The value is an
    /// uninstalled manufacturer figure at its stated condition; the mission
    /// deck adds bleed and power-extraction losses on top.
    #[serde(default)]
    pub cruise_tsfc_source: String,
    /// Evidence class of `cruise_tsfc_kg_kgf_hr`: `published` (a printed
    /// value for this engine), `family-proxy` (printed for a sibling) or
    /// `unsourced-estimate` (no public datum; not validated).
    #[serde(default)]
    pub cruise_tsfc_evidence: String,
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

/// ICAO LTO thrust fractions of the databank's rated thrust.
const LTO_THRUST_FRACTIONS: [f64; 4] = [0.07, 0.30, 0.85, 1.0];

/// The LTO fuel-flow table of a databank row re-read at an installed rating
/// `scale` times the row's rated thrust: the take-off fuel flow at the
/// installed rating and the four ratios at 7, 30, 85 and 100 % of it, by
/// linear interpolation between the databank points (linear extrapolation
/// past the end segments).
fn rerate_lto_table(ratios: [f64; 4], takeoff_fuel_flow_kg_s: f64, scale: f64) -> ([f64; 4], f64) {
    let at = |fraction: f64| {
        let x = LTO_THRUST_FRACTIONS;
        let k = if fraction <= x[1] {
            0
        } else if fraction <= x[2] {
            1
        } else {
            2
        };
        ratios[k] + (fraction - x[k]) / (x[k + 1] - x[k]) * (ratios[k + 1] - ratios[k])
    };
    let full = at(scale);
    let rerated = LTO_THRUST_FRACTIONS.map(|fraction| at(scale * fraction) / full);
    (rerated, takeoff_fuel_flow_kg_s * full)
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
        // Rating basis: the take-off rating the aircraft is operated at. A
        // record flagged `aircraft_installed_rating` flies its `thrust_kn`,
        // the installed rating its aircraft document prints; the databank
        // row is then the certified rating of that hardware and its LTO
        // table is re-read at the installed rating. The mass basis is a
        // separate statement (`mass_basis_thrust_kn`).
        let installed_rating = self.aircraft_installed_rating;
        let rated_thrust_kn = if identity_qualified_lto && !installed_rating {
            self.lto_rated_thrust_kn.unwrap_or(self.thrust_kn)
        } else {
            self.thrust_kn
        };
        let (part_power_fuel_flow_ratios, takeoff_fuel_flow_kg_s) = match (
            identity_qualified_lto && installed_rating,
            self.lto_rated_thrust_kn,
        ) {
            (true, Some(lto_kn)) if lto_kn > 0.0 && self.thrust_kn != lto_kn => rerate_lto_table(
                self.part_power_fuel_flow_ratios,
                self.takeoff_fuel_flow_kg_s,
                self.thrust_kn / lto_kn,
            ),
            // A row borrowed from another rating of the family (a proxy, not
            // this engine's own row) lends its shape and its take-off fuel
            // flow per unit thrust, an intensive quantity. The absolute flow
            // is that of the row's rating, so it is carried to the record's
            // rating in proportion to thrust; the ratios stay the row's.
            (false, Some(lto_kn))
                if !identity_qualified_lto && lto_kn > 0.0 && self.thrust_kn != lto_kn =>
            {
                (
                    self.part_power_fuel_flow_ratios,
                    self.takeoff_fuel_flow_kg_s * self.thrust_kn / lto_kn,
                )
            }
            _ => (
                self.part_power_fuel_flow_ratios,
                self.takeoff_fuel_flow_kg_s,
            ),
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
            part_power_fuel_flow_ratios,
            takeoff_fuel_flow_kg_s,
            part_power_source: self.part_power_source.clone(),
            off_design,
            flat_rating_temperature_k: self.flat_rating_temperature_k,
        })
    }

    /// The nacelle silhouette, as `(x station in metres, radius fraction)`
    /// pairs from the inlet face to the nozzle exit.
    ///
    /// The radius fraction is relative to [`EngineSpec::nacelle_max_radius_m`],
    /// so the pair is what the geometry builder revolves.
    pub fn nacelle_profile(&self) -> Vec<(f64, f64)> {
        if let Some(shape) = self
            .nacelle_shape
            .as_ref()
            .filter(|shape| shape.is_valid(self.nacelle_length_m))
        {
            return shape.profile(self.nacelle_length_m, self.nacelle_max_radius_m);
        }
        if let Some(stations) = self.nacelle_stations.as_ref().filter(|stations| {
            stations.first().is_some_and(|first| first.0 == 0.0)
                && stations
                    .last()
                    .is_some_and(|last| last.0 == self.nacelle_length_m)
                && stations.windows(2).all(|pair| pair[0].0 < pair[1].0)
                && stations.iter().all(|&(_, r)| r > 0.0 && r <= 1.0)
        }) {
            return stations.clone();
        }
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
        // old 14 -> 15: CF6-80C2 (Boeing 747-400) turbofan entry added to the
        // catalogue that already held the E195-E2 and C919 engines
        // old 15 -> 16: PW305A (Learjet 60) appended.
        // old 16 -> 17: PW127G (Airbus C295) turboprop entry added (counted
        // 15 -> 16 on its own branch, before the PW305A merge)
        // old 17 -> 18: Trent XWB-84 (Airbus A350-900) entry added
        assert_eq!(database().len(), 18);
        assert!(get("PW305A").is_ok());
        assert!(get("PW127G").is_ok());
        assert!(get("Trent XWB-84").is_ok());
        assert!(get("GE9X").is_ok());
        assert!(get("CFM56-5B4/3").is_ok());
        assert!(get("CFM56-5C3/F").is_ok());
        assert!(get("Trent 970-84").is_ok());
        assert!(get("PW127M").is_ok());
        assert!(get("TP400-D6").is_ok());
    }

    /// No per-engine flat-rating temperature or design take-off mass has been
    /// retrieved, so no catalogue row carries one (the deck then does not
    /// derate a hot day), and a present value survives the typed payload.
    #[test]
    fn flat_rating_and_design_mass_are_optional_and_absent_from_the_catalogue() {
        for spec in database() {
            assert_eq!(spec.flat_rating_temperature_k, None, "{}", spec.name);
            assert_eq!(spec.design_takeoff_mass_kg, None, "{}", spec.name);
            let value = serde_json::to_value(spec).unwrap();
            assert!(value.get("flat_rating_temperature_k").is_none());
        }
        let mut cfm = get("CFM56-5B4/3").unwrap().clone();
        cfm.flat_rating_temperature_k = Some(303.15);
        let typed = cfm.turbofan_spec().unwrap();
        assert_eq!(typed.flat_rating_temperature_k, Some(303.15));
        let round_trip: TurbofanEngineSpec =
            serde_json::from_value(serde_json::to_value(&typed).unwrap()).unwrap();
        assert_eq!(round_trip, typed);
    }

    #[test]
    fn only_published_engine_cruise_points_anchor_the_deck_thrust_scale() {
        // CFM56-5B4/3 (OpenAP 2CM014 family anchor), CFM56-5C (2CM015),
        // CFM56-5C3/F (1CM011 -5C3 value, variant assumed), CF6-50 (CF6-50C1
        // row, 3GE073/3GE074 cruise value) and CF6-80C2 (2GE045) carry an
        // engine datum (grade B, M 0.80, 35,000 ft); LEAP-1A carries a
        // secondary-simulation datum (grade C, maximum-climb, M 0.78) and LEAP-1C
        // its ratio (family proxy, same type certificate EASA.E.110); PW305A
        // carries the PW305B pair of Jenkinson's engine data file (grade C
        // family proxy, same type certificate, M 0.80, 40,000 ft); the other modern
        // engines carry the Svoboda class statistic computed from the rating.
        let datum = |name: &str| {
            get(name)
                .unwrap()
                .turbofan_spec()
                .unwrap()
                .off_design
                .is_engine_datum()
        };
        assert!(datum("CFM56-5B4/3"));
        assert!(datum("CFM56-5C"));
        for name in [
            "CFM56-5C3/F",
            "CF6-50",
            "CF6-80C2",
            "LEAP-1A",
            "LEAP-1C",
            "PW305A",
        ] {
            assert!(datum(name), "{name}");
        }
        for name in ["GE9X", "Trent 900", "GEnx-1B", "PW1500G"] {
            assert!(!datum(name), "{name}");
        }
    }

    #[test]
    fn the_datum_gate_reads_the_evidence_not_a_word_of_the_source() {
        let spec = |evidence: &str, source: &str| TurbofanOffDesignSpec {
            cruise_reference_thrust_n: 4_428.0,
            cruise_reference_altitude_m: 12_192.0,
            cruise_reference_mach: 0.8,
            evidence: evidence.to_owned(),
            source: source.to_owned(),
            rating: None,
        };
        // A published family pair stays a datum when its source names the
        // class value it replaced.
        let replaced = "Family proxy, grade C, a printed pair. Replaces the earlier \
                        assumed class value 0.2 F_TO + 890 N (Svoboda 2000)";
        assert!(spec("family-proxy", replaced).is_engine_datum());
        assert!(!spec("family-proxy", replaced).is_class_statistic());
        // The statistic is declared by its evidence label, or by a source
        // that opens with the statistic under another label.
        assert!(!spec("openap-static-fallback", "any text").is_engine_datum());
        for prefix in CLASS_STATISTIC_SOURCE_PREFIXES {
            let stated = format!("{prefix}: 0.2 * rated thrust + 890 N per engine");
            assert!(spec("family-proxy", &stated).is_class_statistic(), "{prefix}");
            assert!(!spec("family-proxy", &stated).is_engine_datum(), "{prefix}");
        }
    }

    #[test]
    fn no_engine_datum_is_numerically_the_class_statistic() {
        // Guard independent of any text: a cruise point equal to
        // 0.2 F_TO + 890 N (within 2 N) is the statistic and may not anchor
        // the deck, and every catalogue datum differs from it.
        for name in available() {
            let engine = get(name).unwrap();
            let Some(off_design) = engine.off_design.as_ref() else {
                continue;
            };
            let svoboda_n = 0.2 * engine.thrust_kn * 1_000.0 + 890.0;
            let is_statistic = (off_design.cruise_reference_thrust_n - svoboda_n).abs() < 2.0;
            assert_eq!(off_design.is_class_statistic(), is_statistic, "{name}");
            assert_eq!(off_design.is_engine_datum(), !is_statistic, "{name}");
        }
    }

    #[test]
    fn the_pw305a_datum_is_the_pw305b_column_pair_carried_to_its_rating() {
        // Jenkinson's engine data file, PW305B column: 1,113 lb at M 0.8,
        // 40,000 ft with the 5,266 lb take-off rating of the same column,
        // carried to 20.46 kN; the cruise TSFC is the same column's 0.675.
        let engine = get("PW305A").unwrap();
        let off_design = engine.off_design.as_ref().unwrap();
        let ratio: f64 = 1_113.0 / 5_266.0;
        assert!((off_design.cruise_reference_thrust_n - ratio * 20_460.0).abs() < 1.0);
        assert_eq!(off_design.cruise_reference_altitude_m, 12_192.0);
        assert_eq!(off_design.cruise_reference_mach, 0.8);
        assert_eq!(off_design.evidence, "family-proxy");
        assert!(off_design.rating.is_none());
        assert!(!off_design.source.contains("Svoboda"));
        assert!(off_design.source.contains("PW305B column"));
        assert!(engine.cruise_tsfc_source.contains("PW305B column"));
        assert_eq!(engine.cruise_tsfc_kg_kgf_hr, 0.675);
        assert!(off_design.is_engine_datum());
    }

    #[test]
    fn catalogue_anchors_identify_exact_variants_and_conditions() {
        let cfm = get("CFM56-5B4/3").unwrap();
        assert_eq!(cfm.evidence_variant, "CFM56-5B4/3");
        assert_eq!(cfm.thrust_kn, 120.10);
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
        assert_eq!(exact.thrust_kn, 338.7);
        assert_eq!(generic.part_power_evidence, "family-proxy");
        assert_eq!(
            generic.lto_evidence_variant.as_deref(),
            Some("Trent 970-84")
        );
    }

    #[test]
    fn cruise_reference_thrusts_are_published_pairs_or_svobodas_class_value() {
        // Svoboda 2000 (via Schulz 2007, Eq. 2.12): F_CR = 200 lbf + 0.2 F_TO,
        // with OpenAP's 890 N for 200 lbf. No entry may carry an
        // aircraft-level thrust calibration.
        for name in available() {
            let engine = get(name).unwrap();
            let Some(off_design) = engine.off_design.as_ref() else {
                continue;
            };
            assert!(
                !off_design.evidence.starts_with("aircraft-"),
                "{name}: {}",
                off_design.evidence
            );
            if off_design.evidence == "openap-static-fallback" {
                let svoboda_n = 0.2 * engine.thrust_kn * 1_000.0 + 890.0;
                assert!(
                    (off_design.cruise_reference_thrust_n - svoboda_n).abs() < 0.5,
                    "{name}"
                );
            }
        }
    }

    #[test]
    fn the_leap_1a_anchor_is_a_grade_c_maximum_climb_datum() {
        // FlyByWire A32NX tables, 23.6 kN at FL350 M 0.78 ISA, cross-checked
        // by pilot fuel flow over TSFC; not the Svoboda class value, not the
        // PS or OpenAP-polar required-drag estimates.
        let engine = get("LEAP-1A").unwrap();
        let off_design = engine.off_design.as_ref().unwrap();
        assert_eq!(off_design.cruise_reference_thrust_n, 23_600.0);
        assert_eq!(off_design.cruise_reference_altitude_m, 10_668.0);
        assert_eq!(off_design.cruise_reference_mach, 0.78);
        assert_eq!(off_design.evidence, "secondary-simulation");
        assert!(off_design.is_maximum_climb_datum());
        assert!(off_design.is_engine_datum());
        assert!(off_design.source.contains("FlyByWire"));
        assert!(off_design.source.contains("Grade C"));
        assert!(off_design.source.contains("MAXIMUM CLIMB"));
        // The LEAP-1C carries the same datum as a family ratio (its own test in
        // `altitude_datum`); no other engine has a maximum-climb datum.
        for other in available() {
            if !["LEAP-1A", "LEAP-1C"].contains(other) {
                if let Some(other) = get(other).unwrap().off_design.as_ref() {
                    assert!(!other.is_maximum_climb_datum(), "{other:?}");
                }
            }
        }
    }

    #[test]
    fn corrected_lto_anchors_do_not_overwrite_live_design_inputs() {
        let anchors = [
            ("CFM56-5C", "2CM015", 151.25, 6.60, 31.15),
            ("CFM56-5C3/F", "1CM011", 144.56, 6.60, 29.90),
            ("Trent 970-84", "18RR081", 338.7, 8.45, 38.0),
            ("LEAP-1A", "08P28CM155", 120.636, 10.706, 33.177),
            ("CF6-50", "3GE073", 230.4, 4.30, 28.44),
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

        // The geared-turbofan cruise anchors are the grade-C family proxy of
        // the printed PW1400G / LEAP-1A SFC (0.51), not the legacy 0.50.
        for name in ["PW1500G", "PW1900G"] {
            let engine = get(name).unwrap();
            assert_eq!(engine.cruise_tsfc_kg_kgf_hr, 0.51, "{name}");
            assert_eq!(engine.cruise_tsfc_evidence, "family-proxy", "{name}");
            assert!(engine.cruise_tsfc_source.contains("PW1400G"), "{name}");
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
            // The borrowed row's take-off fuel flow per unit thrust is kept
            // at the record's rating: the deck must not pair the row's
            // absolute flow with a larger thrust.
            let row_kn = engine.lto_rated_thrust_kn.unwrap();
            let row_tsfc = engine.takeoff_fuel_flow_kg_s / row_kn;
            let deck_tsfc = typed.takeoff_fuel_flow_kg_s / typed.rated_thrust_kn;
            assert!((deck_tsfc - row_tsfc).abs() <= 1.0e-12 * row_tsfc, "{name}");
            assert_eq!(
                typed.part_power_fuel_flow_ratios, engine.part_power_fuel_flow_ratios,
                "{name}"
            );
        }
    }

    #[test]
    fn proxies_are_machine_readable_and_not_claimed_as_validation() {
        let ge9x = get("GE9X").unwrap();
        assert_eq!(ge9x.part_power_evidence, "unvalidated-family-proxy");
        let off_design = ge9x.off_design.as_ref().expect("GE9X mission anchor");
        assert_eq!(off_design.evidence, "openap-static-fallback");
        assert!(off_design.source.contains("Svoboda"));
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
    fn every_drawing_fitted_nacelle_is_a_valid_monotone_outline_of_the_stated_size() {
        let fitted: Vec<&EngineSpec> = database()
            .iter()
            .filter(|e| e.nacelle_shape.is_some() || e.nacelle_stations.is_some())
            .collect();
        assert!(fitted.len() >= 10, "{} fitted engines", fitted.len());
        for engine in fitted {
            let name = &engine.name;
            if let Some(shape) = engine.nacelle_shape.as_ref() {
                assert!(shape.is_valid(engine.nacelle_length_m), "{name}");
            }
            let profile = engine.nacelle_profile();
            assert_eq!(profile.first().unwrap().0, 0.0, "{name}");
            assert_eq!(profile.last().unwrap().0, engine.nacelle_length_m, "{name}");
            assert!(profile.windows(2).all(|p| p[0].0 < p[1].0), "{name}");
            assert!(profile.iter().all(|&(_, r)| r > 0.0 && r <= 1.0), "{name}");
            assert!(
                profile.len() > 6,
                "{name}: more than the generic six stations"
            );
            // The inlet highlight must clear the fan tip: a lip smaller than the
            // fan disc would not admit the flow. The outlines are fitted to the
            // manufacturer drawings (highlight below 0.9 of the maximum radius
            // for most of the fleet), so the old 0.9 floor is not a physical
            // limit. The Trent XWB-84 (A350 AC front view) and PW305A (PTM
            // front view lip circle) rows were re-read after this
            // check found them inside their own fans.
            if engine.technology == PropulsionTechnology::Turbofan {
                let highlight_m = profile[0].1 * engine.nacelle_max_radius_m;
                assert!(
                    highlight_m >= 0.5 * engine.fan_diameter_m,
                    "{name}: highlight radius {highlight_m} m, fan tip {} m",
                    0.5 * engine.fan_diameter_m
                );
            }
            // The refit to the manufacturer drawings cites the drawing itself
            // (figure or section of the airport-planning document) or says
            // "drawing-fitted"; a bare envelope statement is not a source.
            assert!(
                ["drawing-fitted", "figure", "section 2.2"]
                    .iter()
                    .any(|marker| engine.geometry_source.contains(marker)),
                "{name}: source not stated"
            );
        }
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

    #[test]
    fn every_turboprop_record_states_its_own_power_lapse_corners() {
        // The record itself carries the corners and the exponent (a serde
        // default only covers older saved configurations).
        let raw: serde_json::Value = serde_json::from_str(ENGINES_JSON).unwrap();
        let records = raw["engines"].as_array().unwrap();
        let mut turboprops = 0;
        for record in records {
            let Some(turboprop) = record.get("turboprop") else {
                continue;
            };
            turboprops += 1;
            for key in [
                "takeoff_flat_rating_temperature_k",
                "maximum_continuous_flat_rating_temperature_k",
                "power_lapse_density_exponent",
                "power_lapse_source",
            ] {
                assert!(turboprop.get(key).is_some(), "{} {key}", record["name"]);
            }
        }
        assert_eq!(turboprops, 3);
        // EASA TCDS IM.E.041 Issue 07 section 5: PW127M 39 C take-off and
        // 48 C maximum continuous, PW127G 35 C for every rating; both take
        // the PW120 exponent. The TP400-D6 has its own derived corner.
        for (name, takeoff_k, continuous_k) in
            [("PW127M", 312.15, 321.15), ("PW127G", 308.15, 308.15)]
        {
            let prop = get(name).unwrap().turboprop.clone().unwrap();
            assert_eq!(prop.takeoff_flat_rating_temperature_k, takeoff_k, "{name}");
            assert_eq!(
                prop.maximum_continuous_flat_rating_temperature_k, continuous_k,
                "{name}"
            );
            assert_eq!(prop.power_lapse_density_exponent, 0.728, "{name}");
        }
        let tp400 = get("TP400-D6").unwrap().turboprop.clone().unwrap();
        let corner = 288.15 * (9_694.0_f64 / 7_971.0).powf(1.0 / 0.728);
        assert!((tp400.takeoff_flat_rating_temperature_k - corner).abs() < 0.01);
        assert_eq!(
            tp400.maximum_continuous_flat_rating_temperature_k,
            tp400.takeoff_flat_rating_temperature_k
        );
        assert!(tp400.power_lapse_source.contains("NO TP400 SOURCE"));
        // Every record's exponent is inside the class band it cites:
        // Torenbeek (1982) eq. (5-70), p. 165, n between .7 and .8 for a
        // given rating and Mach number.
        for name in ["PW127M", "PW127G", "TP400-D6"] {
            let prop = get(name).unwrap().turboprop.clone().unwrap();
            assert!(prop.power_lapse_source.contains("eq. (5-70), p. 165"), "{name}");
            assert!((0.7..=0.8).contains(&prop.power_lapse_density_exponent), "{name}");
        }
    }

    #[test]
    fn an_aircraft_installed_rating_is_the_thrust_and_the_certified_one_the_mass_basis() {
        // Rating basis: the installed take-off rating the aircraft's own
        // document prints, when it prints one; the engine certification
        // rating of the same hardware is the mass basis. COMAC C919 ACAP:
        // C919 STD 28468 lb; ICAO EEDB LEAP-1C28 129.976 kN.
        let leap = get("LEAP-1C").unwrap();
        let installed_kn = 28_468.0 * 4.448_221_615_260_5 / 1_000.0;
        assert!((leap.turbofan_spec().unwrap().rated_thrust_kn - installed_kn).abs() < 0.01);
        assert_eq!(leap.mass_basis_thrust_kn, Some(129.976));
        assert!(leap.thrust_source.contains("28468 lb"));
        // The LTO table is the databank's, re-read at the installed rating:
        // the take-off fuel flow falls with the rating, between the 85 % and
        // 100 % databank points, and the 100 % ratio is 1.
        let spec = leap.turbofan_spec().unwrap();
        let scale = leap.thrust_kn / 129.976;
        let r85 = leap.part_power_fuel_flow_ratios[2];
        let expected_full = r85 + (scale - 0.85) / 0.15 * (1.0 - r85);
        assert!(
            (spec.takeoff_fuel_flow_kg_s - leap.takeoff_fuel_flow_kg_s * expected_full).abs()
                < 1e-12
        );
        assert!((spec.part_power_fuel_flow_ratios[3] - 1.0).abs() < 1e-12);
        assert!(leap.aircraft_installed_rating);
        // A record whose thrust is its databank rating keeps the table as is.
        let cf6 = get("CF6-50").unwrap();
        assert_eq!(
            cf6.turbofan_spec().unwrap().part_power_fuel_flow_ratios,
            cf6.part_power_fuel_flow_ratios
        );
        // The family maximum-climb datum (the LEAP-1A26 FN/FN*, 23,600 N over
        // 120.636 kN) is carried to the installed rating, not the certified one.
        let off_design = &leap.turbofan_spec().unwrap().off_design;
        let carried_n = 23_600.0 / 120_636.0 * installed_kn * 1_000.0;
        assert!((off_design.cruise_reference_thrust_n - carried_n).abs() < 1.0);
    }

    /// The two statements stay apart: a mass basis alone never changes the
    /// flown rating, so a record that declares one without the installed
    /// rating flag must carry its databank rating as `thrust_kn`, and a
    /// flagged record must state the installed rating's evidence.
    #[test]
    fn a_mass_basis_never_stands_in_for_an_installed_rating() {
        let raw: serde_json::Value = serde_json::from_str(ENGINES_JSON).unwrap();
        for record in raw["engines"].as_array().unwrap() {
            let name = record["name"].as_str().unwrap();
            let spec = get(name).unwrap();
            if spec.aircraft_installed_rating {
                assert!(
                    spec.thrust_rating_condition.contains("aircraft installed"),
                    "{name}"
                );
                continue;
            }
            if spec.mass_basis_thrust_kn.is_some() {
                if let Some(lto_kn) = spec.lto_rated_thrust_kn {
                    assert_eq!(spec.thrust_kn, lto_kn, "{name}");
                }
            }
        }
        // The C919's LEAP-1C and the 747-400's CF6-80C2B1F carry an installed
        // rating that differs from their databank row.
        let flagged: Vec<&str> = raw["engines"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r.get("aircraft_installed_rating") == Some(&serde_json::Value::Bool(true)))
            .map(|r| r["name"].as_str().unwrap())
            .collect();
        assert_eq!(flagged, ["LEAP-1C", "CF6-80C2"]);
    }

    /// The deck of an identity-qualified record flies its databank row's
    /// rating, so the record's own `thrust_kn` must be that rating unless it
    /// is flagged as the aircraft's installed rating; otherwise the
    /// catalogue states two take-off thrusts for one engine. The tolerance
    /// is the rounding of a rating printed to 0.01 kN or converted from lbf
    /// or daN by a type certificate (PW1521G 97.73 against 97.72).
    #[test]
    fn an_identity_qualified_record_states_the_rating_its_deck_flies() {
        for spec in database() {
            let qualified = matches!(
                spec.part_power_evidence.as_str(),
                "direct" | "direct-rating-match" | "rating-proxy" | "family-match"
            );
            if !qualified || spec.aircraft_installed_rating {
                continue;
            }
            let Some(lto_kn) = spec.lto_rated_thrust_kn else {
                continue;
            };
            assert!(
                (spec.thrust_kn - lto_kn).abs() <= 0.015,
                "{}: thrust_kn {} against LTO {}",
                spec.name,
                spec.thrust_kn,
                lto_kn
            );
            let typed = spec.turbofan_spec().unwrap();
            assert_eq!(typed.rated_thrust_kn, lto_kn, "{}", spec.name);
        }
    }

    #[test]
    fn every_turboprop_record_states_the_psfc_it_burns() {
        // The PW100 records keep the PW120A flight-record PSFC (the C295 and
        // ATR 72 unchanged); the TP400-D6 burns its own cruise figure.
        for name in ["PW127M", "PW127G"] {
            let prop = get(name).unwrap().turboprop.clone().unwrap();
            assert_eq!(prop.psfc_reference_kg_kwh, 0.2945, "{name}");
            assert_eq!(prop.psfc_reference_temperature_k, 249.65, "{name}");
        }
        let tp400 = get("TP400-D6").unwrap().turboprop.clone().unwrap();
        // Jane's 2008-2009: 9,694 kW thermodynamic power; the PW100 records
        // state none and keep their TCDS flat-rating temperatures.
        assert_eq!(tp400.thermodynamic_shaft_power_kw, 9_694.0);
        for name in ["PW127M", "PW127G"] {
            let prop = get(name).unwrap().turboprop.clone().unwrap();
            assert_eq!(prop.thermodynamic_shaft_power_kw, 0.0, "{name}");
        }
        // 0.35 lbm/(eshp h) in kg/(kW h).
        let expected = 0.35 * 0.453_592_37 / 0.745_699_872;
        assert!((tp400.psfc_reference_kg_kwh - expected).abs() < 5e-5);
        assert_eq!(tp400.psfc_reference_temperature_k, 216.65);
        assert!(tp400.psfc_source.contains("0.35 lbm/eshp-hr"));
    }

    #[test]
    fn only_the_static_corner_turboprop_takes_a_ram_factor() {
        // Nita (2008) eq. 3.5.11 is a fit at 275 kt true airspeed: the PW100
        // records keep it whole, with no ram factor, and the TP400-D6 takes
        // a ram factor read from the PW120 chart, relative to that schedule.
        let reference_m_s = 275.0 * 1_852.0 / 3_600.0;
        for name in ["PW127M", "PW127G", "TP400-D6"] {
            let prop = get(name).unwrap().turboprop.clone().unwrap();
            // TP400-D6: Nita (2008) Fig. 3.6, PW120 maximum cruise power
            // against true airspeed, read as 3.45 +/- 0.15.
            let expected_exponent = if name == "TP400-D6" { 3.45 } else { 0.0 };
            assert_eq!(prop.ram_power_exponent, expected_exponent, "{name}");
            assert!(
                (prop.power_lapse_fit_reference_true_airspeed_m_s - reference_m_s).abs() < 1e-3,
                "{name}"
            );
            assert!(prop.ram_power_source.contains("275 kt"), "{name}");
        }
    }

    #[test]
    fn the_tp400_ratings_are_bounded_at_the_tcds_rating_temperature() {
        // EASA TCDS E.033 states every TP400-D6 rating at sea level ISA and
        // prints no flat-rating temperature; the PW127 records carry their
        // own sea-level temperature corners and take no extra bound.
        let tp400 = get("TP400-D6").unwrap().turboprop.clone().unwrap();
        assert_eq!(tp400.rating_temperature_limit_k, 288.15);
        assert!(tp400.power_lapse_source.contains("288.15 K"));
        for name in ["PW127M", "PW127G"] {
            let prop = get(name).unwrap().turboprop.clone().unwrap();
            assert_eq!(prop.rating_temperature_limit_k, 0.0, "{name}");
        }
    }
}
