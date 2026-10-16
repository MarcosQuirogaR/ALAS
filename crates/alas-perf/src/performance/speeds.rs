// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Translated legacy V-speed schedule and field correlations for numerical replay.

use alas_config::airports::Airport;
use alas_config::PerformanceConfig;

use super::{density_ratio, G, M_TO_FT, PA_TO_PSF, RHO_SL};

/// CS 25.107(b): `V2MIN` is not less than `1.10 VMC`.
pub use alas_config::V2_MIN_OVER_VMC;

/// Reference speeds [m/s] for one aircraft + aerodrome combination:
/// `VSpeeds`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VSpeeds {
    /// Reference stall speed `VSR` in the take-off configuration, m/s:
    /// `sqrt(2 W / (rho S CLmax_TO))` with the configured `CLmax_TO` read as
    /// the lift coefficient at the CS 25.103 reference stall speed on a 1-g
    /// basis (load factor normal to the flight path of one). A `CLmax` taken
    /// from a wind-tunnel or handbook maximum (a minimum-speed basis) needs
    /// the CS 25.103 `VSR` definition applied before use: Torenbeek Sec. 5.4.4
    /// p. 166 puts the apparent difference near 13 percent of `CLmax`.
    pub v_stall_to_ms: f64,
    /// Reference stall speed `VSR0` in the landing configuration, m/s, on the
    /// same 1-g basis as [`Self::v_stall_to_ms`].
    pub v_stall_land_ms: f64,
    /// Minimum control speed (CS 25.149): `vmc_vstall_factor` times `VSR`. The
    /// caller puts the `VMC / VSR` computed from the yaw balance in that field
    /// (`alas_opt::mdo::performance_with_vmc`); left at its default it is the
    /// 1.13 VSR ceiling, an estimate rather than a computed value.
    pub v_mc_ms: f64,
    /// Conceptual decision-speed proxy (floored at airborne `VMC`).
    ///
    /// This is not a certified 14 CFR 25.107(a) `V1`: the product model has
    /// no `VEF`, `VMCG`, engine-out acceleration, or pilot reaction-time
    /// inputs, so it cannot evaluate the required `VEF + speed gained`
    /// relation.
    pub v1_ms: f64,
    /// Rotation speed (FAR 25.107).
    pub v_r_ms: f64,
    /// Take-off safety speed (FAR 25.107).
    pub v2_ms: f64,
    /// Reference approach speed; the selected method declares its stall multiple.
    pub v_app_ms: f64,
    /// Touchdown speed.
    pub v_td_ms: f64,
}

/// Compute the configurable legacy V-speed schedule at the given aerodrome conditions:
/// `compute_v_speeds`.
///
/// The multiplicative factors come from `perf_config` rather than being fixed,
/// so they can be calibrated to a real type. `VR` carries explicit floors: it
/// is the larger of its `VMC` and stall multiples (FAR 25.107). The returned
/// `V1` is only a conceptual proxy floored at airborne `VMC`; a certified V1
/// requires the `VEF`, `VMCG`, engine-out acceleration, and reaction-time
/// data required by 14 CFR 25.107(a).
pub fn compute_v_speeds(
    mtow_kg: f64,
    wing_area_m2: f64,
    airport: &Airport,
    cl_max_to: f64,
    cl_max_land: f64,
    perf_config: &PerformanceConfig,
) -> VSpeeds {
    compute_v_speeds_at_masses(
        mtow_kg,
        mtow_kg,
        wing_area_m2,
        airport,
        cl_max_to,
        cl_max_land,
        perf_config,
    )
}

