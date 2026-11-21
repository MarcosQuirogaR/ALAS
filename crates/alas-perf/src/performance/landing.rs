// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The one landing field-length definition shared by the matching chart, the
//! optimizer residual, the delivery gate and the reported LFL.
//!
//! Certified landing field length = 14 CFR/CS 25.125 demonstrated distance
//! from 50 ft divided by 0.6 (dry runway, jets). The model's actual distance
//! is the segment model of [`LandingSegmentModel`]; the frozen translated
//! replay keeps `k_land (W/S) / (sigma CLmax_land)` with the unfactored
//! `k_land` [m/Pa].

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

/// Mean braked-run deceleration over g of a jet transport with automatic
/// ground spoilers and anti-skid on two-wheel main trucks, Torenbeek, Advanced
/// Aircraft Design (2013), Sec. 9.5.1, p. 274 (statistics of ESDU 84040, no reverse
/// thrust).
pub const JET_DECELERATION_TWO_WHEEL_TRUCKS_G: f64 = 0.40;

/// As above for four-wheel (or larger) main trucks, p. 274.
pub const JET_DECELERATION_FOUR_WHEEL_TRUCKS_G: f64 = 0.32;

/// Time from main-gear touchdown to the full braking configuration used for AFM
/// data expansion: the longer of 1 s or the test time for the first deceleration
/// device, and again for the next one, until every device is active (FAA AC 25-7D,
/// 4.11.7.2 and Fig. 4-19; FAA AC 25-32, 8.3.3-8.3.4). Manual ground spoilers and
/// then wheel brakes are two devices: 2 s. Automatic devices (autobrakes,
/// auto-spoilers) may use the certification test time without the 1 s minimum
/// (AC 25-7D 4.11.7.3, AC 25-32 8.3.5); a per-type test time is applied only where
/// it is published.
pub const LANDING_TRANSITION_TIME_S: f64 = 2.0;

/// Approach speed at which the jet mean deceleration (Torenbeek 2013 p. 274) is
/// converted to a full-braking deceleration plus the transition segment: the
/// touchdown-to-stop ground distance is the same at this speed. The sample behind the
/// ESDU 84040 statistics is not printed, so this is a documented assumption: 70 m/s
/// (136 kt), the middle of the approach speeds of the jet-transport landing categories
/// C and D (ICAO Doc 8168 PANS-OPS Vol. I, Part I, Sec. 4, Ch. 1, Table I-4-1-1: Vat
/// 121-165 kt, 62-85 m/s) and of the 60-80 m/s the Eq. 9.25 check of this crate runs
/// over. The ground distance moves by `t V (1 - V / V0)` away from it, about 1 % per
/// 5 m/s. Replace it if the ESDU sample is found.
pub const TORENBEEK_STATISTIC_APPROACH_SPEED_M_S: f64 = 70.0;

/// The same conversion speed for the turboprop mean deceleration (Torenbeek,
/// Synthesis of Subsonic Airplane Design, 1982, Sec. 5.4.6, p. 170: "mean
/// deceleration, taking into account an equivalent inertia time", .35 to .45 g with
/// propeller reverse thrust inoperative, estimated on the unfactored distances of
/// actual aircraft in Fig. 5-17). By the rule of
/// [`TORENBEEK_STATISTIC_APPROACH_SPEED_M_S`] it is the middle of the approach
/// category of the class, B for the transport turboprops (same ICAO table: Vat
/// 91-120 kt): 105.5 kt.
pub const TORENBEEK_TURBOPROP_STATISTIC_APPROACH_SPEED_M_S: f64 = 105.5 * KNOT_M_S;

const KNOT_M_S: f64 = 1852.0 / 3600.0;
const FOOT_M: f64 = 0.3048;

/// Speed lost between the 50 ft screen and touchdown, m/s: 3 kt, the minimum speed
/// loss of past part 25 certifications, FAA AC 25-7D (4 May 2018), 4.11.2.1.1,
/// "Touchdown Speed = VREF - 3 Knots". It is the pair of
/// [`certification_air_distance_m`].
pub const LANDING_AIR_SPEED_LOSS_M_S: f64 = 3.0 * KNOT_M_S;

