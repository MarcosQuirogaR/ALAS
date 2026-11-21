// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Rotation speed, installed thrust and thrust-line height for the
//! longitudinal-force term of the nose-wheel liftoff (rotation) criterion.
//!
//! Frames and units: SI throughout. Heights are meters above the shared
//! ground plane (`alas_mass::stations::ground_plane_z_m`), positive up.
//! Thrust is the all-engine force along body x, positive forward, in
//! newtons. The criterion is evaluated at sea level on a standard (ISA) day.

use alas_config::{AlasConfig, PerformanceConfig};
use alas_geom::aircraft::airplane::Airplane;
use alas_prop::system::PropulsionRating;

use crate::mdo::propulsion::{max_climb_rate_ft_min, PropulsionDeck};

/// ISA sea-level density, kg/m^3 (ICAO Doc 7488, *Manual of the ICAO
/// Standard Atmosphere*, 3rd ed., 1993).
const SEA_LEVEL_DENSITY_KG_M3: f64 = alas_atmo::SEA_LEVEL_DENSITY_ICAO_KG_M3;

/// `V_R / V_S` for the rotation (nose-wheel liftoff) criterion: the stall
/// branch of the rotation-speed floor, `vr_vstall_factor` (1.10 by
/// default; 14 CFR 25.107(e) and FAA AC 25-7D, section 4.2.8.1, place `V_R`
/// no lower than the speeds that give liftoff at or above the minimum
/// unstick and stall margins).
///
/// The forward-CG rotation check belongs at the **lowest** rotation speed
/// the schedule can produce, because the tail's available download scales
/// with dynamic pressure. The minimum-control branch of the field schedule
/// (`vr_vmc_factor * vmc_vstall_factor`) is not used: `vmc_vstall_factor` is
/// the 14 CFR 25.149 regulatory ceiling (`V_MC <= 1.13 V_SR`) applied to
/// every aircraft, not a declared aircraft-specific `V_MCA`, and letting
/// that generic placeholder raise `V_R` would credit tail authority the
/// aircraft may not have. The computed `V_MCA` floors the speed instead, per
/// state ([`rotation_vmc_floor_m_s`], [`rotation_lift_coefficient_at`]).
#[must_use]
pub(super) fn vr_over_vs(performance: &PerformanceConfig) -> f64 {
    performance.vr_vstall_factor
}

/// The weight-support lift coefficient at `V_R`,
/// `CL_R = CL_max,TO / (V_R / V_S)^2`, from [`vr_over_vs`].
#[must_use]
pub(super) fn rotation_lift_coefficient(performance: &PerformanceConfig) -> f64 {
    performance.cl_max_to / vr_over_vs(performance).powi(2).max(1.0e-6)
}

/// The minimum-control floor of the rotation speed, m/s: `vr_vmc_factor`
/// (1.05, CS 25.107(e)(1)(ii)) times the computed sea-level `V_MCA` the CG
/// gate's configuration carries
/// (`landing_gear.rotation_minimum_control_speed_m_s`); `None` without one.
/// `V_MC` is a speed, not a stall ratio: it does not fall with weight, so it
/// raises `V_R` at the light states.
#[must_use]
pub(super) fn rotation_vmc_floor_m_s(config: &AlasConfig) -> Option<f64> {
    config
        .landing_gear
        .rotation_minimum_control_speed_m_s
        .filter(|speed| speed.is_finite() && *speed > 0.0)
        .map(|speed| config.performance.vr_vmc_factor * speed)
        .filter(|speed| speed.is_finite() && *speed > 0.0)
}

/// The weight-support lift coefficient at this weight's own `V_R =
/// max(V_R,stall, floor)`: `cl_r_stall` (the stall branch, weight-free) or,
/// when the floor is faster, `2 W / (rho S V_floor^2)`.
#[must_use]
pub(super) fn rotation_lift_coefficient_at(
    weight_n: f64,
    wing_area_m2: f64,
    cl_r_stall: f64,
    floor_m_s: Option<f64>,
) -> f64 {
    let stall_branch_m_s = rotation_speed_m_s(weight_n, wing_area_m2, cl_r_stall);
    match floor_m_s {
        Some(floor) if floor > stall_branch_m_s => {
            2.0 * weight_n / (SEA_LEVEL_DENSITY_KG_M3 * wing_area_m2 * floor * floor)
        }
        _ => cl_r_stall,
    }
}

/// Rotation speed at sea level, standard day, m/s, from `W = q_R S CL_R`.
#[must_use]
pub(super) fn rotation_speed_m_s(weight_n: f64, wing_area_m2: f64, cl_r: f64) -> f64 {
    (2.0 * weight_n / (SEA_LEVEL_DENSITY_KG_M3 * wing_area_m2 * cl_r)).sqrt()
}

/// All-engine thrust at `V_R` over weight, and the thrust-line height above
/// the ground plane, for one loading state. `NaN` fields mean the thrust was
/// not evaluated and the criterion takes no thrust credit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct RotationThrust {
    /// `T / W` at `V_R`, dimensionless.
    pub(super) thrust_to_weight: f64,
    /// Thrust-line height above the ground plane, m.
    pub(super) thrust_line_height_m: f64,
}

impl RotationThrust {
    pub(super) const NOT_EVALUATED: Self = Self {
        thrust_to_weight: f64::NAN,
        thrust_line_height_m: f64::NAN,
    };
}

