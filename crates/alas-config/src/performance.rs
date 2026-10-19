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
//! The speed factors follow CS 25.107 and 25.125. Every stall-referenced
//! speed uses the reference stall speed VSR taken on a 1-g basis:
//! `VSR = sqrt(2 W / (rho S CLmax))` with the configured CLmax read as the
//! lift coefficient at the CS 25.103 reference stall speed (load factor
//! normal to the flight path of one). V2 is at least 1.13 VSR (1.08 for the
//! exceptions in CS 25.107(b)) and 1.10 VMC; Vref is at least 1.23 VSR0
//! (CS 25.125(b)(2)(i)). The legacy approach factor of 1.30 is selectable
//! for numerical replay and is not the regulatory minimum.

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

    /// Move the declared maximum lift coefficients with the candidate wing.
    #[serde(default = "default_geometry_scaled_clmax")]
    #[config(
        label = "Scale CLmax with wing geometry",
        help = "During design optimization, scale the declared clean, take-off and landing maximum lift coefficients with the candidate wing: clean by cos(quarter-chord sweep) and the flap and slat increments by flapped-area ratio times cos(hinge sweep), both relative to the registered geometry (Raymer eqs. 12.15 and 12.21; Roskam VI eq. 8.29). At the registered geometry the declared values are used unchanged. Off keeps the declared values for every candidate, a calibration input independent of sweep and flap span."
    )]
    pub geometry_scaled_clmax: bool,

    /// Improved-climb take-off safety speed.
    #[serde(default = "default_improved_climb_v2")]
    #[config(
        label = "Improved-climb take-off speed",
        help = "Fly the take-off safety speed V2 above the schedule when that is what it takes to meet the CS 25.121(b) second-segment gradient (improved climb, overspeed): V2 is raised from the schedule value to the lowest speed that meets the gradient, no higher than the best-climb speed, and the longer take-off field it implies is charged in the field checks. Jets only. Off keeps V2 at the schedule value and reports a second-segment shortfall as it is."
    )]
    pub improved_climb_v2: bool,

    /// The V2/VSR the improved-climb selection resolved to, when it is above the schedule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        hidden,
        help = "Resolved improved-climb take-off safety speed over the reference stall speed. Written by the analysis, not a user input."
    )]
    pub improved_climb_v2_over_vsr: Option<f64>,

    /// Cruise-altitude thrust requirements from the engine deck.
    #[serde(default = "default_cruise_thrust_from_deck")]
    #[config(
        label = "Cruise thrust from the engine deck",
        help = "Judge the cruise-altitude thrust requirements with the thrust the engine deck delivers at the design cruise Mach and altitude (maximum-cruise rating for level flight, maximum-climb rating for the climb rate), instead of the flat thrust lapse ratio: level flight at the mid-cruise weight, and a 300 ft/min residual rate of climb at the top-of-climb mass (initial cruise altitude capability, the criterion the mission cruise-level search applies). Off restores the flat Thrust lapse ratio for the cruise check."
    )]
    pub cruise_thrust_from_deck: bool,

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

    /// Take-off flap/slat drag added during the engine-out climb, gear up.
    #[config(
        label = "OEI climb high-lift drag increment (gear up)",
        help = "Take-off flap/slat drag increment on the reference wing area, landing gear retracted (14 CFR 25.121(b) second-segment configuration). Default 0.015: Roskam, Airplane Design Part I, Table 3.6, take-off flaps 0.010-0.020 (first estimate, zero-lift increment); component methods (Roskam VI Sec. 4.6, Raymer 6th ed. Eq. 12.61-12.62, Torenbeek App. G-4) give 0.010-0.013 for a 20 deg Fowler flap including flap-induced drag, plus 0.002-0.007 for slats. Windmilling and asymmetric trim drag are separate terms."
    )]
    pub oei_climb_delta_cd: f64,

    /// Landing-gear drag with take-off flaps: ground roll, rotation and the
    /// first segment.
    #[serde(default = "default_takeoff_gear_delta_cd")]
    #[config(
        label = "Landing-gear drag increment, take-off",
        help = "Drag increment of the extended landing gear on the reference wing area in the take-off phase (ground roll, rotation, 14 CFR 25.121(a) first segment). Default 0.020: Roskam, Airplane Design Part I, Table 3.6, landing gear 0.015-0.025 (first estimate); Torenbeek Sec. 5.4.3(d) 0.015-0.020 typical; component methods span 0.015 (Raymer 6th ed. Sec. 12.5.7) to 0.030 (Torenbeek Eq. G-66 with sheltering)."
    )]
    pub takeoff_gear_delta_cd: f64,

    /// Landing flap/slat drag with the gear retracted.
    #[serde(default = "default_landing_flap_delta_cd")]
    #[config(
        label = "Landing flap/slat drag increment",
        help = "Landing flap/slat drag increment on the reference wing area, landing gear retracted (14 CFR 25.121(d) approach-climb basis). Default 0.040: Roskam VI Sec. 4.6 and Raymer 6th ed. Eq. 12.61-12.62 give 0.025-0.033 for a 35 deg Fowler flap including flap-induced drag, plus 0.002-0.007 for slats; Roskam I Table 3.6 lists 0.055-0.075 for large-deflection (about 60 deg) plain or split flaps."
    )]
    pub landing_flap_delta_cd: f64,

    /// Landing-gear drag in the landing phase.
    #[serde(default = "default_landing_gear_delta_cd")]
    #[config(
        label = "Landing-gear drag increment, landing",
        help = "Drag increment of the extended landing gear on the reference wing area in the landing phase (approach, flare, 14 CFR 25.119 landing climb). Default 0.020, same basis as the take-off value; the main gear is sheltered by flap lift (Torenbeek Eq. G-67), so the landing value is not larger."
    )]
    pub landing_gear_delta_cd: f64,

    /// Go-around thrust available 8 s after the thrust levers are advanced.
    #[serde(default = "default_go_around_spool_thrust_fraction")]
    #[config(
        label = "Go-around thrust fraction at 8 s (spool-up)",
        unit = "fraction",
        help = "Ratio of the thrust available 8 seconds after advancing from minimum flight idle (CS 25.119) to the full go-around thrust. 1.0 is the upper bound and is non-conservative: 14 CFR 33.73(b) only requires 95 percent of rated take-off thrust within 5 s at sea-level static, so a defensible lower bound is 0.95. Replace with an engine-specific acceleration datum when available."
    )]
    pub go_around_spool_thrust_fraction: f64,

    /// Minimum take-off safety speed relative to the reference stall speed.
    #[serde(default = "default_v2_min_vsr_factor")]
    #[config(
        label = "V2MIN / VSR",
        help = "CS 25.107(b): minimum take-off safety speed as a multiple of the reference stall speed VSR. 1.13 for two- and three-engined turbo-propeller aeroplanes and turbojets without provisions for a significant reduction in one-engine-inoperative power-on stall speed; 1.08 for turbo-propeller aeroplanes with more than three engines and turbojets with such provisions. V2 is also never below 1.10 VMC (CS 25.107(b))."
    )]
    pub v2_min_vsr_factor: f64,

    /// Wheels on each main-gear truck (bogie); zero when undeclared.
    #[serde(default)]
    #[config(
        label = "Main-gear wheels per truck (0 = undeclared)",
        help = "Declared wheels on each main-gear truck, which selects the mean braked-run deceleration of the jet landing-distance model: Torenbeek, Advanced Aircraft Design (2013), p. 274 (ESDU 84040 statistics for jets with automatic ground spoilers, anti-skid, no reverse thrust): 0.40 g for two-wheel trucks, 0.32 g for four-wheel trucks. Presets copy their landing-gear declaration (first strut of the bogie list, else wheels per strut). 0 is treated as the four-wheel class, the longer distance."
    )]
    pub main_gear_truck_wheels: i64,

    /// Whether the landing field-length check assumes a wet runway.
    #[serde(default)]
    #[config(
        label = "Wet landing runway",
        help = "Apply the wet-runway landing distance factor. EU Air OPS CAT.POL.A.235(a)(2): when the flight manual gives no wet-runway distance, the landing distance available must be at least 115 percent of the dry distance required by CAT.POL.A.230, so the usable dry share of the runway falls by 1/1.15 (jets 0.60 to 0.522). Off by default (dry runway); the wet take-off distance of CS 25.113(b) is not modelled."
    )]
    pub wet_landing_runway: bool,

    /// Accepted ratio band between the closed take-off mass and the mass the
    /// engine was designed for.
    #[serde(default = "default_engine_match_tolerance")]
    #[config(
        label = "Engine-to-mass match tolerance",
        unit = "fraction",
        help = "Half-width of the accepted band for the closed take-off mass divided by the take-off mass the selected engine was designed for. A ratio outside 1 +/- this value raises a warning: a fixed engine rating no longer matches the aircraft it was sized with. Default 0.15."
    )]
    pub engine_match_tolerance: f64,

    /// Ratio of installed thrust available at the selected OEI V2 condition
    /// to the rated sea-level-static thrust. This must come from an
    /// engine-performance deck or an explicitly identified aircraft source;
    /// it is intentionally unset for the generic conceptual defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        label = "OEI thrust at V2 / rated SLS thrust",
        help = "Optional manual override of the condition-to-SLS installed thrust ratio for the CS 25.121 engine-out checks. Empty (default): the ratio is taken from the selected engine deck, take-off/go-around rating, at V2, the departure elevation and ISA deviation (one shared helper for the optimizer, the feasibility findings and the report). Supply a value only from approved performance data."
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
        help = "Optional manual override of the asymmetric trim drag coefficient at V2 (reference wing area). Empty (default): Torenbeek, Synthesis of Subsonic Airplane Design, App. G-8.3, Eqs. G-84 to G-89 (fin induced drag plus rudder profile drag for the zero-sideslip yaw balance of the failed-engine thrust and windmilling drag) from the candidate fin, rudder and engine positions. That value is an upper-bound estimate: it ignores the sideslip and 5 deg bank relief CS 25.149 allows."
    )]
    pub oei_asymmetric_trim_cd: Option<f64>,

    /// Inoperative-engine/windmilling drag increment in the OEI V2 condition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(
        label = "OEI inoperative-engine/windmilling drag increment",
        help = "Optional manual override of the failed-engine drag coefficient at V2 (reference wing area). Empty (default): Roskam, Airplane Design Part VI, Eq. 4.67 (equal to Torenbeek Eq. G-82) windmilling turbofan drag area from the fan diameter and nozzle stream data, or Eq. 4.69 (Torenbeek G-83) for a stopped propeller; it is never a generic zero."
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
        help = "Declared minimum-control speed as a multiple of the take-off reference stall speed VSR. The speed schedules use the VMC computed from the one-engine-inoperative yaw balance of the built fin and rudder; this value is the fallback when that balance cannot be evaluated and, for a registered (certified) aircraft, the cap on the computed value. CS 25.149(c) puts the certification ceiling at 1.13 VSR. VMC drives V2 >= 1.10 VMC and VR >= 1.05 VMC."
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
        help = "Selected take-off safety speed as a multiple of the take-off reference stall speed VSR. V2 = max(v2_vstall_factor*VSR, V2MIN, VR) with V2MIN = max(v2_min_vsr_factor*VSR, 1.10*VMC) per CS 25.107(b)-(c); the regulatory minimum therefore overrides a lower selected factor."
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