/// Airborne distance from the 50 ft screen to touchdown of the 14 CFR/CS 25.125
/// landing distance, m, for a reference landing speed (true, m/s): FAA AC 25-7D
/// (4 May 2018), 4.11.2.1.1,
///
/// ```text
/// air distance [ft] = 1.55 (VREF - 80)^1.35 + 800 ,  VREF in knots TAS
/// ```
///
/// "an upper bound to the part 25 zero-wind airborne distances achieved in past
/// certifications", which 4.11.2.1.2 accepts in place of a measured air distance.
/// The parametric example of 4.11.8 (-3.5 deg, 8 ft/s at touchdown, 220 ft/s) gives
/// 1,100 ft against 1,105 ft from this relation. It replaces the energy form with a
/// mean `(D - T)/W` of 0.10, a value Torenbeek (1982, p. 170) calls difficult to
/// calculate, which flew 250 to 365 m where this relation gives 280 to 410 m.
///
/// Validity: part 25 landings, VREF from 80 kt up (the relation is not defined
/// below; the 800 ft is kept there).
pub fn certification_air_distance_m(v_ref_m_s: f64) -> f64 {
    let excess_kt = (v_ref_m_s / KNOT_M_S - 80.0).max(0.0);
    (1.55 * excess_kt.powf(1.35) + 800.0) * FOOT_M
}

/// Touchdown speed for a reference landing speed, m/s (AC 25-7D 4.11.2.1.1).
fn touchdown_speed_m_s(v_ref_m_s: f64) -> f64 {
    (v_ref_m_s - LANDING_AIR_SPEED_LOSS_M_S).max(0.0)
}

/// Full-braking deceleration over g from a mean touchdown-to-stop deceleration over
/// g that includes the free roll: equal ground distance at the touchdown speed of the
/// approach speed `statistic_approach_speed_m_s` for a transition of
/// [`LANDING_TRANSITION_TIME_S`], `V^2 / (2 a_av) = t V + V^2 / (2 a_fb)`.
pub fn full_braking_deceleration_g(
    mean_deceleration_g: f64,
    statistic_approach_speed_m_s: f64,
) -> f64 {
    let v_td = touchdown_speed_m_s(statistic_approach_speed_m_s);
    mean_deceleration_g
        / (1.0 - 2.0 * mean_deceleration_g * super::G * LANDING_TRANSITION_TIME_S / v_td)
}

/// Mean braked-run deceleration over g selected by the declared main-gear
/// truck: two wheels 0.40 g, four or more (or undeclared, `<= 0`) 0.32 g.
pub fn jet_landing_deceleration_g(main_gear_truck_wheels: i64) -> f64 {
    if (1..=2).contains(&main_gear_truck_wheels) {
        JET_DECELERATION_TWO_WHEEL_TRUCKS_G
    } else {
        JET_DECELERATION_FOUR_WHEEL_TRUCKS_G
    }
}

/// The 25.125 landing distance from 50 ft as the certification segments of FAA
/// AC 25-7D 4.11.1.1: the airborne distance, the transition and the full braking,
///
/// ```text
/// L_air = certification_air_distance_m(V_ref) ,  V_TD = V_ref - 3 kt
/// L_GR  = t V_TD + V_TD^2 / (2 a_fb)
/// ```
///
/// with no reverse thrust. The class decelerations are published mean values from
/// touchdown to standstill; [`full_braking_deceleration_g`] restates them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LandingSegmentModel {
    /// Braked-run deceleration divided by g: the mean from touchdown with no
    /// transition segment, the full-braking value with one.
    pub deceleration_g: f64,
    /// Transition from touchdown to the full braking configuration, at the
    /// touchdown speed, s; zero when `deceleration_g` is a mean that includes
    /// the free roll.
    pub transition_time_s: f64,
}

impl LandingSegmentModel {
    /// Jet model for a declared main-gear truck, see [`jet_landing_deceleration_g`].
    pub fn jet(main_gear_truck_wheels: i64) -> Self {
        Self {
            deceleration_g: full_braking_deceleration_g(
                jet_landing_deceleration_g(main_gear_truck_wheels),
                TORENBEEK_STATISTIC_APPROACH_SPEED_M_S,
            ),
            transition_time_s: LANDING_TRANSITION_TIME_S,
        }
    }

