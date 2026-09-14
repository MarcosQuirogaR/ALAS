// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/performance_config.py
// Reference: alas @ rust-port-baseline.

//! High-lift capability, field performance and the certified speed schedule.
//!
//! The matching chart turns requirements into a wing loading and a
//! thrust-to-weight ratio, and almost everything it needs is here: what lift
//! the high-lift system can produce, how much thrust is lost during the
//! ground roll, and the climb gradient certification demands with an engine
//! out. Defaults follow Raymer (*Aircraft Design: A Conceptual Approach*,
//! 5th ed.) and FAR Part 25.
//!
//! The V-speed factors are the certified schedule written as multiples of
//! stall speed, which is how the regulation itself states them: FAR 25.107
//! requires rotation at no less than 1.05 times the minimum control speed and
//! 1.10 times the stall speed, and 25.125 sets the approach speed at 1.30
//! times the landing stall speed. They are configuration rather than
//! constants so a known type can be reproduced exactly, which is the only way
//! to tell a modelling error from a calibration one.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// High-lift and field-performance parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct PerformanceConfig {
    /// Maximum lift coefficient in the takeoff configuration.
    #[config(
        label = "Max lift coefficient, take-off (CLmax_TO)",
        help = "Maximum lift coefficient achievable in the take-off flap/slat configuration. Drives take-off field length via the matching chart."
    )]
    pub cl_max_to: f64,

    /// Maximum lift coefficient in the landing configuration.
    #[config(
        label = "Max lift coefficient, landing (CLmax_L)",
        help = "Maximum lift coefficient achievable in the landing flap/slat configuration. Drives landing distance."
    )]
    pub cl_max_land: f64,

    /// Maximum lift coefficient with the high-lift system stowed.
    #[config(
        label = "Max lift coefficient, clean (CLmax_clean)",
        help = "Maximum lift coefficient in clean (flaps/slats up) configuration -- the true aerodynamic stall limit used for the V-n diagram's stall boundary, distinct from the flaps-down CLmax_TO/CLmax_L above."
    )]
    pub cl_max_clean: f64,

    /// Most negative lift coefficient the clean wing reaches.
    #[config(
        label = "Min lift coefficient, clean (CLmin_clean)",
        help = "Most negative (inverted-flight) lift coefficient in clean configuration -- the negative stall boundary on the V-n diagram."
    )]
    pub cl_min_clean: f64,

    /// How much static thrust is lost by the end of the ground roll.
    #[config(
        label = "Thrust lapse ratio",
        help = "Fraction by which static sea-level thrust falls off during the take-off ground roll / initial climb, used in the matching-chart take-off constraint."
    )]
    pub thrust_lapse: f64,

    /// Engine-out second-segment climb gradient, for engine counts with no
    /// certified figure of their own.
    #[config(
        label = "OEI 2nd-segment climb gradient (fallback)",
        unit = "fraction",
        help = "Conceptual fallback for an engine count outside the implemented 14 CFR 25.121(b) two/three/four-engine table. The matching chart auto-selects 0.024 (twin) / 0.027 (tri-jet) / 0.030 (quad) from the actual engine count; this fallback does not establish a Part 25 result for unsupported counts."
    )]
    pub oei_gradient: f64,

    /// Empirical constant relating approach speed to landing distance.
    #[config(
        label = "Landing distance factor (k_land)",
        help = "Empirical constant relating approach speed / wing loading to landing ground-roll + air distance."
    )]
    pub k_land: f64,

    /// Lift coefficient assumed during the engine-out climb.
    #[config(
        label = "OEI climb configuration CL",
        help = "Legacy constant lift coefficient for conceptual OEI second-segment climb L/D (Raymer Ch.17). A V2-based evaluation should derive CL from CLmax_TO and the selected V2/VSR or V2/VS ratio instead."
    )]
    pub oei_climb_cl: f64,

    /// High-lift drag added during the engine-out climb with gear retracted.
    #[config(
        label = "OEI climb high-lift drag increment (gear up)",
        help = "Parasite-drag increment added to clean CD0 for the takeoff flap/slat configuration with landing gear retracted, as required by 14 CFR 25.121(b). Asymmetric trim/control and inoperative-engine or windmilling drag require separate source values; this field does not represent them."
    )]
    pub oei_climb_delta_cd: f64,

    /// Ratio of installed thrust available at the selected OEI V2 condition
    /// to the rated sea-level-static thrust. This must come from an
    /// engine-performance deck or an explicitly identified aircraft source;
    /// it is intentionally unset for the generic conceptual defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        label = "OEI thrust at V2 / rated SLS thrust",
        help = "Optional condition-to-SLS installed thrust ratio for the FAR 25.121(b) V2 engine-out check. Supply a value from the selected engine deck at the departure altitude, temperature, Mach, bleed state and take-off thrust setting. A missing value keeps the OEI result as conceptual and reports an evidence gap."
    )]
    pub oei_condition_to_sls_thrust_ratio: Option<f64>,

    /// Asymmetric trim and control drag increment in the OEI V2 condition.
    ///
    /// This is separate from the high-lift increment above because the rudder
    /// and aileron trim solution depends on engine placement and the aircraft
    /// yaw-control schedule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        label = "OEI asymmetric trim/control drag increment",
        help = "Optional additional OEI trim/control drag coefficient at V2. Set only from an aircraft-specific yaw/trim analysis or source. Leave empty when no such evidence is available."
    )]
    pub oei_asymmetric_trim_cd: Option<f64>,

    /// Inoperative-engine/windmilling drag increment in the OEI V2 condition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        label = "OEI inoperative-engine/windmilling drag increment",
        help = "Optional additional drag coefficient for the failed engine or propeller at V2. Set only from the engine/airframe drag source for the selected failure state; it is not a generic zero."
    )]
    pub oei_windmilling_cd: Option<f64>,

    /// Left-hand end of the matching chart's wing-loading axis.
    #[config(
        label = "Matching chart wing-loading axis: minimum",
        unit = "Pa",
        help = "Lower wing-loading (W/S) axis limit on the matching chart plot. Widen for very light (GA) aircraft. (~204 kg/m^2)"
    )]
    pub ws_min_pa: f64,

    /// Right-hand end of the matching chart's wing-loading axis.
    #[config(
        label = "Matching chart wing-loading axis: maximum",
        unit = "Pa",
        help = "Upper wing-loading (W/S) axis limit on the matching chart plot. Widen for very heavy (freighter) aircraft. (~1020 kg/m^2)"
    )]
    pub ws_max_pa: f64,

    /// Multiplier turning takeoff distance into balanced field length.
    #[config(
        label = "Balanced field length factor",
        help = "BFL = bfl_factor x TODR (take-off distance required). Raymer Table 17.1: 1.15 for twin jets, ~1.18 for quads."
    )]
    pub bfl_factor: f64,

    /// Minimum control speed as a multiple of takeoff stall speed.
    #[config(
        label = "VMC / VS_TO",
        help = "Minimum-control speed as a multiple of take-off stall speed. FAR 25.149 caps VMC at 1.13*VSR; that ceiling is used as the default."
    )]
    pub vmc_vstall_factor: f64,

    /// Rotation speed floor relative to minimum control speed.
    #[config(
        label = "VR / VMC floor",
        help = "FAR 25.107: rotation speed must be at least 1.05*VMC."
    )]
    pub vr_vmc_factor: f64,

    /// Rotation speed floor relative to takeoff stall speed.
    #[config(
        label = "VR / VS_TO floor",
        help = "FAR 25.107: rotation speed must also be at least 1.10*VS_TO. VR = max(vr_vmc_factor*VMC, vr_vstall_factor*VS_TO)."
    )]
    pub vr_vstall_factor: f64,

    /// Takeoff safety speed as a multiple of takeoff stall speed.
    #[config(
        label = "V2 / VS_TO",
        help = "Take-off safety speed as a multiple of take-off stall speed (FAR 25.107). V2 = max(v2_vstall_factor*VS_TO, VR)."
    )]
    pub v2_vstall_factor: f64,

    /// Decision speed as a fraction of rotation speed.
    #[config(
        label = "V1 / VR",
        help = "Decision speed as a fraction of rotation speed. On a dry balanced field V1 sits just below VR (~0.95-0.98); the earlier 0.90 default put V1 unrealistically far below VR (e.g. ~152 kt against a real 787-9 V1 of 160-165 kt)."
    )]
    pub v1_vr_factor: f64,

    /// Approach speed as a multiple of landing stall speed.
    #[config(
        label = "VAPP / VS_land",
        help = "Approach speed as a multiple of landing stall speed (FAR 25.125)."
    )]
    pub vapp_vstall_land_factor: f64,

    /// Touchdown speed as a multiple of landing stall speed.
    #[config(
        label = "VTD / VS_land",
        help = "Touchdown speed as a multiple of landing stall speed."
    )]
    pub vtd_vstall_land_factor: f64,

    /// How finely the matching chart's constraint curves are drawn.
    #[config(
        label = "Matching chart plot resolution",
        help = "Number of wing-loading points swept when drawing the matching-chart constraint curves (Results -> Matching Chart). Purely a plotting resolution knob -- higher gives smoother curves at extra compute cost. Not part of the Performance preset (it isn't a physical assumption)."
    )]
    pub matching_chart_resolution: i64,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            cl_max_to: 1.80,
            cl_max_land: 2.60,
            cl_max_clean: 1.50,
            cl_min_clean: -1.00,
            thrust_lapse: 0.235,
            oei_gradient: 0.024,
            k_land: 0.60,
            oei_climb_cl: 1.2,
            oei_climb_delta_cd: 0.025,
            oei_condition_to_sls_thrust_ratio: None,
            oei_asymmetric_trim_cd: None,
            oei_windmilling_cd: None,
            ws_min_pa: 2000.0,
            ws_max_pa: 10000.0,
            bfl_factor: 1.15,
            vmc_vstall_factor: 1.13,
            vr_vmc_factor: 1.05,
            vr_vstall_factor: 1.10,
            v2_vstall_factor: 1.20,
            v1_vr_factor: 0.95,
            vapp_vstall_land_factor: 1.30,
            vtd_vstall_land_factor: 1.15,
            matching_chart_resolution: 150,
        }
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_high_lift_system_helps_in_both_configurations() {
        // Deploying flaps that produce less lift than the clean wing is a
        // transposed pair rather than an aircraft, and the matching chart
        // would size a field length from it without complaint.
        let config = PerformanceConfig::default();
        assert!(config.cl_max_to > config.cl_max_clean);
        assert!(config.cl_max_land > config.cl_max_to);
        assert!(config.cl_min_clean < 0.0);
    }

    #[test]
    fn the_takeoff_speed_schedule_is_ordered_as_the_regulation_requires() {
        // V1 below VR below V2, all above the stall. An out-of-order schedule
        // describes a takeoff where the decision speed comes after rotation.
        let config = PerformanceConfig::default();
        assert!(config.v1_vr_factor < 1.0, "V1 must sit below VR");
        assert!(config.vr_vstall_factor < config.v2_vstall_factor);
        assert!(config.vr_vstall_factor > 1.0, "VR must sit above the stall");
    }

    #[test]
    fn the_approach_is_flown_faster_than_the_touchdown() {
        let config = PerformanceConfig::default();
        assert!(config.vapp_vstall_land_factor > config.vtd_vstall_land_factor);
        assert!(config.vtd_vstall_land_factor > 1.0);
    }

    #[test]
    fn a_balanced_field_is_never_shorter_than_the_takeoff_it_contains() {
        assert!(PerformanceConfig::default().bfl_factor >= 1.0);
    }

    #[test]
    fn the_matching_chart_axis_spans_a_real_range() {
        let config = PerformanceConfig::default();
        assert!(config.ws_min_pa < config.ws_max_pa);
        assert!(config.ws_min_pa > 0.0);
    }
}