fn default_geometry_scaled_clmax() -> bool {
    true
}

fn default_cruise_thrust_from_deck() -> bool {
    true
}

fn default_improved_climb_v2() -> bool {
    true
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            cl_max_to: 1.80,
            cl_max_to_source: default_takeoff_lift_source(),
            cl_max_land: 2.60,
            cl_max_land_source: default_landing_lift_source(),
            legacy_field_correlations: false,
            geometry_scaled_clmax: true,
            cruise_thrust_from_deck: true,
            improved_climb_v2: true,
            improved_climb_v2_over_vsr: None,
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
            // Legacy replay only (`legacy_field_correlations`): the production
            // landing distance is the Torenbeek segment model (alas-perf
            // `landing.rs`). 0.505 m/Pa was the mean of 0.6 LFL / (0.5 rho0
            // (Vref/1.23)^2) over eight certified jets (calibration, retained
            // for the frozen replay; Loftin NASA RP-1060 Fig. 3.4: 0.572).
            k_land: 0.505,
            oei_climb_cl: 1.2,
            oei_climb_delta_cd: 0.015,
            takeoff_gear_delta_cd: default_takeoff_gear_delta_cd(),
            landing_flap_delta_cd: default_landing_flap_delta_cd(),
            landing_gear_delta_cd: default_landing_gear_delta_cd(),
            go_around_spool_thrust_fraction: default_go_around_spool_thrust_fraction(),
            v2_min_vsr_factor: default_v2_min_vsr_factor(),
            main_gear_truck_wheels: 0,
            wet_landing_runway: false,
            engine_match_tolerance: default_engine_match_tolerance(),
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
            vapp_vstall_land_factor: 1.23,
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
fn default_takeoff_gear_delta_cd() -> f64 {
    0.020
}
fn default_landing_flap_delta_cd() -> f64 {
    0.040
}
fn default_landing_gear_delta_cd() -> f64 {
    0.020
}
fn default_go_around_spool_thrust_fraction() -> f64 {
    1.0
}
fn default_v2_min_vsr_factor() -> f64 {
    1.13
}
fn default_engine_match_tolerance() -> f64 {
    0.15
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