/// The bound engine deck and the geometry the per-state thrust needs.
///
/// Thrust is the takeoff/go-around rating of the mission deck
/// ([`PropulsionDeck`]): the turbofan with its speed lapse, the turboprop
/// as propeller thrust from shaft power. The thrust line is the mean
/// nacelle centerline height (the same `z` the mass ledger's propulsion
/// items carry), which for a turboprop is the propeller axis.
pub(super) struct RotationThrustModel {
    deck: Option<PropulsionDeck>,
    thrust_line_height_m: f64,
    wing_area_m2: f64,
    cl_r: f64,
    vr_floor_m_s: Option<f64>,
    gravity_m_s2: f64,
}

impl RotationThrustModel {
    pub(super) fn new(plane: &Airplane, config: &AlasConfig, ground_z_m: f64, cl_r: f64) -> Self {
        let heights_m: Vec<f64> = plane
            .fuselages
            .iter()
            .filter(|body| body.name.contains("Nacelle"))
            .filter_map(|nacelle| nacelle.xsecs.first())
            .map(|inlet| inlet.xyz_c[2] - ground_z_m)
            .collect();
        let thrust_line_height_m = if heights_m.is_empty() {
            f64::NAN
        } else {
            heights_m.iter().sum::<f64>() / heights_m.len() as f64
        };
        let requirements = &config.requirements;
        let deck = PropulsionDeck::from_engine(
            &config.geometry.engine,
            requirements.cruise_mach,
            requirements.cruise_altitude_m,
            max_climb_rate_ft_min(config.mission.profile.initial_climb_rate_m_s),
        )
        .ok();
        Self {
            deck,
            thrust_line_height_m,
            wing_area_m2: plane.wings.first().map_or(f64::NAN, |w| w.reference_area()),
            cl_r,
            vr_floor_m_s: rotation_vmc_floor_m_s(config),
            gravity_m_s2: requirements.gravity_m_s2,
        }
    }

    /// TOGA thrust at this mass's own `V_R` (sea level, ISA), over weight.
    pub(super) fn at_mass(&self, mass_kg: f64) -> RotationThrust {
        let Some(deck) = self.deck.as_ref() else {
            return RotationThrust::NOT_EVALUATED;
        };
        let weight_n = mass_kg * self.gravity_m_s2;
        let vr_m_s = rotation_speed_m_s(weight_n, self.wing_area_m2, self.cl_r)
            .max(self.vr_floor_m_s.unwrap_or(0.0));
        if !vr_m_s.is_finite() || weight_n <= 0.0 || !self.thrust_line_height_m.is_finite() {
            return RotationThrust::NOT_EVALUATED;
        }
        let thrust_n = deck
            .flight_condition(0.0, vr_m_s, self.gravity_m_s2, 0.0)
            .and_then(|flight| deck.rated_point(flight, PropulsionRating::TakeoffGoAround))
            .map(|point| point.thrust_n);
        match thrust_n {
            Ok(thrust_n) if thrust_n.is_finite() && thrust_n >= 0.0 => RotationThrust {
                thrust_to_weight: thrust_n / weight_n,
                thrust_line_height_m: self.thrust_line_height_m,
            },
            _ => RotationThrust::NOT_EVALUATED,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rotation_criterion_uses_the_stall_branch_of_the_default_schedule() {
        let performance = PerformanceConfig::default();
        assert!((vr_over_vs(&performance) - 1.10).abs() < 1.0e-12);
        let expected = performance.cl_max_to / 1.10_f64.powi(2);
        assert!((rotation_lift_coefficient(&performance) - expected).abs() < 1.0e-12);
    }

    #[test]
    fn the_generic_minimum_control_placeholder_does_not_raise_the_rotation_speed() {
        let performance = PerformanceConfig {
            vr_vmc_factor: 1.10,
            vmc_vstall_factor: 1.20,
            vr_vstall_factor: 1.08,
            ..PerformanceConfig::default()
        };
        assert!((vr_over_vs(&performance) - 1.08).abs() < 1.0e-12);
    }

    /// The minimum-control floor raises the light weights' rotation speed and
    /// leaves the heavy ones on the stall branch.
    #[test]
    fn the_minimum_control_floor_lowers_the_rotation_lift_only_where_it_is_faster() {
        let (area_m2, cl_r) = (61.0, 1.79);
        let heavy = 23_000.0 * 9.80665;
        let light = 14_000.0 * 9.80665;
        let floor = rotation_speed_m_s(0.5 * (heavy + light), area_m2, cl_r);
        assert_eq!(rotation_lift_coefficient_at(heavy, area_m2, cl_r, Some(floor)), cl_r);
        let floored = rotation_lift_coefficient_at(light, area_m2, cl_r, Some(floor));
        assert!(floored < cl_r);
        assert!((rotation_speed_m_s(light, area_m2, floored) - floor).abs() < 1.0e-9);
        assert_eq!(rotation_lift_coefficient_at(light, area_m2, cl_r, None), cl_r);
    }

    #[test]
    fn the_rotation_speed_closes_the_weight_support_balance() {
        let (weight_n, area_m2, cl_r) = (700_000.0, 122.6, 1.7);
        let v = rotation_speed_m_s(weight_n, area_m2, cl_r);
        let lift_n = 0.5 * SEA_LEVEL_DENSITY_KG_M3 * v * v * area_m2 * cl_r;
        assert!((lift_n - weight_n).abs() < 1.0e-6 * weight_n);
    }
}
