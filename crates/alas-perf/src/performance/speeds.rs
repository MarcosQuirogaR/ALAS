// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/performance.py
// Reference: alas @ rust-port-baseline.

//! The FAR-25 V-speed schedule and the estimated field-performance distances.

use alas_config::airports::Airport;
use alas_config::PerformanceConfig;

use super::{density_ratio, G, M_TO_FT, PA_TO_PSF, RHO_SL};

/// Reference speeds [m/s] for one aircraft + aerodrome combination:
/// `VSpeeds`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VSpeeds {
    /// Stall speed in take-off configuration (1 g).
    pub v_stall_to_ms: f64,
    /// Stall speed in landing configuration (1 g).
    pub v_stall_land_ms: f64,
    /// Minimum control speed (FAR 25.149).
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
    /// Approach speed (FAR 25.125).
    pub v_app_ms: f64,
    /// Touchdown speed.
    pub v_td_ms: f64,
}

/// Compute the FAR-25 V-speed schedule at the given aerodrome conditions:
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

/// Compute the certified speed schedule with separate take-off and landing
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

    let v_mc = pc.vmc_vstall_factor * v_stall_to;
    let v_r = (pc.vr_vmc_factor * v_mc).max(pc.vr_vstall_factor * v_stall_to);
    let v1 = (pc.v1_vr_factor * v_r).max(v_mc);
    let v2 = (pc.v2_vstall_factor * v_stall_to).max(v_r);
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
/// are read from the same source the sizing used, exactly as upstream's
/// dataclass carries the `Airport` and exposes the margins as properties.
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
    /// Accelerate-stop distance, m.
    pub asd_m: f64,
    /// Landing distance required, m.
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

    /// Runway to spare on landing, m.
    pub fn land_margin_m(&self) -> f64 {
        self.lda_m() - self.ldr_m
    }

    /// Whether the take-off fits the runway.
    pub fn to_feasible(&self) -> bool {
        self.todr_m <= self.toda_m()
    }

    /// Whether the landing fits the runway.
    pub fn land_feasible(&self) -> bool {
        self.ldr_m <= self.lda_m()
    }
}

/// Estimate take-off and landing distances: `compute_field_performance`.
///
/// Uses the same empirical constants as the constraint curves so the distances
/// are consistent with the design-space boundary. `tw_sl` is the sea-level
/// static `T/W` at MTOW; `bfl_factor` turns take-off distance into balanced
/// field length (Raymer Table 17.1). The take-off constant `37.7` and the
/// landing factor are the reverse of the matching-chart constraints.
#[allow(clippy::too_many_arguments)] // mirrors upstream's own signature
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

/// Estimate field performance with distinct take-off and landing masses.
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

    let toda_req_ft = 37.7 * ws_psf / (sigma * cl_max_to * tw_sl.max(1e-6));
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
}
