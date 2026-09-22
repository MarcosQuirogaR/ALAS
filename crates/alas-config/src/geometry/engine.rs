// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/geometry_config.py (`EngineConfig`)
// Reference: alas @ rust-port-baseline.

//! The engine: where it hangs, what shape its nacelle is, and what cycle it
//! runs.
//!
//! # Why this holds the cycle data rather than a database key
//!
//! [`crate::engines`] is an immutable table of published engines, and
//! [`EngineConfig::engine_name`] names one. It would be shorter for every
//! consumer: mass estimation, the mission analysis, the matching chart, the
//! payload-range diagram, the cycle analysis, to hold that name and look the
//! entry up when it needs a number. It would also mean an engine that has been
//! edited is edited for some of them and not others, since a look-up cannot
//! see an edit.
//!
//! So the name is a *selector*, not a reference: choosing one copies the
//! table's values and tagged physics payload into the fields below once, through
//! [`EngineConfig::apply_engine_spec`], and everything downstream reads those
//! fields. After that the design's engine can be modified freely: a
//! hypothetical derivative, a re-rated variant, and every discipline sees
//! the same modification, because there is only one copy of it.
//!
//! That is also why this group is hidden from the generated geometry form:
//! it has its own editor, and two forms writing the same fields is how the
//! two of them come to disagree.

use serde::{Deserialize, Serialize};

use crate::engines::{PropulsionTechnology, TurbofanEngineSpec, TurbopropEngineSpec};
use crate::ConfigNode;

#[path = "engine_wire.rs"]
mod engine_wire;

fn is_default_propulsion_technology(value: &PropulsionTechnology) -> bool {
    *value == PropulsionTechnology::default()
}

/// Why a selected engine cannot be exposed to a propulsion solver.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EngineBindingError {
    /// The selector is not a canonical catalogue name or alias.
    #[error(transparent)]
    Unknown(#[from] crate::engines::UnknownEngine),
    /// Selector, technology tag and typed payload do not describe one model.
    #[error("engine '{engine_name}' has no coherent {technology:?} physics binding")]
    MismatchedBinding {
        /// Selected catalogue key.
        engine_name: String,
        /// Technology claimed by the live configuration.
        technology: PropulsionTechnology,
    },
    /// The active technology payload contains a nonphysical value.
    #[error("engine '{engine_name}' has invalid physics: {reason}")]
    InvalidPayload {
        /// Selected catalogue key.
        engine_name: String,
        /// Stable, actionable validation explanation.
        reason: &'static str,
    },
}

