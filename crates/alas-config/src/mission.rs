// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/mission_config.py

//! Whether a run flies its mission, and how its native mission is configured.
//!
//! The mission analysis integrates the design along a real trajectory rather
//! than evaluating it at a single cruise point, which is what turns a lift-to-
//! drag ratio into a block fuel figure. It is on by default because a design
//! that has not been flown has not really been evaluated.
//!
//! The speed and altitude profile it flies is [`MissionProfileConfig`], and
//! it is configuration rather than a constant in the mission builder for the
//! same reason everything else here is: a fidelity assumption that cannot be
//! seen cannot be questioned.

mod plan;
mod profile;
mod speed_caps;

pub use plan::{
    CruiseExtent, EventAction, EventTime, MissionPlan, PlanError, PlanStage, RaymerProfile,
    StageEvent, StageKind, TakeoffField, TemplateInputs, CATEGORY_I_DECISION_HEIGHT_FT,
};
pub use profile::{
    resolve_true_airspeed_m_s, CruiseAltitudePolicy, MissionProfileConfig, SpeedReference,
    CAS_SPEED_SUBDIVISIONS, MIN_CRUISE_ROUTE_FRACTION,
};
pub use speed_caps::{
    crossover_altitude_m, pressure_altitude_m, LimitTable, SpeedCaps, LIMIT_TABLE_ROWS,
};

/// 14 CFR 91.117(a) (ICAO Annex 2 states the same limit): the indicated
/// airspeed below [`SPEED_RESTRICTION_ALTITUDE_M`], 250 kt, m/s.
pub const SPEED_RESTRICTION_CAS_M_S: f64 = 250.0 * 1852.0 / 3600.0;
/// Altitude below which that limit applies, 10,000 ft MSL, m.
pub const SPEED_RESTRICTION_ALTITUDE_M: f64 = 10_000.0 * 0.3048;

/// Height above the field below which the landing configuration may be flown, 3,000 ft: BADA 3.8
/// User Manual (EUROCONTROL, public document) `H_max,LD`, Sec. 5.6 (printed p. 35); the BADA
/// configuration thresholds are heights above ground level (printed p. 19).
pub const LANDING_CONFIGURATION_HEIGHT_M: f64 = 3_000.0 * 0.3048;

/// BADA 3 `a_l,max(civ)`: 2.0 ft/s^2, m/s^2, the largest longitudinal acceleration of a civil
/// aeroplane (any phase, either sense), which the priced and the native flights both respect on a
/// level speed change. BADA 3.8 User Manual (EUROCONTROL, public document) Sec. 5.2, printed p. 33,
/// eq. 5.2-1; the global parameter listing, printed p. 63, `acc_long_max` `.20000E+01`.
pub const LEVEL_ACCELERATION_LIMIT_M_S2: f64 = 2.0 * 0.3048;

/// Height above the field at which the take-off climb stops at the climb speed and starts to
/// accelerate to the climb-out speed, 1,000 ft: a modelling choice inside the range the departure
/// procedures leave open, not a published value.
///
/// What is verified on the page: FAA AC 91-53A (1993) para 6 gives two noise abatement departure
/// profiles, "close-in" and "distant"; both initiate the thrust cutback no lower than 800 ft
/// above field elevation, hold the all-engine target climb speed (never below V2) to 3,000 ft
/// above field elevation or until the en-route climb configuration, and the distant profile starts
/// the flaps/slats retraction (the acceleration) before the cutback. ICAO PANS-OPS Doc 8168
/// Vol. I NADP 1 and 2 were not read (the document is paywalled and not in the project library):
/// the usual reading, NADP 1 accelerating at 3,000 ft and NADP 2 from 800 ft, is not relied on
/// here. 1,000 ft sits 200 ft above the 800 ft minimum of the FAA text.
pub const TAKEOFF_ACCELERATION_HEIGHT_M: f64 = 1_000.0 * 0.3048;

/// Length of the take-off band left at its own speed after the take-off acceleration ends, 50 ft,
/// m: a piece shorter than the junction logic's minimum is not cut off. The native flight and the
/// priced route model both keep the band this much above the end of the acceleration.
pub const TAKEOFF_BAND_STRETCH_M: f64 = 50.0 * 0.3048;

/// Speed differences below this are not worth a leg, m/s (about 1-2 % of an airspeed: the
/// tolerance within which two schedules are taken to agree). The native junction logic and the
/// priced route model both cut a speed change at a junction only above it.
pub const MINIMUM_SPEED_CHANGE_M_S: f64 = 3.0;

