// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/performance_config.py

//! High-lift capability, field performance and the certified speed schedule.
//!
//! The matching chart turns requirements into a wing loading and a
//! thrust-to-weight ratio, and almost everything it needs is here: what lift
//! the high-lift system can produce, how much thrust is lost during the
//! ground roll, and the climb gradient certification demands with an engine
//! out. Defaults follow Raymer (*Aircraft Design: A Conceptual Approach*,
//! 5th ed.) and FAR Part 25.
//!
//! The configurable speed factors preserve the translated legacy schedule.
//! Current 14 CFR 25.125(b)(2) sets a non-icing Vref floor of 1.23 VSR0;
//! the propulsion-specific field method uses 1.23 VS1g as its preliminary
//! approximation. The legacy approach factor of 1.30 remains selectable for
//! numerical replay and is not the current regulatory minimum.

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

    /// Reference supporting the configured takeoff maximum lift coefficient.
    #[serde(default = "default_takeoff_lift_source")]
    #[config(
        label = "Takeoff maximum lift source",
        help = "Reference for CLmax_TO. A coefficient recovered from a published V2 and the selected model speed ratio is an effective model input, not a measured aerodynamic maximum or a certified speed schedule."
    )]
    pub cl_max_to_source: String,

    /// Maximum lift coefficient in the landing configuration.
    #[config(
        label = "Max lift coefficient, landing (CLmax_L)",
        help = "Maximum lift coefficient achievable in the landing flap/slat configuration. Drives landing distance."
    )]
    pub cl_max_land: f64,

    /// Reference supporting the configured landing maximum lift coefficient.
    #[serde(default = "default_landing_lift_source")]
    #[config(
        label = "Landing maximum lift source",
        help = "Reference for CLmax_land, including the published landing mass and reference approach speed when it is derived from an aircraft source. Generic values are conceptual high-lift assumptions."
    )]
    pub cl_max_land_source: String,

    /// Select the translated field correlations and their original speed factors.
    #[serde(default)]
    #[config(
        label = "Use legacy field correlations",
        help = "Reproduce the translated jet correlation for every propulsion type, including its additional balanced-field multiplier and configured approach-speed factor. Disable for propulsion-specific sourced field estimates."
    )]
    pub legacy_field_correlations: bool,

    /// Dry hard-runway rolling coefficient for the propeller takeoff calculation.
    #[serde(default = "default_takeoff_rolling_friction")]
    #[config(
        label = "Propeller takeoff rolling friction",
        help = "Dry concrete rolling coefficient in the ground equation of motion. Torenbeek, Synthesis of Subsonic Airplane Design, Sec. 5.4.5, p. 168: 0.02. This is rolling resistance, not the braking coefficient."
    )]
    pub propeller_takeoff_rolling_friction: f64,

    /// Mean dry-runway reject deceleration divided by standard gravity.
    #[serde(default = "default_takeoff_stop_deceleration")]
    #[config(
        label = "Propeller rejected-takeoff deceleration",
        unit = "g",
        help = "Torenbeek Sec. 5.4.5, p. 169: mean stopping deceleration 0.37 g in the preliminary balanced-field calculation. No reverse-thrust credit is included. Aircraft-specific brake data supersede this conceptual value."
    )]
    pub propeller_takeoff_stop_deceleration_g: f64,

    /// Sea-level inertia-distance allowance in the preliminary balanced-field model.
    #[serde(default = "default_takeoff_inertia_distance")]
    #[config(
        label = "Propeller takeoff inertia distance",
        unit = "m",
        help = "Torenbeek Sec. 5.4.5, p. 169, Eq. 5-89: 200 m for an equivalent inertia time of 4.5 s, valid for preliminary propeller and jet field estimates. The allowance scales as 1/sqrt(density ratio)."
    )]
    pub propeller_takeoff_inertia_distance_m: f64,

    /// Mean landing airborne drag minus thrust divided by weight.
    #[serde(default = "default_landing_mean_drag_to_weight")]
    #[config(
        label = "Propeller landing mean (D-T)/W",
        help = "Torenbeek Sec. 5.4.6, p. 170, Eq. 5-93: 0.10 is a preliminary mean excess-drag-to-weight ratio between the 50 ft screen and touchdown. Includes approach and flare energy dissipation."
    )]
    pub propeller_landing_mean_drag_to_weight: f64,

    /// Mean dry-concrete landing deceleration divided by gravity.
    #[serde(default = "default_landing_deceleration")]
    #[config(
        label = "Propeller landing deceleration",
        unit = "g",
        help = "Torenbeek Sec. 5.4.6, p. 170: turboprop without propeller reverse 0.35-0.45 g. The default 0.40 g is the midpoint of that sourced preliminary range, not an aircraft-data fit."
    )]
    pub propeller_landing_deceleration_g: f64,

    /// Share of available runway usable by the unfactored dry landing distance.
    #[serde(default = "default_propeller_landing_distance_share")]
    #[config(
        label = "Propeller dry landing distance share",
        help = "EU Air Ops CAT.POL.A.230(a)(2): turboprop dry landing distance must fit within 70 percent of LDA. Use 0.60 for a 14 CFR 121.195(b) turbine-airplane dispatch comparison. This factor is applied to actual landing distance exactly once."
    )]
    pub propeller_dry_landing_distance_share: f64,

    /// Maximum lift coefficient with the high-lift system stowed.
    #[config(
        label = "Max lift coefficient, clean (CLmax_clean)",
        help = "Maximum lift coefficient in clean (flaps/slats up) configuration: the true aerodynamic stall limit used for the V-n diagram's stall boundary, distinct from the flaps-down CLmax_TO/CLmax_L above."
    )]
    pub cl_max_clean: f64,

    /// Most negative lift coefficient the clean wing reaches.
    #[config(
        label = "Min lift coefficient, clean (CLmin_clean)",
        help = "Most negative (inverted-flight) lift coefficient in clean configuration: the negative stall boundary on the V-n diagram."
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

    /// Historical replay multiplier on the translated jet field correlation.
    #[config(
        label = "Balanced field length factor",
        help = "Additional historical multiplier applied only by the legacy field path. The corrected jet TOP correlation already estimates field length and ignores this factor. Under 14 CFR 25.113, 1.15 applies to the all-engine distance candidate, not to every balanced field length."
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

    /// Legacy approach speed as a multiple of landing stall speed.
    #[config(
        label = "VAPP / VS_land",
        help = "Approach-speed multiple retained for translated legacy results. The corrected method uses Vref = 1.23 VS1g, approximating the non-icing 14 CFR 25.125 VSR0 floor."
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
        help = "Number of wing-loading points swept when drawing the matching-chart constraint curves (Results -> Matching Chart). Purely a plotting resolution knob: higher gives smoother curves at extra compute cost. Not part of the Performance preset (it isn't a physical assumption)."
    )]
    pub matching_chart_resolution: i64,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            cl_max_to: 1.80,
            cl_max_to_source: default_takeoff_lift_source(),
            cl_max_land: 2.60,
            cl_max_land_source: default_landing_lift_source(),
            legacy_field_correlations: false,
            propeller_takeoff_rolling_friction: default_takeoff_rolling_friction(),
            propeller_takeoff_stop_deceleration_g: default_takeoff_stop_deceleration(),
            propeller_takeoff_inertia_distance_m: default_takeoff_inertia_distance(),
            propeller_landing_mean_drag_to_weight: default_landing_mean_drag_to_weight(),
            propeller_landing_deceleration_g: default_landing_deceleration(),
            propeller_dry_landing_distance_share: default_propeller_landing_distance_share(),
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

fn default_takeoff_lift_source() -> String {
    "Raymer, Aircraft Design: A Conceptual Approach, 5th ed., 2012, Sec. 5.4 and Ch. 17; retained conceptual high-lift CLmax_TO input, not a manufacturer measurement or a recovered certified stall limit".to_owned()
}

fn default_landing_lift_source() -> String {
    "Mattingly et al., Aircraft Engine Design, 2nd ed., Table 2.1, p. 36 (credits Torenbeek 1976): Fowler landing CLmax/cos(quarter-chord sweep) 2.5-2.9; CLmax 2.60 is a retained engineering selection at nominal 25 deg (normalized 2.87), a conceptual class default rather than an aircraft coefficient; https://doczz.net/doc/8595015/2-constraint-analysis".to_owned()
}

fn default_takeoff_rolling_friction() -> f64 {
    0.02
}
fn default_takeoff_stop_deceleration() -> f64 {
    0.37
}
fn default_takeoff_inertia_distance() -> f64 {
    200.0
}
fn default_landing_mean_drag_to_weight() -> f64 {
    0.10
}
fn default_landing_deceleration() -> f64 {
    0.40
}
fn default_propeller_landing_distance_share() -> f64 {
    0.70
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