/// A validated, technology-specific view of the live engine configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActiveEngineModel<'a> {
    /// A thrust-producing gas turbine.
    Turbofan(&'a TurbofanEngineSpec),
    /// A shaft-power gas turbine and propeller installation.
    Turboprop(&'a TurbopropEngineSpec),
}

/// Podded engine placement, nacelle shape, and the live cycle parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(
    try_from = "engine_wire::EngineConfigWire",
    into = "engine_wire::EngineConfigWire"
)]
pub struct EngineConfig {
    /// Which table entry the fields below were last copied from.
    #[config(
        options = Engine,
        label = "Engine model",
        help = "Name from the built-in engine database (see the Engine selector on the Inputs tab): drives thrust, mass, and the default nacelle profile."
    )]
    pub engine_name: String,

    /// Technology discriminator used by the propulsion orchestrator. Hidden
    /// until the dedicated propulsion editor can present tagged payloads.
    #[serde(default, skip_serializing_if = "is_default_propulsion_technology")]
    #[config(skip)]
    pub propulsion_technology: PropulsionTechnology,

    /// Typed turbofan physics payload. The flat fields below remain a
    /// deprecated compatibility mirror for existing consumers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(skip)]
    pub turbofan: Option<TurbofanEngineSpec>,

    /// Typed turboprop physics payload; never inferred from turbofan fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(skip)]
    pub turboprop: Option<TurbopropEngineSpec>,

    /// The nacelle silhouette, as station and radius-fraction pairs.
    #[config(
        advanced,
        columns = ["x-station [m]", "radius fraction [0-1]"],
        label = "Nacelle profile points",
        help = "List of (x-station [m], radius-fraction [0-1 of radius_scale_m below]) pairs tracing the nacelle's longitudinal silhouette from inlet (x=0) to exit, see the Engine Designer tab's live nacelle-silhouette preview for a picture of the shape these points draw. Auto-filled from the engine database when engine_name is recognised."
    )]
    pub nacelle_profile: Vec<(f64, f64)>,

    /// The radius those fractions are of.
    #[config(
        advanced,
        label = "Nacelle max radius",
        unit = "m",
        help = "Physical radius the nacelle_profile's fraction-of-1.0 points are scaled by."
    )]
    pub radius_scale_m: f64,

    /// Where each engine hangs along the span.
    #[config(
        advanced,
        label = "Engine spanwise (Y) positions",
        unit = "m",
        help = "Y-coordinate of each engine's mount position (negative = left/port side); one entry per engine."
    )]
    pub spanwise_positions_m: Vec<f64>,

    /// How far below the wing reference plane the centerline sits.
    #[config(
        advanced,
        label = "Engine vertical offset",
        unit = "m",
        help = "Vertical placement of the engine centerline relative to the wing reference plane (negative = below the wing)."
    )]
    pub z_m: f64,

    /// How far ahead of the wing leading edge the inlet face sits.
    #[config(
        advanced,
        label = "Inlet offset ahead of wing LE",
        unit = "m",
        help = "How far forward of the (swept) wing leading edge the nacelle inlet face sits."
    )]
    pub inlet_x_offset_m: f64,

    /// Deprecated turbofan compatibility mirror: bypass flow over core flow.
    #[config(
        label = "Bypass ratio (BPR)",
        unit = "-",
        help = "Ratio of bypass (fan duct) to core mass flow. Feeds the mission turbofan network and the Propulsion Analysis on-design cycle."
    )]
    pub bypass_ratio: f64,

    /// Total pressure ratio through the core compressors.
    #[config(
        advanced,
        label = "Overall (core) pressure ratio (OPR)",
        unit = "-",
        help = "Total pressure ratio through the core compressors (LPC x HPC combined, NOT including the fan). Feeds mission compressor sizing (split into a fixed LPC ratio + a solved HPC ratio) and the Propulsion Analysis cycle's compressor_pressure_ratio."
    )]
    pub overall_pressure_ratio: f64,

    /// Pressure ratio across the fan.
    #[config(
        advanced,
        label = "Fan pressure ratio (FPR)",
        unit = "-",
        help = "Pressure ratio across the fan (bypass stream), separate from the core OPR above."
    )]
    pub fan_pressure_ratio: f64,

    /// Combustor-exit stagnation temperature.
    #[config(
        advanced,
        label = "Turbine inlet temperature (T4t)",
        unit = "K",
        help = "Combustor-exit stagnation temperature: the primary driver of specific thrust and thermal efficiency in the on-design cycle."
    )]
    pub turbine_inlet_temp_k: f64,

    /// Published cruise fuel consumption, as the range diagram uses it.
    #[config(
        label = "Cruise TSFC (reference)",
        unit = "kg/(kgf.hr)",
        help = "Reference cruise thrust-specific fuel consumption, used by the Breguet payload-range diagram. Not necessarily identical to the Propulsion Analysis tab's on-design-cycle-computed TSFC (a fast conceptual cycle model with generic component efficiencies vs. this field's real/published in-service figure), see that tab's caption."
    )]
    pub cruise_tsfc_kg_kgf_hr: f64,

    /// Fan face diameter, for reference.
    #[config(
        advanced,
        label = "Fan diameter",
        unit = "m",
        help = "Fan face diameter: informational/reference only (does not currently size the nacelle profile, which comes from radius_scale_m/nacelle_profile above)."
    )]
    pub fan_diameter_m: f64,

    /// Normalized fuel flow at ICAO LTO thrust fractions 7%, 30%, 85%, 100%.
    #[config(
        advanced,
        label = "Part-power fuel-flow ratios",
        help = "Fuel flow divided by take-off fuel flow at 7%, 30%, 85%, and 100% rated net thrust. Sea-level-static ICAO/EASA anchors; altitude use is an explicitly empirical Level-1 approximation."
    )]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub part_power_fuel_flow_ratios: Vec<f64>,

    /// Provenance for the part-power schedule.
    #[config(
        advanced,
        label = "Part-power schedule source",
        help = "ICAO Engine Emissions Databank UID/variant, or a clearly identified family proxy when an exact entry is unavailable."
    )]
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub part_power_source: String,
}

