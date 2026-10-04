// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Longitudinal ground-attitude checks: tip-back (longitudinal tip-over),
//! tail-scrape clearance at rotation, and the dynamic nose-gear braking
//! reaction (CS/14 CFR 25.733).
//!
//! These are pure functions of geometry and load, independent of
//! [`super::size_landing_gear_with_group_stations`], so a caller (the CG-
//! envelope gate, a report figure) can evaluate them at whichever loading
//! state and fuselage geometry it already has, without this crate needing to
//! own the fuselage lower-contour representation.

/// Tip-back (longitudinal tip-over) angle, degrees, measured from the
/// vertical at the static ground line.
///
/// `theta_tipback = atan((x_mlg_aft_axle - x_cg_aft) / h_cg)`: the angle the
/// aircraft would have to rotate about the main-gear axle, nose up, before
/// the centre of gravity crosses over the main-gear contact point and the
/// aircraft tips onto its tail. `h_cg` is the centre-of-gravity height above
/// the static ground line (Raymer, *Aircraft Design*, ch. 11; the same
/// convention as the lateral turnover check in
/// [`super::size_landing_gear_with_group_stations`]).
///
/// `x_mlg_aft_axle` is the most-aft main-gear axle station (the pivot the
/// aircraft would rotate about); `x_cg_aft` is the most-aft centre-of-gravity
/// station across the loading states being checked. Both are in the same
/// nose-tip-positive-aft frame. A larger angle is safer (more nose-up
/// rotation available before tip-back); the aircraft requirement is
/// `tip_back_angle_deg >= max(min_tip_back_deg, tail_scrape_angle_deg)`.
///
/// Returns `f64::NAN` if `h_cg` is not finite and positive: a tip-back angle
/// is only defined above the ground line, and a caller must not treat NaN as
/// zero (which would silently read as a tip-back failure).
#[must_use]
pub fn tip_back_angle_deg(x_mlg_aft_axle_m: f64, x_cg_aft_m: f64, h_cg_m: f64) -> f64 {
    if !h_cg_m.is_finite() || h_cg_m <= 0.0 {
        return f64::NAN;
    }
    ((x_mlg_aft_axle_m - x_cg_aft_m) / h_cg_m)
        .atan()
        .to_degrees()
}

/// One point of the fuselage's lower outer contour, aft-of-nose station and
/// belly height, both metres in the same ground-referenced frame the gear
/// stations use.
///
/// The lower contour at a cross-section is `z_c - height/2` for a fuselage
/// modelled as elliptical/rounded cross-sections about a centreline `z_c`
/// (matching `alas-mass::stations`'s own fuselage datum convention); callers
/// build this slice from whichever built-geometry representation they hold.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FuselageLowerPoint {
    /// Longitudinal station, m (nose-tip frame, positive aft).
    pub x_m: f64,
    /// Fuselage lower-contour height at this station, m (same frame as the
    /// gear ground line, e.g. `z_c - height/2`).
    pub z_bottom_m: f64,
}

/// Tail-scrape angle, degrees: the smallest angle, over every fuselage
/// lower-contour point aft of the main gear, that the tail would need to
/// rotate through nose-up before scraping the ground.
///
/// `scrape_angle = min_{x > x_mlg} atan((z_bottom(x) - z_ground) / (x -
/// x_mlg))`. This is the geometric ceiling on rotation: the aircraft cannot
/// pitch up past this angle (about the main-gear contact) without the tail
/// (or any other aft lower-fuselage point) touching the runway first.
///
/// `points` need not be pre-filtered to aft-of-gear stations: points at or
/// ahead of `x_mlg_m` (`x - x_mlg <= 0`) are skipped, since they are not on
/// the arc the tail sweeps through during rotation.
///
/// Returns `None` if no supplied point lies strictly aft of the main gear,
/// or if `x_mlg_m`/`ground_z_m` are not finite: there is then no scrape
/// constraint to report, which is different from an unconstrained (infinite)
/// angle a caller might otherwise be tempted to invent.
#[must_use]
pub fn tail_scrape_angle_deg(
    points: &[FuselageLowerPoint],
    x_mlg_m: f64,
    ground_z_m: f64,
) -> Option<f64> {
    if !x_mlg_m.is_finite() || !ground_z_m.is_finite() {
        return None;
    }
    points
        .iter()
        .filter(|point| point.x_m.is_finite() && point.z_bottom_m.is_finite())
        .filter_map(|point| {
            let dx = point.x_m - x_mlg_m;
            if dx <= 0.0 {
                return None;
            }
            let dz = point.z_bottom_m - ground_z_m;
            Some((dz / dx).atan().to_degrees())
        })
        .reduce(f64::min)
}