/// Compute the configurable legacy speed schedule with separate take-off and landing
/// masses. Take-off speeds remain tied to MTOW, while stall/approach/
/// touchdown speeds use the arrival mass.
#[allow(clippy::too_many_arguments)] // one per mass, configuration and field input
pub fn compute_v_speeds_at_masses(
    mtow_kg: f64,
    landing_mass_kg: f64,
    wing_area_m2: f64,
    airport: &Airport,
    cl_max_to: f64,
    cl_max_land: f64,
    perf_config: &PerformanceConfig,
) -> VSpeeds {
    let pc = perf_config;
    let sigma = density_ratio(airport.elevation_m, airport.isa_deviation_c);
    let rho = sigma * RHO_SL;
    let ws_pa = mtow_kg * G / wing_area_m2;
    let landing_ws_pa = landing_mass_kg.max(0.0) * G / wing_area_m2;

    let v_stall_to = (2.0 * ws_pa / (rho * cl_max_to)).sqrt();
    let v_stall_land = (2.0 * landing_ws_pa / (rho * cl_max_land)).sqrt();

    // `v_stall_to` is the take-off reference stall speed VSR on a 1-g basis
    // (CS 25.103); VMC is `vmc_vstall_factor` (computed by the caller or the declared ceiling).
    let v_mc = pc.vmc_vstall_factor * v_stall_to;
    let v_r = (pc.vr_vmc_factor * v_mc).max(pc.vr_vstall_factor * v_stall_to);
    let v1 = (pc.v1_vr_factor * v_r).max(v_mc);
    // CS 25.107(b): V2MIN is not below 1.13 VSR (1.08 VSR for the listed
    // exceptions) and not below 1.10 VMC; (c): V2 is not below V2MIN or VR.
    // The "VR + speed gained before 35 ft" term needs the rotation march
    // (FLD-09) and is not modelled, so that increment is zero here.
    // The translated legacy replay (`legacy_field_correlations`) keeps the
    // original `V2 = max(factor * VS, VR)` the golden fixtures were
    // generated with.
    let v2_min = if pc.legacy_field_correlations {
        0.0
    } else {
        (pc.v2_min_vsr_factor * v_stall_to).max(V2_MIN_OVER_VMC * v_mc)
    };
    // An improved-climb V2 (improved_climb_v2_over_vsr, resolved from the
    // second-segment gradient) only ever raises the safety speed.
    let v2 = (pc.v2_vstall_factor * v_stall_to)
        .max(v2_min)
        .max(v_r)
        .max(pc.improved_climb_v2_over_vsr.unwrap_or(0.0) * v_stall_to);
    let v_app = pc.vapp_vstall_land_factor * v_stall_land;
    let v_td = pc.vtd_vstall_land_factor * v_stall_land;

    VSpeeds {
        v_stall_to_ms: v_stall_to,
        v_stall_land_ms: v_stall_land,
        v_mc_ms: v_mc,
        v1_ms: v1,
        v_r_ms: v_r,
        v2_ms: v2,
        v_app_ms: v_app,
        v_td_ms: v_td,
    }
}

/// Estimated field-performance distances for one aircraft + aerodrome:
/// `FieldPerformance`.
///
/// Holds the aerodrome so the distances-available and the margins against them
/// are read from the same source the sizing used.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldPerformance {
    /// The aerodrome these distances were estimated at.
    pub airport: Airport,
    /// The V-speed schedule at the same conditions.
    pub v_speeds: VSpeeds,
    /// Take-off distance required (35 ft screen), m.
    pub todr_m: f64,
    /// Balanced field length, m.
    pub bfl_m: f64,
    /// Conceptual accelerate-stop proxy, m; not an independent certified calculation.
    pub asd_m: f64,
    /// Actual landing distance from the 50 ft screen, m, before dispatch factors.
    pub ldr_m: f64,
    /// Landing mass used for the landing speeds and distance, kg.
    pub landing_mass_kg: f64,
}

impl FieldPerformance {
    /// Take-off distance available, m.
    pub fn toda_m(&self) -> f64 {
        self.airport.toda_m
    }

    /// Landing distance available, m.
    pub fn lda_m(&self) -> f64 {
        self.airport.lda_m
    }

    /// Runway to spare on take-off, m (negative if the field is too short).
    pub fn to_margin_m(&self) -> f64 {
        self.toda_m() - self.todr_m
    }

    /// Runway to spare before applying the operational landing factor, m.
    pub fn land_margin_m(&self) -> f64 {
        self.lda_m() - self.ldr_m
    }

    /// Whether the take-off fits the runway.
    pub fn to_feasible(&self) -> bool {
        self.todr_m <= self.toda_m()
    }

    /// Whether the actual landing fits, before operational dispatch factors.
    pub fn land_feasible(&self) -> bool {
        self.ldr_m <= self.lda_m()
    }
}