impl Default for EngineConfig {
    fn default() -> Self {
        // The GE9X entry of the engine table, copied out. A bare
        // `EngineConfig` is constructed before anything has had a chance to
        // call `apply_engine_spec`, and it has to describe a real engine by
        // the time something asks it for thrust.
        Self {
            engine_name: "GE9X".to_owned(),
            propulsion_technology: PropulsionTechnology::Turbofan,
            turbofan: crate::engines::get("GE9X")
                .ok()
                .and_then(crate::engines::EngineSpec::turbofan_spec),
            turboprop: None,
            nacelle_profile: vec![
                (0.0, 0.40),
                (0.6, 0.92),
                (1.2, 1.0),
                (4.3, 1.0),
                (5.9, 0.82),
                (7.8, 0.45),
            ],
            radius_scale_m: 2.1,
            spanwise_positions_m: vec![9.8, -9.8],
            z_m: -2.9,
            inlet_x_offset_m: 4.2,
            bypass_ratio: 10.0,
            overall_pressure_ratio: 60.0,
            fan_pressure_ratio: 1.45,
            turbine_inlet_temp_k: 1670.0,
            cruise_tsfc_kg_kgf_hr: 0.50,
            fan_diameter_m: 3.40,
            // GE9X is not present in the March 2026 EEDB release. Until a
            // manufacturer deck is available, retain an explicit GEnx family
            // proxy rather than pretending the old linear law is measured.
            part_power_fuel_flow_ratios: vec![0.082_578_70, 0.253_287_04, 0.815, 1.0],
            part_power_source: "ICAO EEDB 03/2026 family proxy: 07P27GE235 GEnx-1B74/75/P2"
                .to_owned(),
        }
    }
}

impl EngineConfig {
    /// Authoritative per-engine sea-level static thrust, kN. Turboprops are
    /// power-rated and return zero; incoherent bindings return NaN.
    pub fn thrust_kn(&self) -> f64 {
        match self.active_model() {
            Ok(ActiveEngineModel::Turbofan(payload)) => payload.rated_thrust_kn,
            Ok(ActiveEngineModel::Turboprop(_)) => 0.0,
            Err(_) => f64::NAN,
        }
    }

    /// Set the sole turbofan thrust rating after validating its domain.
    pub fn set_thrust_kn(&mut self, thrust_kn: f64) -> Result<(), EngineBindingError> {
        if !thrust_kn.is_finite() || thrust_kn <= 0.0 {
            return Err(EngineBindingError::InvalidPayload {
                engine_name: self.engine_name.clone(),
                reason: "rated thrust must be finite and positive",
            });
        }
        self.active_model()?;
        let payload =
            self.turbofan
                .as_mut()
                .ok_or_else(|| EngineBindingError::MismatchedBinding {
                    engine_name: self.engine_name.clone(),
                    technology: self.propulsion_technology,
                })?;
        payload.rated_thrust_kn = thrust_kn;
        Ok(())
    }