/// Dynamic nose-gear braking reaction, kg-force equivalent (CS/14 CFR
/// 25.733; Raymer Sec. 11.2, "Nose Gear Braking Load").
///
/// `N_dyn = W (l_m + (a/g) h_cg) / B`, where `l_m` is the distance from the
/// centre of gravity to the main-gear axle (the same lever arm the static
/// nose reaction uses), `B` is the wheelbase, `h_cg` is the centre-of-gravity
/// height above the static ground line, and `a` is the certification
/// deceleration. Retrieved text, 14 CFR 25.733(b)(2) (CS-25 recalled as
/// substantively identical): the braking case combines 1.0g down with 0.31g
/// forward, up to maximum landing weight, capped at 1.5x the tire's static
/// rating. `decel_g` is `a/g` (dimensionless): the default configuration
/// value is 0.31; a caller may override it for a different load case (e.g.
/// 14 CFR 25.733(b)(3)'s 0.20g at maximum ramp weight).
///
/// Returns 0.0 (not negative) if the computed reaction would be negative
/// (e.g. a non-finite or degenerate input): a braking reaction cannot unload
/// the nose gear below its static value in this simplified two-point model.
#[must_use]
pub fn dynamic_nose_braking_load_kg(
    weight_kg: f64,
    l_m_to_main_gear_m: f64,
    wheelbase_m: f64,
    h_cg_m: f64,
    decel_g: f64,
) -> f64 {
    if !weight_kg.is_finite()
        || !l_m_to_main_gear_m.is_finite()
        || !wheelbase_m.is_finite()
        || wheelbase_m <= 0.0
        || !h_cg_m.is_finite()
        || !decel_g.is_finite()
    {
        return 0.0;
    }
    (weight_kg * (l_m_to_main_gear_m + decel_g * h_cg_m) / wheelbase_m).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tip_back_angle_matches_hand_calculation() {
        // atan(5/3) = 59.0362... deg.
        let angle = tip_back_angle_deg(40.0, 35.0, 3.0);
        assert!((angle - 59.036_243_5).abs() < 1e-6);
    }

    #[test]
    fn tip_back_angle_shrinks_as_the_cg_moves_aft_toward_the_gear() {
        let far = tip_back_angle_deg(40.0, 30.0, 3.0);
        let near = tip_back_angle_deg(40.0, 38.0, 3.0);
        assert!(near < far);
    }

    #[test]
    fn tip_back_angle_is_nan_for_a_non_physical_cg_height() {
        assert!(tip_back_angle_deg(40.0, 35.0, 0.0).is_nan());
        assert!(tip_back_angle_deg(40.0, 35.0, -1.0).is_nan());
        assert!(tip_back_angle_deg(40.0, 35.0, f64::NAN).is_nan());
    }

    #[test]
    fn tail_scrape_angle_matches_hand_calculation_and_takes_the_minimum() {
        // Main gear at x=40, ground at z=-2.0.
        // Point A: x=42, z_bottom=-1.0 -> atan(1.0/2.0) = 26.565 deg.
        // Point B (further aft, lower clearance): x=45, z_bottom=-1.7 ->
        // atan(0.3/5.0) = 3.434 deg -- this is the governing (smallest) case.
        let points = [
            FuselageLowerPoint {
                x_m: 42.0,
                z_bottom_m: -1.0,
            },
            FuselageLowerPoint {
                x_m: 45.0,
                z_bottom_m: -1.7,
            },
        ];
        let angle = tail_scrape_angle_deg(&points, 40.0, -2.0).expect("aft points exist");
        // atan(0.3/5.0) = atan(0.06) = 3.43363... deg.
        assert!((angle - 3.433_63).abs() < 1e-3);
    }

    #[test]
    fn tail_scrape_angle_ignores_points_at_or_ahead_of_the_main_gear() {
        let points = [
            FuselageLowerPoint {
                x_m: 40.0,
                z_bottom_m: -1.0,
            },
            FuselageLowerPoint {
                x_m: 38.0,
                z_bottom_m: -1.9,
            },
        ];
        assert_eq!(tail_scrape_angle_deg(&points, 40.0, -2.0), None);
    }

    #[test]
    fn tail_scrape_angle_is_none_with_no_aft_points_or_bad_inputs() {
        assert_eq!(tail_scrape_angle_deg(&[], 40.0, -2.0), None);
        let points = [FuselageLowerPoint {
            x_m: 45.0,
            z_bottom_m: -1.7,
        }];
        assert_eq!(tail_scrape_angle_deg(&points, f64::NAN, -2.0), None);
    }

    #[test]
    fn dynamic_nose_braking_load_matches_hand_calculation() {
        // N_dyn = 100,000 * (8 + 0.3*2) / 10 = 86,000 kg.
        let load = dynamic_nose_braking_load_kg(100_000.0, 8.0, 10.0, 2.0, 0.3);
        assert!((load - 86_000.0).abs() < 1e-9);
    }

    #[test]
    fn dynamic_nose_braking_load_exceeds_the_equivalent_static_reaction() {
        // The static reaction (no braking term) is W*l_m/B; the dynamic
        // reaction always adds a non-negative deceleration term.
        let static_reaction = 100_000.0_f64 * 8.0 / 10.0;
        let dynamic = dynamic_nose_braking_load_kg(100_000.0, 8.0, 10.0, 3.6, 0.3109);
        assert!(dynamic > static_reaction);
    }

    #[test]
    fn dynamic_nose_braking_load_is_zero_not_negative_for_degenerate_inputs() {
        assert_eq!(
            dynamic_nose_braking_load_kg(100_000.0, 8.0, 0.0, 2.0, 0.3),
            0.0
        );
        assert_eq!(
            dynamic_nose_braking_load_kg(f64::NAN, 8.0, 10.0, 2.0, 0.3),
            0.0
        );
    }
}