/// Smallest vertical rate, m/s, at which a climb or descent still carries a speed change: 100
/// ft/min, the rate of climb that defines the service ceiling, "the highest altitude at which the
/// aircraft can climb with a rate of 100 ft/min" (Sadraey, Aircraft Design: A Systems
/// Engineering Approach, 2013, Sec. 4.3.6, p 140). A change the engine can only fly more slowly
/// than this is not a climb or descent but a level change, and a climb is not continued below it.
pub const MINIMUM_PATH_CHANGE_RATE_M_S: f64 = 100.0 * 0.3048 / 60.0;

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// Top-level mission-analysis settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct MissionConfig {
    /// Whether this run includes the mission analysis.
    ///
    /// Every normal full run flies the mission: the loading boundary forces
    /// this to `true` for any document, so a saved file cannot switch it off
    /// (see [`crate::ConfigLoadNotes`]). Library and command-line callers may
    /// clear it in memory (`--no-mission`) for diagnostics.
    #[config(
        advanced,
        help = "Fly the design along the configured route in a normal Run, producing block fuel and trajectory figures.",
        long_help = "Fly the design along the configured route as part of a normal Run, producing block fuel and the trajectory figures. On by default: a design that has not been flown has only been evaluated at a single cruise point."
    )]
    pub enabled: bool,

    /// Retained for configuration files that carry it. No mission stage reads
    /// it: a mission run is bounded by its segment-solve cap, not by a clock.
    #[config(
        advanced,
        hidden,
        help = "Not used: the mission analysis is not stopped by a time limit. Kept so existing configuration files still load.",
        long_help = "Not used. The mission analysis is bounded by its solver iteration limits and not by wall-clock time, so changing this value has no effect. The key is kept so existing configuration files still load."
    )]
    pub timeout_s: f64,

    /// Where the navigation data lives.
    #[config(
        help = "Directory holding the navigation data the airway router reads. Downloaded on demand rather than distributed with the program."
    )]
    pub navdata_dir: String,

    /// Where the globe texture lives.
    #[config(
        help = "Image the route globe is drawn on. Downloaded on demand rather than distributed with the program."
    )]
    pub texture_path: String,

    /// Where saved routes live.
    #[config(help = "Directory holding saved and imported route files.")]
    pub routes_dir: String,

    /// How finely a great-circle fallback route is sampled.
    #[config(
        advanced,
        help = "Number of points a great-circle route is sampled at when no airway or dispatched routing is available. Higher is smoother on the globe and costs nothing else."
    )]
    pub great_circle_points: i64,

    /// Maximum acceptable generated airway distance divided by great-circle distance.
    #[serde(default = "default_max_airway_stretch")]
    #[config(
        advanced,
        label = "Maximum airway route stretch",
        help = "Reject generated airway detours above this ratio to great-circle distance (dimensionless) and use the great circle instead.",
        long_help = "Reject generated airway detours above this ratio to great-circle distance and visibly use the great-circle approximation. Default 1.20 is a conceptual-model quality threshold, not a clearance constraint. Imported and dispatched plans are exempt. Set 0 to retain legacy unfiltered airway routing."
    )]
    pub max_airway_stretch: f64,

    /// Use endpoint coordinates in legacy airway records, including radio navaids.
    #[serde(default = "default_airway_endpoint_coordinates")]
    #[config(
        advanced,
        label = "Use airway endpoint coordinates",
        help = "Read complete endpoints from coordinate-bearing airway data, including navaids absent from the fix catalog. Turn off with maximum airway stretch 0 to reproduce legacy routing."
    )]
    pub use_airway_endpoint_coordinates: bool,

    /// Which dispatch account to read a filed flight plan from.
    #[config(
        advanced,
        hidden,
        help = "Dispatch account name or pilot ID. When set, routing tries this account's latest flight plan before imported or great-circle routing. Blank skips it.",
        long_help = "Dispatch account name or pilot ID. When set, the routing tries this account's most recently generated flight plan (real current-cycle departure, arrival and airway routing) before falling back to imported or great-circle routing. Blank skips that tier. Set on Workspace > External Tools."
    )]
    pub simbrief_username: String,

    /// How long to wait for a filed flight plan.
    #[config(
        advanced,
        hidden,
        help = "Time limit for fetching a filed flight plan before falling through to the other routing tiers. Set on Workspace > External Tools."
    )]
    pub simbrief_timeout_s: f64,

    /// Whether a fetched flight plan's city pair wins over the selected one.
    #[config(
        advanced,
        label = "SimBrief overrides route airports",
        help = "If your latest SimBrief OFP is for a different city pair than the airports selected above, fly the OFP's pair instead (the most accurate route available).",
        long_help = "When your most recent SimBrief OFP is for a different city pair than the departure/arrival airports selected above, fly the OFP's pair instead. A real dispatched OFP (real SID/STAR/airways, current AIRAC) is the most accurate route ALAS can get, so it takes precedence. Turn off to keep the manually-selected airports and ignore a mismatched OFP."
    )]
    pub simbrief_overrides_airports: bool,

    /// The speed and altitude profile the mission flies.
    #[config(
        nested,
        help = "The climb, cruise and descent speeds, rates and altitudes the mission flies."
    )]
    pub profile: MissionProfileConfig,

    /// The user's stage list; empty flies the standard mission.
    #[serde(default, skip_serializing_if = "MissionPlan::is_empty")]
    #[config(
        hidden,
        help = "The mission as a list of stages (take-off, climbs, cruises, holds, descents, diversions, landing) with payload and fuel events. Empty flies the standard mission.",
        long_help = "The mission as a list of stages (take-off, climbs, cruises, holds, descents, go-arounds, diversions, landing) with payload-release, store, refuelling and fuel-transfer events. Empty flies the standard mission."
    )]
    pub plan: MissionPlan,

    /// Still-air distance of the route planned for this run, m; zero when
    /// none was planned.
    ///
    /// A run plans its route once, before the search, and writes the
    /// planned distance here so the search flies the route the published
    /// mission and the delivery check fly, rather than the great circle.
    /// The sizing loop reads it in place of the great-circle distance
    /// between the configured airports; a declared design range still sizes
    /// a mission-closed aircraft, which then flies this route off-design.
    #[serde(default, skip_serializing_if = "is_unset_distance")]
    #[config(
        hidden,
        help = "Still-air distance of the route this run planned [m].",
        long_help = "Still-air distance of the route this run planned, in metres. Written by the run itself so the optimizer flies the same route as the reported mission; zero means the great circle between the selected airports."
    )]
    pub route_distance_m: f64,
}

