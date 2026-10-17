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
}