    /// Turboprop model for a mean deceleration over g with propeller reverse thrust
    /// inoperative (Torenbeek 1982, p. 170).
    pub fn turboprop(mean_deceleration_g: f64) -> Self {
        Self {
            deceleration_g: full_braking_deceleration_g(
                mean_deceleration_g,
                TORENBEEK_TURBOPROP_STATISTIC_APPROACH_SPEED_M_S,
            ),
            transition_time_s: LANDING_TRANSITION_TIME_S,
        }
    }

    /// Airborne distance from the screen to touchdown, m.
    pub fn air_distance_m(&self, v_app_m_s: f64) -> f64 {
        certification_air_distance_m(v_app_m_s)
    }

    /// Ground distance from touchdown to standstill, m: the transition at the
    /// touchdown speed, then the full braking.
    pub fn ground_distance_m(&self, v_app_m_s: f64) -> f64 {
        let v_td = touchdown_speed_m_s(v_app_m_s);
        self.transition_time_s * v_td + v_td * v_td / (2.0 * super::G * self.deceleration_g)
    }

    /// Actual landing distance from 50 ft for an approach (true) speed, m.
    pub fn distance_m(&self, v_app_m_s: f64) -> f64 {
        self.air_distance_m(v_app_m_s) + self.ground_distance_m(v_app_m_s)
    }