/// Whether no planned route distance is recorded.
#[allow(clippy::trivially_copy_pass_by_ref)] // serde passes the field by reference
fn is_unset_distance(distance_m: &f64) -> bool {
    *distance_m == 0.0
}

impl MissionConfig {
    /// The planned route's still-air distance, m, when one is recorded.
    #[must_use]
    pub fn planned_route_distance_m(&self) -> Option<f64> {
        (self.route_distance_m.is_finite() && self.route_distance_m > 0.0)
            .then_some(self.route_distance_m)
    }
}

impl Default for MissionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            timeout_s: 900.0,
            navdata_dir: "alas/data/navdata".to_owned(),
            texture_path: "alas/data/textures/earth_blue_marble.jpg".to_owned(),
            routes_dir: "alas/data/routes".to_owned(),
            great_circle_points: 50,
            max_airway_stretch: default_max_airway_stretch(),
            use_airway_endpoint_coordinates: true,
            simbrief_username: String::new(),
            simbrief_timeout_s: 15.0,
            simbrief_overrides_airports: true,
            profile: MissionProfileConfig::default(),
            plan: MissionPlan::default(),
            route_distance_m: 0.0,
        }
    }
}

// Teoh et al., Atmospheric Chemistry and Physics 24 (2024), 725-744,
// doi:10.5194/acp-24-725-2024: mean whole-flight extension is 5.2%.
// 20% is a conservative engineering rejection threshold, not a fitted percentile.
fn default_max_airway_stretch() -> f64 {
    1.20
}

fn default_airway_endpoint_coordinates() -> bool {
    true
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Entry;

    #[test]
    fn the_profile_reaches_the_form_as_a_group_of_its_own() {
        let schema = MissionConfig::default().schema();
        match &schema.field("profile").unwrap().entry {
            Entry::Node(node) => {
                assert_eq!(node.type_name, "MissionProfileConfig");
                assert!(node.field("takeoff_air_speed_m_s").is_some());
            }
            Entry::Leaf(_) => panic!("the profile is a group, not a value"),
        }
    }

    #[test]
    fn a_partial_overlay_leaves_the_rest_of_the_profile_alone() {
        let base = MissionConfig::default();
        let changed = crate::overlay(
            &base,
            &serde_json::json!({"profile": {"takeoff_climb_rate_m_s": 8.0}}),
        )
        .unwrap();
        assert_eq!(changed.profile.takeoff_climb_rate_m_s, 8.0);
        assert_eq!(
            changed.profile.takeoff_air_speed_m_s,
            base.profile.takeoff_air_speed_m_s
        );
    }
}
