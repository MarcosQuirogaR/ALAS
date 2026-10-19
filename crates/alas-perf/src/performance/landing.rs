// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The one landing field-length definition shared by the matching chart, the
//! optimizer residual, the delivery gate and the reported LFL.
//!
//! Certified landing field length = 14 CFR/CS 25.125 demonstrated distance
//! from 50 ft divided by 0.6 (dry runway, jets). The model's actual distance
//! is `k_land (W/S) / (sigma CLmax_land)` with the unfactored `k_land` [m/Pa].

/// Dry-runway planning share of the certified landing field length for jets:
/// the 25.125 demonstrated (actual) distance from 50 ft is at most 0.6 of the
/// landing field length, 14 CFR 121.195(b) and EASA CAT.POL.A.230(a)(2)
/// (60 % of the LDA). This single share links the unfactored `k_land`
/// distance to every field-length check (matching chart, optimizer residual,
/// delivery gate, reported LFL).
pub const JET_DRY_LANDING_SHARE: f64 = 0.60;

/// Factor on the dry landing distance for a wet runway when the flight manual
/// gives no wet-runway distance: EU Air OPS CAT.POL.A.235(a)(2) requires a
/// landing distance available of at least 115 % of the distance required by
/// CAT.POL.A.230 (Commission Regulation (EU) No 965/2012, Annex IV, Part-CAT;
/// text read in the EASA Easy Access Rules for Air Operations, Revision 24).
/// CS-25 itself has no wet landing distance; the factor is operational.
pub const WET_RUNWAY_LANDING_FACTOR: f64 = 1.15;

/// Usable share of the landing distance available for the actual landing
/// distance: the dry share, divided by [`WET_RUNWAY_LANDING_FACTOR`] on a wet
/// runway (jets 0.60 to 0.5217, turboprops 0.70 to 0.6087). The dry case
/// returns `dry_share` unchanged.
pub fn landing_runway_share(dry_share: f64, wet_runway: bool) -> f64 {
    if wet_runway {
        dry_share / WET_RUNWAY_LANDING_FACTOR
    } else {
        dry_share
    }
}

/// Maximum wing loading [Pa] for an *actual* (unfactored) landing distance
/// of `distance_m`: `ws_landing_limit`.
///
/// ```text
/// (W/S)_max = distance * sigma * CL_max_land / K
/// ```
///
/// `K` is the unfactored coefficient `k_land` [m/Pa]. Callers that hold a
/// runway length (LDA) must convert it first, or use
/// [`ws_landing_field_limit`].
pub fn ws_landing_limit(distance_m: f64, sigma: f64, cl_max_land: f64, k_factor: f64) -> f64 {
    distance_m * sigma * cl_max_land / k_factor
}

/// Maximum wing loading [Pa] that keeps the factored landing field length
/// within the landing distance available: the actual distance may use at most
/// `dry_share` (0.6 for jets, [`JET_DRY_LANDING_SHARE`]) of `lda_m`.
pub fn ws_landing_field_limit(
    lda_m: f64,
    dry_share: f64,
    sigma: f64,
    cl_max_land: f64,
    k_factor: f64,
) -> f64 {
    ws_landing_limit(lda_m * dry_share, sigma, cl_max_land, k_factor)
}

/// Screen height of the 14 CFR/CS 25.125(a) landing distance, m (50 ft).
pub const LANDING_SCREEN_HEIGHT_M: f64 = 15.24;

/// Incremental load factor of the circular flare, Torenbeek, Advanced Aircraft
/// Design (2013), Sec. 9.5.1, p. 274 (value for jet transports).
pub const FLARE_LOAD_FACTOR_INCREMENT: f64 = 0.10;

/// Approach flight-path angle used by the same prediction, rad (p. 274).
pub const LANDING_APPROACH_ANGLE_RAD: f64 = 0.08;

/// Mean braked-run deceleration over g of a jet transport with automatic
/// ground spoilers and anti-skid on two-wheel main trucks, Torenbeek (2013),
/// p. 274 (statistics of ESDU 84040, no reverse thrust).
pub const JET_DECELERATION_TWO_WHEEL_TRUCKS_G: f64 = 0.40;

/// As above for four-wheel (or larger) main trucks, p. 274.
pub const JET_DECELERATION_FOUR_WHEEL_TRUCKS_G: f64 = 0.32;