    /// Resolve a selected database engine only when the cycle fields still
    /// carry the built-in, unselected defaults.
    ///
    /// Preset documents created before engine provenance was persisted contain
    /// a preset-specific `engine_name` alongside the default GE9X cycle. This
    /// one-time compatibility bridge initializes those documents while
    /// preserving any explicit thrust/cycle edits made by a user.
    pub fn apply_engine_spec_if_uninitialized(&mut self) {
        let defaults = Self::default();
        let cycle_is_uninitialized = self.radius_scale_m == defaults.radius_scale_m
            && self.turbofan.as_ref().map(|p| p.rated_thrust_kn)
                == defaults.turbofan.as_ref().map(|p| p.rated_thrust_kn)
            && self.bypass_ratio == defaults.bypass_ratio
            && self.overall_pressure_ratio == defaults.overall_pressure_ratio
            && self.fan_pressure_ratio == defaults.fan_pressure_ratio
            && self.turbine_inlet_temp_k == defaults.turbine_inlet_temp_k
            && self.cruise_tsfc_kg_kgf_hr == defaults.cruise_tsfc_kg_kgf_hr
            && self.fan_diameter_m == defaults.fan_diameter_m
            && self.part_power_fuel_flow_ratios == defaults.part_power_fuel_flow_ratios
            && self.part_power_source == defaults.part_power_source
            && self.nacelle_profile == defaults.nacelle_profile;
        if cycle_is_uninitialized {
            self.apply_engine_spec();
        }
    }

    /// Copy the entry named by [`Self::engine_name`] into every field it
    /// covers.
    ///
    /// This is the only place the engine table is read once a design is
    /// running; see the module documentation for why. On an unknown name the
    /// flat compatibility values remain available to the editor, but the
    /// typed binding is invalidated and [`Self::active_model`] fails closed.
    pub fn apply_engine_spec(&mut self) {
        if let Err(error) = self.try_apply_engine_spec() {
            tracing::debug!(engine = %self.engine_name, %error, "engine binding invalidated");
        }
    }

    /// Atomically replace geometry, compatibility mirrors and typed physics.
    ///
    /// On lookup failure the previous typed binding is cleared, so an unknown
    /// selector can never continue to expose another engine's valid physics.
    pub fn try_apply_engine_spec(&mut self) -> Result<(), EngineBindingError> {
        let spec = match crate::engines::get(&self.engine_name) {
            Ok(spec) => spec,
            Err(error) => {
                self.turbofan = None;
                self.turboprop = None;
                return Err(error.into());
            }
        };

        let (turbofan, turboprop) = match spec.technology {
            PropulsionTechnology::Turbofan => (spec.turbofan_spec(), None),
            PropulsionTechnology::Turboprop => (None, spec.turboprop.clone()),
        };
        if turbofan.is_none() && turboprop.is_none() {
            self.turbofan = None;
            self.turboprop = None;
            return Err(EngineBindingError::MismatchedBinding {
                engine_name: self.engine_name.clone(),
                technology: spec.technology,
            });
        }

        let mut replacement = self.clone();
        replacement.nacelle_profile = spec.nacelle_profile();
        replacement.radius_scale_m = spec.nacelle_max_radius_m;
        replacement.bypass_ratio = spec.bypass_ratio;
        replacement.overall_pressure_ratio = spec.overall_pressure_ratio;
        replacement.fan_pressure_ratio = spec.fan_pressure_ratio;
        replacement.turbine_inlet_temp_k = spec.turbine_inlet_temp_k;
        replacement.cruise_tsfc_kg_kgf_hr = spec.cruise_tsfc_kg_kgf_hr;
        replacement.fan_diameter_m = spec.fan_diameter_m;
        replacement.part_power_fuel_flow_ratios = spec.part_power_fuel_flow_ratios.to_vec();
        replacement.part_power_source = spec.part_power_source.clone();
        replacement.propulsion_technology = spec.technology;
        replacement.turbofan = turbofan;
        replacement.turboprop = turboprop;
        *self = replacement;
        Ok(())
    }