/// Replay the translated take-off and landing correlations: `compute_field_performance`.
///
/// Uses the same empirical constants as the constraint curves so the distances
/// are consistent with the design-space boundary. `tw_sl` is the sea-level
/// static `T/W` at MTOW. The historical `bfl_factor` multiplies a TOP field
/// estimate that already includes its field-length basis; production callers
/// use [`super::field::compute_field_performance_for_propulsion`] to avoid
/// that duplicate multiplier and select an appropriate propulsion method.
#[allow(clippy::too_many_arguments)] // one argument per physical input
pub fn compute_field_performance(
    mtow_kg: f64,
    wing_area_m2: f64,
    airport: &Airport,
    cl_max_to: f64,
    cl_max_land: f64,
    tw_sl: f64,
    k_land: f64,
    bfl_factor: f64,
    perf_config: &PerformanceConfig,
) -> FieldPerformance {
    compute_field_performance_at_masses(
        mtow_kg,
        mtow_kg,
        wing_area_m2,
        airport,
        cl_max_to,
        cl_max_land,
        tw_sl,
        k_land,
        bfl_factor,
        perf_config,
    )
}

/// Replay translated field performance with distinct take-off and landing masses.
///
/// Take-off sizing uses MTOW while the landing schedule uses the supplied
/// landing mass (normally the mission arrival mass or, absent telemetry, the
/// configured MLW cap). Keeping these masses separate prevents an MTOW-based
/// LDR from being presented as an arrival performance result.
#[allow(clippy::too_many_arguments)] // one per mass, configuration and field input
pub fn compute_field_performance_at_masses(
    mtow_kg: f64,
    landing_mass_kg: f64,
    wing_area_m2: f64,
    airport: &Airport,
    cl_max_to: f64,
    cl_max_land: f64,
    tw_sl: f64,
    k_land: f64,
    bfl_factor: f64,
    perf_config: &PerformanceConfig,
) -> FieldPerformance {
    let sigma = density_ratio(airport.elevation_m, airport.isa_deviation_c);
    let ws_pa = mtow_kg * G / wing_area_m2;
    let landing_mass_kg = landing_mass_kg.max(0.0);
    let landing_ws_pa = landing_mass_kg * G / wing_area_m2;
    let ws_psf = ws_pa * PA_TO_PSF;

    let v_speeds = compute_v_speeds_at_masses(
        mtow_kg,
        landing_mass_kg,
        wing_area_m2,
        airport,
        cl_max_to,
        cl_max_land,
        perf_config,
    );

    // The correlation reads the lift-off speed through CLmax_TO; flying an improved-
    // climb V2 raises it, which is the same as a lower effective CLmax_TO.
    let toda_req_ft =
        37.7 * ws_psf / (sigma * perf_config.takeoff_field_cl_max(cl_max_to) * tw_sl.max(1e-6));
    let todr_m = toda_req_ft / M_TO_FT;
    let bfl_m = todr_m * bfl_factor;
    let asd_m = bfl_m;
    let ldr_m = landing_ws_pa * k_land / (sigma * cl_max_land);

    FieldPerformance {
        airport: airport.clone(),
        v_speeds,
        todr_m,
        bfl_m,
        asd_m,
        ldr_m,
        landing_mass_kg,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sea_level() -> Airport {
        Airport::custom("Fixture", 0.0, 3500.0, 3500.0, 0.0, 0.0, 0.0)
    }

    #[test]
    fn the_takeoff_schedule_is_ordered_and_above_the_stall() {
        let vs = compute_v_speeds(
            79_000.0,
            122.0,
            &sea_level(),
            1.8,
            2.6,
            &PerformanceConfig::default(),
        );
        assert!(vs.v_stall_to_ms < vs.v_r_ms, "VR is above the stall");
        assert!(vs.v1_ms <= vs.v_r_ms, "V1 sits at or below VR");
        assert!(vs.v_r_ms <= vs.v2_ms, "V2 is at or above VR");
        assert!(
            vs.v_app_ms > vs.v_td_ms,
            "approach is faster than touchdown"
        );
    }

    #[test]
    fn v1_is_never_below_minimum_control_speed() {
        // A tight VR schedule would put v1_vr_factor*VR under VMC; the floor
        // must hold V1 at VMC rather than let it dip below.
        let pc = PerformanceConfig {
            vr_vmc_factor: 1.0,
            vr_vstall_factor: 1.0,
            v1_vr_factor: 0.9,
            ..PerformanceConfig::default()
        };
        let vs = compute_v_speeds(79_000.0, 122.0, &sea_level(), 1.8, 2.6, &pc);
        assert!(vs.v1_ms >= vs.v_mc_ms);
        assert_eq!(vs.v1_ms, vs.v_mc_ms);
    }

    #[test]
    fn a_short_field_is_reported_infeasible() {
        let tiny = Airport::custom("Tiny", 0.0, 500.0, 500.0, 0.0, 0.0, 0.0);
        let fp = compute_field_performance(
            250_000.0,
            120.0,
            &tiny,
            1.8,
            2.6,
            0.25,
            0.6,
            1.15,
            &PerformanceConfig::default(),
        );
        assert!(!fp.to_feasible());
        assert!(fp.to_margin_m() < 0.0);
    }

    #[test]
    fn hot_day_density_raises_every_stall_speed() {
        let airport = sea_level();
        let hot_airport = Airport::custom("Hot", 0.0, 3500.0, 3500.0, 25.0, 0.0, 0.0);
        let standard = compute_v_speeds(
            79_000.0,
            122.0,
            &airport,
            1.8,
            2.6,
            &PerformanceConfig::default(),
        );
        let hot = compute_v_speeds(
            79_000.0,
            122.0,
            &hot_airport,
            1.8,
            2.6,
            &PerformanceConfig::default(),
        );

        assert!(hot.v_stall_to_ms > standard.v_stall_to_ms);
        assert!(hot.v_stall_land_ms > standard.v_stall_land_ms);
        assert!(hot.v_mc_ms > standard.v_mc_ms);
        assert!(hot.v_app_ms > standard.v_app_ms);
    }

    #[test]
    fn landing_schedule_uses_arrival_mass() {
        let full = compute_v_speeds_at_masses(
            100_000.0,
            100_000.0,
            122.0,
            &sea_level(),
            1.8,
            2.6,
            &PerformanceConfig::default(),
        );
        let arrival = compute_v_speeds_at_masses(
            100_000.0,
            80_000.0,
            122.0,
            &sea_level(),
            1.8,
            2.6,
            &PerformanceConfig::default(),
        );
        assert_eq!(arrival.v_stall_to_ms, full.v_stall_to_ms);
        assert!(arrival.v_stall_land_ms < full.v_stall_land_ms);
        assert!(arrival.v_app_ms < full.v_app_ms);
        assert!(arrival.v_td_ms < full.v_td_ms);
    }

    fn speeds_with(pc: &PerformanceConfig) -> VSpeeds {
        compute_v_speeds(79_000.0, 122.0, &sea_level(), 1.8, 2.6, pc)
    }

    #[test]
    fn v2_is_never_below_the_cs25_107_minimum_for_any_factor_combination() {
        // CS 25.107(b)-(c): V2 >= V2MIN = max(1.13 VSR, 1.10 VMC) and
        // V2 >= VR; (e)(1)(ii): VR >= 1.05 VMC; V1 <= VR <= V2.
        for vmc in [1.0, 1.09, 1.13, 1.2] {
            for v2 in [1.05, 1.13, 1.2, 1.3] {
                for vr_vstall in [1.0, 1.1, 1.15] {
                    for v1 in [0.9, 0.95, 1.0] {
                        let pc = PerformanceConfig {
                            vmc_vstall_factor: vmc,
                            v2_vstall_factor: v2,
                            vr_vstall_factor: vr_vstall,
                            v1_vr_factor: v1,
                            ..PerformanceConfig::default()
                        };
                        let vs = speeds_with(&pc);
                        let eps = 1e-12 * vs.v2_ms;
                        assert!(vs.v2_ms + eps >= 1.13 * vs.v_stall_to_ms);
                        assert!(vs.v2_ms + eps >= 1.10 * vs.v_mc_ms);
                        assert!(vs.v_r_ms + eps >= 1.05 * vs.v_mc_ms);
                        assert!(vs.v1_ms + eps >= vs.v_mc_ms);
                        assert!(vs.v1_ms <= vs.v_r_ms + eps && vs.v_r_ms <= vs.v2_ms + eps);
                    }
                }
            }
        }
    }

    #[test]
    fn the_schedule_agrees_with_the_configuration_speed_ratios() {
        // `PerformanceConfig::takeoff_speed_ratios` is the single statement of
        // the CS 25.107 rule other crates use; this schedule must equal it.
        for legacy in [false, true] {
            for vmc in [1.0, 1.09, 1.13, 1.2] {
                for v2 in [1.05, 1.2, 1.3] {
                    let pc = PerformanceConfig {
                        legacy_field_correlations: legacy,
                        vmc_vstall_factor: vmc,
                        v2_vstall_factor: v2,
                        ..PerformanceConfig::default()
                    };
                    let vs = speeds_with(&pc);
                    let ratios = pc.takeoff_speed_ratios();
                    assert!((vs.v_mc_ms / vs.v_stall_to_ms - ratios.vmc).abs() < 1e-12);
                    assert!((vs.v_r_ms / vs.v_stall_to_ms - ratios.vr).abs() < 1e-12);
                    assert!((vs.v1_ms / vs.v_stall_to_ms - ratios.v1).abs() < 1e-12);
                    assert!((vs.v2_ms / vs.v_stall_to_ms - ratios.v2).abs() < 1e-12);
                }
            }
        }
    }

    #[test]
    fn the_legacy_replay_keeps_the_translated_v2_rule() {
        // Golden replay: V2 = max(1.20 VS, VR), no V2MIN term.
        let pc = PerformanceConfig {
            legacy_field_correlations: true,
            ..PerformanceConfig::default()
        };
        let vs = speeds_with(&pc);
        assert!((vs.v2_ms / vs.v_stall_to_ms - 1.20).abs() < 1e-12);
    }

    #[test]
    fn the_default_vmc_estimate_gives_v2_of_1_243_vsr() {
        // VMC estimate 1.13 VSR (the CS 25.149(c) ceiling): 1.10 * 1.13 =
        // 1.243 exceeds both the selected 1.20 and the 1.13 minimum, and VR
        // = 1.05 * 1.13 = 1.1865 VSR.
        let vs = speeds_with(&PerformanceConfig::default());
        assert!((vs.v2_ms / vs.v_stall_to_ms - 1.243).abs() < 1e-12);
        assert!((vs.v_r_ms / vs.v_stall_to_ms - 1.1865).abs() < 1e-12);
    }

    #[test]
    fn a_selected_factor_above_the_minimum_is_kept() {
        let pc = PerformanceConfig {
            v2_vstall_factor: 1.30,
            ..PerformanceConfig::default()
        };
        let vs = speeds_with(&pc);
        assert!((vs.v2_ms / vs.v_stall_to_ms - 1.30).abs() < 1e-12);
    }

    #[test]
    fn the_four_engine_turboprop_exception_lowers_only_the_stall_floor() {
        // 1.08 VSR (CS 25.107(b)(2)): with a low VMC estimate the selected
        // factor governs; the 1.10 VMC floor still applies.
        let pc = PerformanceConfig {
            v2_min_vsr_factor: 1.08,
            v2_vstall_factor: 1.08,
            vmc_vstall_factor: 1.0,
            vr_vstall_factor: 1.0,
            ..PerformanceConfig::default()
        };
        let vs = speeds_with(&pc);
        assert!((vs.v2_ms / vs.v_stall_to_ms - 1.10).abs() < 1e-12);
    }

    #[test]
    fn hand_case_with_a_60_ms_stall_speed() {
        // Choose the wing loading so VS = 60 m/s exactly: W/S = 0.5 rho VS^2 CL.
        let sigma_rho = 1.225 * density_ratio(0.0, 0.0);
        let cl_max = 1.8;
        let ws = 0.5 * sigma_rho * 60.0 * 60.0 * cl_max;
        let area = 100.0;
        let mass = ws * area / G;
        let vs = compute_v_speeds(
            mass,
            area,
            &sea_level(),
            cl_max,
            2.6,
            &PerformanceConfig::default(),
        );
        assert!((vs.v_stall_to_ms - 60.0).abs() < 1e-9);
        assert!((vs.v2_ms - 74.58).abs() < 1e-9);
        assert!((vs.v_r_ms - 71.19).abs() < 1e-9);
        assert!((vs.v_mc_ms - 67.8).abs() < 1e-9);
    }
}