/// Mean glide parameter W/(D - T) between the screen and touchdown for jet
/// transports, Torenbeek (2013), p. 274; the propeller method keeps its own
/// sourced value (`propeller_landing_mean_drag_to_weight`, Torenbeek 1982).
pub const JET_LANDING_GLIDE_RATIO: f64 = 10.0;

/// Mean braked-run deceleration over g selected by the declared main-gear
/// truck: two wheels 0.40 g, four or more (or undeclared, `<= 0`) 0.32 g.
pub fn jet_landing_deceleration_g(main_gear_truck_wheels: i64) -> f64 {
    if (1..=2).contains(&main_gear_truck_wheels) {
        JET_DECELERATION_TWO_WHEEL_TRUCKS_G
    } else {
        JET_DECELERATION_FOUR_WHEEL_TRUCKS_G
    }
}

/// The 25.125 landing distance from 50 ft as air, flare and braked-run
/// segments (Torenbeek 2013, Eqs. 9.21-9.24):
///
/// ```text
/// L_AB = G (V_app^2 - V_TD^2) / (2 g) + G h_s ,  V_TD = V_app sqrt(1 - gamma^2 / dn)
/// L_GR = V_TD^2 / (2 a_av)
/// ```
///
/// `G = W / (D - T)_av`. The braked-run deceleration `a_av` is the mean from
/// touchdown to standstill, free roll included, with no reverse thrust.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LandingSegmentModel {
    /// Mean glide parameter `W/(D-T)` from the screen to touchdown.
    pub glide_ratio: f64,
    /// Mean braked-run deceleration divided by g.
    pub deceleration_g: f64,
}

impl LandingSegmentModel {
    /// Jet model for a declared main-gear truck, see [`jet_landing_deceleration_g`].
    pub fn jet(main_gear_truck_wheels: i64) -> Self {
        Self {
            glide_ratio: JET_LANDING_GLIDE_RATIO,
            deceleration_g: jet_landing_deceleration_g(main_gear_truck_wheels),
        }
    }

    /// Coefficient of V_app^2 [s^2/m] in the distance.
    fn speed_coefficient(&self) -> f64 {
        let ratio = LANDING_APPROACH_ANGLE_RAD.powi(2) / FLARE_LOAD_FACTOR_INCREMENT;
        ((1.0 - ratio) / self.deceleration_g + ratio * self.glide_ratio) / (2.0 * super::G)
    }

    /// Fixed airborne term `G h_s`, m.
    fn screen_distance_m(&self) -> f64 {
        self.glide_ratio * LANDING_SCREEN_HEIGHT_M
    }

    /// Actual landing distance from 50 ft for an approach (true) speed, m.
    pub fn distance_m(&self, v_app_m_s: f64) -> f64 {
        self.screen_distance_m() + self.speed_coefficient() * v_app_m_s * v_app_m_s
    }

    /// Largest approach (true) speed whose distance fits `distance_m`; zero
    /// when the screen term alone exceeds it.
    pub fn max_approach_speed_m_s(&self, distance_m: f64) -> f64 {
        ((distance_m - self.screen_distance_m()).max(0.0) / self.speed_coefficient()).sqrt()
    }
}

/// Maximum wing loading [Pa] keeping the segment-model landing distance
/// within `dry_share` of `lda_m`, with `V_app = vref_over_vs1g V_S1g` and
/// `V_S1g^2 = 2 (W/S) / (rho0 sigma CLmax)`.
pub fn ws_landing_segment_limit(
    model: &LandingSegmentModel,
    lda_m: f64,
    dry_share: f64,
    sigma: f64,
    cl_max_land: f64,
    vref_over_vs1g: f64,
) -> f64 {
    let v_app = model.max_approach_speed_m_s(lda_m * dry_share);
    0.5 * super::RHO_SL * sigma * cl_max_land * (v_app / vref_over_vs1g).powi(2)
}