    /// Largest approach (true) speed whose distance fits `distance_m`; zero
    /// when the airborne distance alone exceeds it. The distance rises
    /// monotonically with the speed, so the root is bracketed and bisected.
    pub fn max_approach_speed_m_s(&self, distance_m: f64) -> f64 {
        if distance_m.is_nan() || distance_m <= self.distance_m(0.0) {
            return 0.0;
        }
        let (mut low, mut high) = (0.0_f64, 1.0_f64);
        while self.distance_m(high) < distance_m && high < 1.0e6 {
            low = high;
            high *= 2.0;
        }
        // Halving to the last representable step of the bracket.
        for _ in 0..200 {
            let mid = 0.5 * (low + high);
            if mid <= low || mid >= high {
                break;
            }
            if self.distance_m(mid) < distance_m {
                low = mid;
            } else {
                high = mid;
            }
        }
        0.5 * (low + high)
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

/// The landing segment model the field stage flies (`field::compute_field_performance_for_propulsion`):
/// the turboprop model at the configured mean deceleration for a propeller
/// aircraft, the jet model of the declared main-gear truck otherwise.
pub fn landing_segment_model_for(
    performance: &alas_config::PerformanceConfig,
    propeller: bool,
) -> LandingSegmentModel {
    if propeller {
        LandingSegmentModel::turboprop(performance.propeller_landing_deceleration_g)
    } else {
        LandingSegmentModel::jet(performance.main_gear_truck_wheels)
    }
}

/// Dry-runway share of the landing distance available the actual landing distance
/// may use: the configured propeller share (0.70, CAT.POL.A.230(a)(2)) for a
/// propeller aircraft outside the translated replay, [`JET_DRY_LANDING_SHARE`]
/// otherwise. The same rule as the reported landing field length.
pub fn dry_landing_share_for(performance: &alas_config::PerformanceConfig, propeller: bool) -> f64 {
    if propeller && !performance.legacy_field_correlations {
        performance.propeller_dry_landing_distance_share
    } else {
        JET_DRY_LANDING_SHARE
    }
}

/// Maximum wing loading [Pa] for the configured landing-distance method:
/// the segment model of the aircraft's propulsion
/// ([`landing_segment_model_for`]), or the `k_land` correlation of the frozen
/// translated replay (`legacy_field_correlations`). One definition for the
/// matching chart, the optimizer residual and the delivery gate, and the same
/// model as the reported landing distance: a turboprop was limited by the jet
/// braking model before.
pub fn ws_landing_limit_for(
    performance: &alas_config::PerformanceConfig,
    propeller: bool,
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
            &landing_segment_model_for(performance, propeller),
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
    fn the_air_distance_is_the_ac_25_7d_relation() {
        // AC 25-7D 4.11.2.1.1 at 130 kt: 1.55 * 50^1.35 + 800 = 1,105 ft; the
        // parametric example of 4.11.8 prints 1,100 ft at 220 ft/s (130.3 kt).
        let at_130_kt = certification_air_distance_m(130.0 * KNOT_M_S) / FOOT_M;
        assert!((at_130_kt - 1_104.8).abs() < 0.5, "{at_130_kt}");
        let example = certification_air_distance_m(220.0 * FOOT_M) / FOOT_M;
        assert!((example / 1_100.0 - 1.0).abs() < 0.01, "{example}");
        // Not defined below 80 kt: the 800 ft is kept, with a continuous slope.
        for v_kt in [0.0, 40.0, 80.0] {
            assert_eq!(
                certification_air_distance_m(v_kt * KNOT_M_S),
                800.0 * FOOT_M
            );
        }
        let just_above = certification_air_distance_m(80.5 * KNOT_M_S) / FOOT_M;
        assert!(just_above > 800.0 && just_above < 801.0, "{just_above}");
        // The speed loss is 3 kt.
        let v = 136.0 * KNOT_M_S;
        assert!((touchdown_speed_m_s(v) / KNOT_M_S - 133.0).abs() < 1e-9);
    }

    #[test]
    fn the_model_is_longer_than_torenbeek_eq_9_25_by_the_air_distance() {
        // Eq. 9.25: factored LFL = 250 m + V^2/(2g) (1.56 / (a/g) + 1.07), with a
        // glide of 10 from the screen. The certification air distance is 35 to 50 m
        // longer over these speeds and the touchdown 3 kt below Vref is faster than
        // the 0.9675 V of the flare of Eq. 9.23, so the model is 2 to 10 % longer.
        for (wheels, a_g) in [(2, 0.40), (4, 0.32)] {
            let model = LandingSegmentModel::jet(wheels);
            assert_eq!(
                model.deceleration_g,
                full_braking_deceleration_g(a_g, TORENBEEK_STATISTIC_APPROACH_SPEED_M_S)
            );
            assert!(model.deceleration_g > a_g);
            let printed = |v: f64| 250.0 + v * v / (2.0 * 9.81) * (1.56 / a_g + 1.07);
            for v in [60.0_f64, 70.0, 80.0] {
                let ratio = model.distance_m(v) / JET_DRY_LANDING_SHARE / printed(v);
                assert!(ratio > 1.02 && ratio < 1.10, "{wheels} {v} {ratio}");
            }
        }
    }

    #[test]
    fn the_transition_gives_the_full_braking_deceleration_for_equal_ground_distance() {
        // V^2 / (2 a_av) = t V + V^2 / (2 a_fb) at the touchdown speed of V0.
        for (a_av_g, v0) in [
            (0.32, TORENBEEK_STATISTIC_APPROACH_SPEED_M_S),
            (0.40, TORENBEEK_STATISTIC_APPROACH_SPEED_M_S),
            (0.40, TORENBEEK_TURBOPROP_STATISTIC_APPROACH_SPEED_M_S),
        ] {
            let a_fb_g = full_braking_deceleration_g(a_av_g, v0);
            let v_td = touchdown_speed_m_s(v0);
            let mean = v_td * v_td / (2.0 * a_av_g * crate::performance::G);
            let split = LANDING_TRANSITION_TIME_S * v_td
                + v_td * v_td / (2.0 * a_fb_g * crate::performance::G);
            assert!((mean - split).abs() < 1e-9, "{mean} {split}");
        }
        // The turboprop statistic is converted at its own class speed (category B):
        // the same mean deceleration is a higher full-braking value at a lower speed.
        let turboprop = LandingSegmentModel::turboprop(0.40);
        let model = LandingSegmentModel {
            deceleration_g: 0.40,
            transition_time_s: 0.0,
        };
        let v0 = TORENBEEK_TURBOPROP_STATISTIC_APPROACH_SPEED_M_S;
        assert!((turboprop.ground_distance_m(v0) - model.ground_distance_m(v0)).abs() < 1e-9);
        assert!(turboprop.deceleration_g > LandingSegmentModel::jet(2).deceleration_g);
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
        let v_app = k_app * (2.0 * ws / (crate::performance::RHO_SL * sigma * cl)).sqrt();
        assert!((model.distance_m(v_app) - 2_500.0 * JET_DRY_LANDING_SHARE).abs() < 1e-6);
        // A distance shorter than the screen term leaves no wing loading.
        assert_eq!(
            ws_landing_segment_limit(&model, 100.0, 0.6, sigma, cl, k_app),
            0.0
        );
    }
}
