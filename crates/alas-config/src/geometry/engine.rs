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
//! consumer -- mass estimation, the mission analysis, the matching chart, the
//! payload-range diagram, the cycle analysis -- to hold that name and look the
//! entry up when it needs a number. It would also mean an engine that has been
//! edited is edited for some of them and not others, since a look-up cannot
//! see an edit.
//!
//! So the name is a *selector*, not a reference: choosing one copies the
//! table's values into the fields below once, through
//! [`EngineConfig::apply_engine_spec`], and everything downstream reads those
//! fields. After that the design's engine can be modified freely -- a
//! hypothetical derivative, a re-rated variant -- and every discipline sees
//! the same modification, because there is only one copy of it.
//!
//! That is also why this group is hidden from the generated geometry form:
//! it has its own editor, and two forms writing the same fields is how the
//! two of them come to disagree.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// Podded engine placement, nacelle shape, and the live cycle parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct EngineConfig {
    /// Which table entry the fields below were last copied from.
    #[config(
        options = Engine,
        label = "Engine model",
        help = "Name from the built-in engine database (see the Engine selector on the Inputs tab) -- drives thrust, mass, and the default nacelle profile."
    )]
    pub engine_name: String,

    /// The nacelle silhouette, as station and radius-fraction pairs.
    #[config(
        advanced,
        columns = ["x-station [m]", "radius fraction [0-1]"],
        label = "Nacelle profile points",
        help = "List of (x-station [m], radius-fraction [0-1 of radius_scale_m below]) pairs tracing the nacelle's longitudinal silhouette from inlet (x=0) to exit -- see the Engine Designer tab's live nacelle-silhouette preview for a picture of the shape these points draw. Auto-filled from the engine database when engine_name is recognised."
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

    /// Sea-level static takeoff thrust, per engine.
    #[config(
        decimals = 2,
        label = "Rated thrust per engine",
        unit = "kN",
        help = "Maximum rated sea-level-static take-off thrust, per engine. Drives propulsion mass, the Matching Chart T/W lookup, and the mission turbofan sizing target."
    )]
    pub thrust_kn: f64,

    /// Bypass flow over core flow.
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
        help = "Combustor-exit stagnation temperature -- the primary driver of specific thrust and thermal efficiency in the on-design cycle."
    )]
    pub turbine_inlet_temp_k: f64,

    /// Published cruise fuel consumption, as the range diagram uses it.
    #[config(
        label = "Cruise TSFC (reference)",
        unit = "kg/(kgf.hr)",
        help = "Reference cruise thrust-specific fuel consumption, used by the Breguet payload-range diagram. Not necessarily identical to the Propulsion Analysis tab's on-design-cycle-computed TSFC (a fast conceptual cycle model with generic component efficiencies vs. this field's real/published in-service figure) -- see that tab's caption."
    )]
    pub cruise_tsfc_kg_kgf_hr: f64,

    /// Fan face diameter, for reference.
    #[config(
        advanced,
        label = "Fan diameter",
        unit = "m",
        help = "Fan face diameter -- informational/reference only (does not currently size the nacelle profile, which comes from radius_scale_m/nacelle_profile above)."
    )]
    pub fan_diameter_m: f64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        // The GE9X entry of the engine table, copied out. A bare
        // `EngineConfig` is constructed before anything has had a chance to
        // call `apply_engine_spec`, and it has to describe a real engine by
        // the time something asks it for thrust.
        Self {
            engine_name: "GE9X".to_owned(),
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
            thrust_kn: 467.0,
            bypass_ratio: 10.0,
            overall_pressure_ratio: 60.0,
            fan_pressure_ratio: 1.45,
            turbine_inlet_temp_k: 1670.0,
            cruise_tsfc_kg_kgf_hr: 0.50,
            fan_diameter_m: 3.40,
        }
    }
}

impl EngineConfig {
    /// Copy the entry named by [`Self::engine_name`] into every field it
    /// covers.
    ///
    /// This is the only place the engine table is read once a design is
    /// running; see the module documentation for why. A name the table does
    /// not carry leaves the current values alone, which is what lets an
    /// engine be given a name of its own and still be flown -- upstream does
    /// the same by swallowing the lookup failure, and this records it instead
    /// of discarding it.
    pub fn apply_engine_spec(&mut self) {
        let spec = match crate::engines::get(&self.engine_name) {
            Ok(spec) => spec,
            Err(error) => {
                tracing::debug!(
                    engine = %self.engine_name,
                    %error,
                    "engine not in the table; keeping the values already set"
                );
                return;
            }
        };

        self.nacelle_profile = spec.nacelle_profile();
        self.radius_scale_m = spec.nacelle_max_radius_m;
        self.thrust_kn = spec.thrust_kn;
        self.bypass_ratio = spec.bypass_ratio;
        self.overall_pressure_ratio = spec.overall_pressure_ratio;
        self.fan_pressure_ratio = spec.fan_pressure_ratio;
        self.turbine_inlet_temp_k = spec.turbine_inlet_temp_k;
        self.cruise_tsfc_kg_kgf_hr = spec.cruise_tsfc_kg_kgf_hr;
        self.fan_diameter_m = spec.fan_diameter_m;
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
        assert_eq!(applied.thrust_kn, bare.thrust_kn);
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
        // and no station off by as much as a decimetre -- so the two draw the
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
        assert_eq!(engine.thrust_kn, spec.thrust_kn);
        assert_eq!(engine.bypass_ratio, spec.bypass_ratio);
        assert_eq!(engine.fan_diameter_m, spec.fan_diameter_m);
        assert_eq!(engine.radius_scale_m, spec.nacelle_max_radius_m);
    }

    #[test]
    fn an_engine_the_table_does_not_carry_keeps_the_values_already_set() {
        // A re-rated or hypothetical engine is named, edited and flown; a
        // lookup failure must not silently revert it to something published.
        let mut engine = EngineConfig {
            engine_name: "GE9X derivative".to_owned(),
            thrust_kn: 480.0,
            ..Default::default()
        };
        engine.apply_engine_spec();
        assert_eq!(engine.thrust_kn, 480.0);
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