/// Maximum wing loading [Pa] for the configured landing-distance method:
/// the segment model, or the `k_land` correlation of the frozen translated
/// replay (`legacy_field_correlations`). One definition for the matching
/// chart, the optimizer residual and the delivery gate.
pub fn ws_landing_limit_for(
    performance: &alas_config::PerformanceConfig,
    lda_m: f64,
    dry_share: f64,
    sigma: f64,
) -> f64 {
    if performance.legacy_field_correlations {
        ws_landing_field_limit(
            lda_m,
            dry_share,
            sigma,
            performance.cl_max_land,
            performance.k_land,
        )
    } else {
        ws_landing_segment_limit(
            &LandingSegmentModel::jet(performance.main_gear_truck_wheels),
            lda_m,
            dry_share,
            sigma,
            performance.cl_max_land,
            super::field::VREF_OVER_VS1G,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wet_share_is_the_dry_share_over_115_percent() {
        // CAT.POL.A.235(a)(2): 0.60 / 1.15 and 0.70 / 1.15.
        assert!((landing_runway_share(0.60, true) - 0.60 / 1.15).abs() < 1e-15);
        assert!((landing_runway_share(0.60, true) - 0.5217391304).abs() < 1e-9);
        assert!((landing_runway_share(0.70, true) - 0.6086956522).abs() < 1e-9);
    }

    #[test]
    fn a_dry_runway_is_bit_identical_to_the_dry_share() {
        for share in [0.6, 0.7, 0.8] {
            assert_eq!(
                landing_runway_share(share, false).to_bits(),
                share.to_bits()
            );
        }
    }

    #[test]
    fn a_wet_runway_cuts_the_wing_loading_limit_by_the_factor() {
        let dry = ws_landing_field_limit(2500.0, JET_DRY_LANDING_SHARE, 1.0, 2.6, 0.505);
        let wet = ws_landing_field_limit(
            2500.0,
            landing_runway_share(JET_DRY_LANDING_SHARE, true),
            1.0,
            2.6,
            0.505,
        );
        assert!((dry / wet - WET_RUNWAY_LANDING_FACTOR).abs() < 1e-12);
    }

    #[test]
    fn the_segment_model_reproduces_torenbeek_eq_9_25_within_its_rounding() {
        // Eq. 9.25: factored LFL = 250 m + V^2/(2g) (1.56 / (a/g) + 1.07); the
        // printed constants round 5/3 * 10 * 15.24 = 254 m, 5/3 * (1 - 0.064) =
        // 1.56 and 5/3 * 0.64 = 1.07.
        for (wheels, a_g) in [(2, 0.40), (4, 0.32)] {
            let model = LandingSegmentModel::jet(wheels);
            assert_eq!(model.deceleration_g, a_g);
            for v in [60.0_f64, 70.0, 80.0] {
                let factored = model.distance_m(v) / JET_DRY_LANDING_SHARE;
                let printed = 250.0 + v * v / (2.0 * 9.81) * (1.56 / a_g + 1.07);
                assert!(
                    (factored / printed - 1.0).abs() < 0.01,
                    "{factored} {printed}"
                );
            }
        }
    }

    #[test]
    fn the_truck_class_selects_the_deceleration() {
        assert_eq!(
            jet_landing_deceleration_g(2),
            JET_DECELERATION_TWO_WHEEL_TRUCKS_G
        );
        for wheels in [0, 4, 6] {
            assert_eq!(
                jet_landing_deceleration_g(wheels),
                JET_DECELERATION_FOUR_WHEEL_TRUCKS_G
            );
        }
        // Fewer braked wheels per tonne is not modelled: a lower deceleration
        // can only lengthen the run.
        let two = LandingSegmentModel::jet(2).distance_m(70.0);
        let four = LandingSegmentModel::jet(4).distance_m(70.0);
        assert!(four > two);
    }

    #[test]
    fn the_limit_inverts_the_distance() {
        let model = LandingSegmentModel::jet(4);
        let (sigma, cl, k_app) = (0.9, 2.5, 1.23);
        let ws = ws_landing_segment_limit(&model, 2_500.0, JET_DRY_LANDING_SHARE, sigma, cl, k_app);
        let v_app = k_app * (2.0 * ws / (1.225 * sigma * cl)).sqrt();
        assert!((model.distance_m(v_app) - 2_500.0 * JET_DRY_LANDING_SHARE).abs() < 1e-6);
        // A distance shorter than the screen term leaves no wing loading.
        assert_eq!(
            ws_landing_segment_limit(&model, 100.0, 0.6, sigma, cl, k_app),
            0.0
        );
    }
}
