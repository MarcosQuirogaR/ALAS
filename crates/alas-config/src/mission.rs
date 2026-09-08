// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/mission_config.py
// Reference: alas @ rust-port-baseline.

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

mod profile;

pub use profile::{
    resolve_true_airspeed_m_s, MissionProfileConfig, SpeedReference, CAS_SPEED_SUBDIVISIONS,
};

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// Top-level mission-analysis settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct MissionConfig {
    /// Whether a normal run includes the mission analysis.
    #[config(
        help = "Fly the design along the configured route as part of a normal Run, producing block fuel and the trajectory figures. On by default: a design that has not been flown has only been evaluated at a single cruise point."
    )]
    pub enabled: bool,

    /// How long the mission analysis may take before it is abandoned.
    #[config(
        help = "Time limit for one mission analysis. A mission that has not converged by here is reported as such rather than left running."
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
        help = "Number of points a great-circle route is sampled at when no airway or dispatched routing is available. Higher is smoother on the globe and costs nothing else."
    )]
    pub great_circle_points: i64,

    /// Maximum acceptable generated airway distance divided by great-circle distance.
    #[serde(default = "default_max_airway_stretch")]
    #[config(
        label = "Maximum airway route stretch",
        help = "Reject generated airway detours above this ratio to great-circle distance and visibly use the great-circle approximation. Default 1.20 is a conceptual-model quality threshold, not a clearance constraint. Imported and dispatched plans are exempt. Set 0 to retain legacy unfiltered airway routing."
    )]
    pub max_airway_stretch: f64,

    /// Use endpoint coordinates in legacy airway records, including radio navaids.
    #[serde(default = "default_airway_endpoint_coordinates")]
    #[config(
        label = "Use airway endpoint coordinates",
        help = "Read complete endpoints from coordinate-bearing airway data, including navaids absent from the fix catalog. Turn off with maximum airway stretch 0 to reproduce legacy routing."
    )]
    pub use_airway_endpoint_coordinates: bool,

    /// Which dispatch account to read a filed flight plan from.
    #[config(
        hidden,
        help = "Dispatch account name or pilot ID. When set, the routing tries this account's most recently generated flight plan -- real current-cycle departure, arrival and airway routing -- before falling back to imported or great-circle routing. Blank skips that tier. Set on Setup > External Tools."
    )]
    pub simbrief_username: String,

    /// How long to wait for a filed flight plan.
    #[config(
        hidden,
        help = "Time limit for fetching a filed flight plan before falling through to the other routing tiers. Set on Setup > External Tools."
    )]
    pub simbrief_timeout_s: f64,

    /// Whether a fetched flight plan's city pair wins over the selected one.
    #[config(
        label = "SimBrief overrides route airports",
        help = "When your most recent SimBrief OFP is for a different city pair than the departure/arrival airports selected above, fly the OFP's pair instead. A real dispatched OFP (real SID/STAR/airways, current AIRAC) is the most accurate route ALAS can get, so it takes precedence. Turn off to keep the manually-selected airports and ignore a mismatched OFP."
    )]
    pub simbrief_overrides_airports: bool,

    /// The speed and altitude profile the mission flies.
    #[config(
        nested,
        help = "The climb, cruise and descent speeds, rates and altitudes the mission flies."
    )]
    pub profile: MissionProfileConfig,
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