    /// Return physics only when the selector, technology and typed payload
    /// still agree with the catalogue binding.
    pub fn active_model(&self) -> Result<ActiveEngineModel<'_>, EngineBindingError> {
        let spec = crate::engines::get(&self.engine_name)?;
        if spec.technology != self.propulsion_technology {
            return Err(EngineBindingError::MismatchedBinding {
                engine_name: self.engine_name.clone(),
                technology: self.propulsion_technology,
            });
        }
        match self.propulsion_technology {
            PropulsionTechnology::Turbofan if self.turboprop.is_none() => {
                let payload = self.turbofan.as_ref().ok_or_else(|| {
                    EngineBindingError::MismatchedBinding {
                        engine_name: self.engine_name.clone(),
                        technology: self.propulsion_technology,
                    }
                })?;
                let scalars = [
                    payload.rated_thrust_kn,
                    payload.bypass_ratio,
                    payload.overall_pressure_ratio,
                    payload.fan_pressure_ratio,
                    payload.turbine_inlet_temp_k,
                    payload.cruise_tsfc_kg_kgf_hr,
                    payload.takeoff_fuel_flow_kg_s,
                    payload.off_design.cruise_reference_thrust_n,
                    payload.off_design.cruise_reference_altitude_m,
                    payload.off_design.cruise_reference_mach,
                ];
                if scalars.iter().any(|value| !value.is_finite())
                    || payload.rated_thrust_kn <= 0.0
                    || payload.bypass_ratio < 0.0
                    || payload
                        .takeoff_bypass_ratio
                        .is_some_and(|value| !value.is_finite() || value <= 0.0)
                    || payload.overall_pressure_ratio <= 1.0
                    || payload.fan_pressure_ratio <= 1.0
                    || payload.turbine_inlet_temp_k <= 0.0
                    || payload.cruise_tsfc_kg_kgf_hr <= 0.0
                    || payload.takeoff_fuel_flow_kg_s <= 0.0
                    || payload.off_design.cruise_reference_thrust_n <= 0.0
                    || payload.off_design.cruise_reference_altitude_m <= 9_144.0
                    || !(0.0..1.0).contains(&payload.off_design.cruise_reference_mach)
                    || !matches!(
                        payload.off_design.evidence.as_str(),
                        "direct-openap"
                            | "openap-static-fallback"
                            | "family-proxy"
                            | "aircraft-kinematic-calibration"
                            | "aircraft-requirement-calibration"
                    )
                    || payload.off_design.source.trim().is_empty()
                    || payload
                        .part_power_fuel_flow_ratios
                        .iter()
                        .any(|ratio| !ratio.is_finite() || *ratio < 0.0)
                {
                    return Err(EngineBindingError::InvalidPayload {
                        engine_name: self.engine_name.clone(),
                        reason: "turbofan ratings, ratios, temperatures and fuel-flow anchors must be finite and in their physical domains",
                    });
                }
                Ok(ActiveEngineModel::Turbofan(payload))
            }
            PropulsionTechnology::Turboprop if self.turbofan.is_none() => {
                let payload = self.turboprop.as_ref().ok_or_else(|| {
                    EngineBindingError::MismatchedBinding {
                        engine_name: self.engine_name.clone(),
                        technology: self.propulsion_technology,
                    }
                })?;
                let positive = [
                    payload.takeoff_shaft_power_kw,
                    payload.maximum_reserve_shaft_power_kw,
                    payload.maximum_continuous_shaft_power_kw,
                    payload.maximum_climb_shaft_power_kw,
                    payload.maximum_cruise_shaft_power_kw,
                    payload.maximum_cruise_fuel_flow_kg_h,
                    payload.propeller_diameter_m,
                    payload.governed_propeller_speed_rpm,
                    payload.reduction_ratio,
                ];
                if positive
                    .iter()
                    .any(|value| !value.is_finite() || *value <= 0.0)
                {
                    return Err(EngineBindingError::InvalidPayload {
                        engine_name: self.engine_name.clone(),
                        reason: "turboprop ratings, propeller geometry, governed speed and reduction ratio must be finite and positive",
                    });
                }
                Ok(ActiveEngineModel::Turboprop(payload))
            }
            _ => Err(EngineBindingError::MismatchedBinding {
                engine_name: self.engine_name.clone(),
                technology: self.propulsion_technology,
            }),
        }
    }

    /// Nacelle length, read off the aft-most profile station.
    ///
    /// The profile already fixes it exactly, so storing it separately would
    /// be storing the same number twice and inviting the two to disagree.
    /// An empty profile has no length, which is what a nacelle with no
    /// silhouette draws.
    pub fn nacelle_length_m(&self) -> f64 {
        self.nacelle_profile
            .last()
            .map_or(0.0, |&(station, _)| station)
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Entry, Kind};

    #[test]
    fn the_bare_default_already_carries_the_cycle_of_the_engine_it_names() {
        // Something asks for thrust before anything has selected an engine,
        // and a zero there is a design that cannot take off rather than an
        // obvious failure. So the fallback values are the named engine's,
        // written out.
        let mut applied = EngineConfig::default();
        applied.apply_engine_spec();

        let bare = EngineConfig::default();
        assert_eq!(applied.thrust_kn(), bare.thrust_kn());
        assert_eq!(applied.bypass_ratio, bare.bypass_ratio);
        assert_eq!(applied.overall_pressure_ratio, bare.overall_pressure_ratio);
        assert_eq!(applied.fan_pressure_ratio, bare.fan_pressure_ratio);
        assert_eq!(applied.turbine_inlet_temp_k, bare.turbine_inlet_temp_k);
        assert_eq!(applied.cruise_tsfc_kg_kgf_hr, bare.cruise_tsfc_kg_kgf_hr);
        assert_eq!(applied.fan_diameter_m, bare.fan_diameter_m);
        assert_eq!(applied.radius_scale_m, bare.radius_scale_m);
    }

    #[test]
    fn the_fallback_silhouette_is_the_same_shape_to_within_a_decimetre() {
        // The written-out fallback profile is not bitwise what selecting the
        // same engine produces: the table's stations are fractions of the
        // nacelle length (0.624 m, 1.17 m, ...) and the fallback rounds them
        // (0.6 m, 1.2 m, ...). Same overall length, same radius fractions,
        // and no station off by as much as a decimetre, so the two draw the
        // same nacelle, and this is a rounding rather than a transposed
        // digit. Recorded rather than corrected; see docs/PORTING.md.
        let mut applied = EngineConfig::default();
        applied.apply_engine_spec();
        let bare = EngineConfig::default();

        assert_eq!(applied.nacelle_length_m(), bare.nacelle_length_m());
        assert_eq!(applied.nacelle_profile.len(), bare.nacelle_profile.len());
        for (&(station, radius), &(rounded_station, rounded_radius)) in
            applied.nacelle_profile.iter().zip(&bare.nacelle_profile)
        {
            assert_eq!(radius, rounded_radius);
            assert!((station - rounded_station).abs() < 0.1, "{station} m");
        }
    }

    #[test]
    fn selecting_an_engine_replaces_every_field_the_table_covers() {
        let mut engine = EngineConfig {
            engine_name: "Trent 900".to_owned(),
            ..Default::default()
        };
        engine.apply_engine_spec();

        let spec = crate::engines::get("Trent 900").unwrap();
        assert_eq!(engine.thrust_kn(), spec.thrust_kn);
        assert_eq!(engine.bypass_ratio, spec.bypass_ratio);
        assert_eq!(engine.fan_diameter_m, spec.fan_diameter_m);
        assert_eq!(engine.radius_scale_m, spec.nacelle_max_radius_m);
    }

    #[test]
    fn an_unknown_engine_fails_closed_without_exposing_stale_thrust() {
        // An unknown selector cannot expose the previous engine's rating.
        let mut engine = EngineConfig {
            engine_name: "GE9X derivative".to_owned(),
            ..Default::default()
        };
        engine.apply_engine_spec();
        assert!(engine.thrust_kn().is_nan());
        assert!(engine.active_model().is_err());
    }

    #[test]
    fn pw127m_selection_exposes_only_turboprop_physics() {
        let mut engine = EngineConfig {
            engine_name: "PW127M".to_owned(),
            ..Default::default()
        };
        engine.try_apply_engine_spec().unwrap();
        assert!(matches!(
            engine.active_model(),
            Ok(ActiveEngineModel::Turboprop(_))
        ));
        assert!(engine.turbofan.is_none());
        assert_eq!(engine.thrust_kn(), 0.0);
    }

    #[test]
    fn changing_the_selector_cannot_expose_stale_ge9x_physics() {
        let mut engine = EngineConfig::default();
        assert!(matches!(
            engine.active_model(),
            Ok(ActiveEngineModel::Turbofan(_))
        ));
        engine.engine_name = "PW127M".to_owned();
        assert!(engine.active_model().is_err());
    }

    #[test]
    fn a_physically_valid_typed_derivative_remains_usable() {
        let mut engine = EngineConfig::default();
        engine.turbofan.as_mut().unwrap().rated_thrust_kn = 480.0;
        assert!(matches!(
            engine.active_model(),
            Ok(ActiveEngineModel::Turbofan(model)) if model.rated_thrust_kn == 480.0
        ));
    }

    #[test]
    fn typed_configs_saved_before_takeoff_bpr_split_remain_loadable() {
        let engine = EngineConfig::default();
        let mut value = serde_json::to_value(&engine).unwrap();
        value
            .pointer_mut("/turbofan")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("takeoff_bypass_ratio");
        let restored: EngineConfig = serde_json::from_value(value).unwrap();
        let Ok(ActiveEngineModel::Turbofan(spec)) = restored.active_model() else {
            panic!("legacy typed turbofan binding");
        };
        assert_eq!(spec.takeoff_bypass_ratio, None);
        assert!(spec.bypass_ratio > 0.0);
    }

    #[test]
    fn conditional_resolution_preserves_an_explicit_edit_to_a_known_engine() {
        let mut engine = EngineConfig {
            engine_name: "Trent 900".to_owned(),
            ..Default::default()
        };
        engine.turbofan.as_mut().unwrap().rated_thrust_kn = 401.0;
        engine.apply_engine_spec_if_uninitialized();
        assert_eq!(engine.thrust_kn(), 401.0);
    }

    #[test]
    fn conditional_resolution_initializes_a_name_only_engine_selection() {
        let mut engine = EngineConfig {
            engine_name: "Trent 900".to_owned(),
            ..Default::default()
        };
        engine.apply_engine_spec_if_uninitialized();
        assert_eq!(
            engine.thrust_kn(),
            crate::engines::get("Trent 900").unwrap().thrust_kn
        );
    }

    #[test]
    fn the_nacelle_length_is_the_last_station_of_its_own_silhouette() {
        let engine = EngineConfig::default();
        assert_eq!(engine.nacelle_length_m(), 7.8);
    }

    #[test]
    fn a_nacelle_with_no_silhouette_has_no_length_rather_than_panicking() {
        let engine = EngineConfig {
            nacelle_profile: Vec::new(),
            ..Default::default()
        };
        assert_eq!(engine.nacelle_length_m(), 0.0);
    }

    #[test]
    fn the_profile_reaches_the_form_as_a_two_column_table() {
        let schema = EngineConfig::default().schema();
        let Entry::Leaf(leaf) = &schema.field("nacelle_profile").unwrap().entry else {
            panic!("the profile is not a group");
        };
        assert_eq!(leaf.kind, Kind::TupleList);
        assert_eq!(
            leaf.columns,
            Some(&["x-station [m]", "radius fraction [0-1]"][..])
        );
    }

    #[test]
    fn the_default_engines_are_mounted_symmetrically_about_the_centerline() {
        let engine = EngineConfig::default();
        let total: f64 = engine.spanwise_positions_m.iter().sum();
        assert_eq!(total, 0.0);
    }
}
