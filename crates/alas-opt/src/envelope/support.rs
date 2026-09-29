// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Small pure helpers that `assess_model_cg_envelope` needs for the physical
//! CG-limits helper.

use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::Fuselage;
use alas_mass::breakdown::{MassBreakdown, MassCoordinates};

use alas_config::AlasConfig;

/// Wing-body aerodynamic centre used by the scissor-plot forward-limit
/// estimate, as a fraction of MAC aft of LEMAC: the quarter-chord point,
/// absent a fuselage-corrected wing-body AC of its own (an estimate).
pub(super) const SCISSOR_X_AC_WB_FRAC: f64 = 0.25;
/// Landing-configuration wing-body pitching moment about its own AC used by
/// the scissor-plot estimate, dimensionless: a documented flap-increment
/// estimate (Torenbeek order of magnitude for a single-slotted flap,
/// nose-down), not a measured value.
pub(super) const SCISSOR_CM_AC_WB_LANDING: f64 = -0.20;
/// Tail dynamic-pressure efficiency used by the scissor-plot estimate,
/// dimensionless: a declared estimate (Torenbeek order of magnitude for a
/// low-mounted tail), not measured.
pub(super) const SCISSOR_ETA: f64 = 0.9;
/// Maximum (most negative) adjustable-stabiliser tail lift coefficient used
/// by the scissor-plot estimate: a declared estimate (Torenbeek), not
/// measured.
pub(super) const SCISSOR_CL_H_MAX: f64 = -0.8;

/// Most negative horizontal-tail lift coefficient available *at rotation*,
/// dimensionless: the stabiliser is set to the takeoff trim position, so
/// only the elevator adds download, and the tail sits in the wing's ground
/// downwash. Torenbeek (Synthesis, ch. 9) and Obert (Aerodynamic Design of
/// Transport Aircraft, ch. 34) place the elevator-only tail lift increment
/// at rotation around -0.5 to -0.6 for transports, well below the
/// full-authority adjustable-stabiliser value ([`SCISSOR_CL_H_MAX`], -0.8)
/// used for landing trim. A declared estimate, not measured.
pub(super) const ROTATION_CL_H: f64 = -0.55;

/// Wing-body pitching moment about its own AC in the *takeoff* flap/slat
/// configuration, dimensionless, used by the rotation (nose-wheel liftoff)
/// forward-limit criterion. A documented Torenbeek
/// order-of-magnitude estimate for a single/double-slotted takeoff flap
/// setting: less flap deflection than landing, so a smaller-magnitude
/// nose-down increment than [`SCISSOR_CM_AC_WB_LANDING`], not a measured
/// value.
pub(super) const ROTATION_CM_AC_WB_TAKEOFF: f64 = -0.15;

/// This design's horizontal-tail reference area over the wing reference
/// area (`S_h / S`), for the rotation and landing-trim criteria's tail-force
/// (rather than tail-volume) bookkeeping: `0.0` with no second (tail) wing.
pub(super) fn tail_area_ratio(plane: &Airplane, s_ref: f64) -> f64 {
    let Some(h_stab) = plane.wings.get(1) else {
        return 0.0;
    };
    if s_ref <= 0.0 {
        0.0
    } else {
        h_stab.projected_area() / s_ref
    }
}

/// The horizontal tail's own aerodynamic-centre station, primary body
/// frame, m aft of the nose tip; `f64::NAN` with no second (tail) wing so a
/// caller that forgets to check can only ever disable the mechanism that
/// depends on it (matching the tip-back boundary's non-finite convention),
/// never silently substitute zero.
pub(super) fn horizontal_tail_ac_x_m(plane: &Airplane) -> f64 {
    plane
        .wings
        .get(1)
        .map_or(f64::NAN, |h_stab| h_stab.aerodynamic_center(0.25)[0])
}

/// A Wieselsberger-style ground-effect knockdown on the horizontal tail's
/// available maximum lift coefficient, dimensionless in `(0, 1]`, `1.0`
/// (no correction) with no second (tail) wing or a non-finite/non-positive
/// height or span.
///
/// Conceptual-design ground-effect factors are usually stated for a wing's
/// induced-drag reduction near the ground (Wieselsberger's classical
/// `sigma = (16 h/b)^2 / (1 + (16 h/b)^2)`, `h` = height above the ground,
/// `b` = span); this reuses that functional form as a documented,
/// conservative estimate of how much the tail's maximum-download authority
/// is degraded near the ground during rotation/landing flare (reduced local
/// dynamic pressure and interference), not a measured tail-specific result.
pub(super) fn tail_ground_effect_factor(plane: &Airplane, ground_z_m: f64) -> f64 {
    let Some(h_stab) = plane.wings.get(1) else {
        return 1.0;
    };
    let height_above_ground_m = h_stab.aerodynamic_center(0.25)[2] - ground_z_m;
    let span_m = h_stab.span();
    if !height_above_ground_m.is_finite()
        || !span_m.is_finite()
        || height_above_ground_m <= 0.0
        || span_m <= 0.0
    {
        return 1.0;
    }
    let ratio = 16.0 * height_above_ground_m / span_m;
    (ratio * ratio / (1.0 + ratio * ratio)).clamp(0.0, 1.0)
}

/// This design's mass-weighted vertical CG for the `OEW_KEYS` component
/// set, metres, mirroring `alas_payload::oew::oew_and_cg`'s `x` aggregation
/// on the `z` axis (needed for the tip-back boundary's `h_cg`, which
/// `oew_and_cg` does not itself carry).
pub(super) fn oew_cg_z(masses: &MassBreakdown, coords: &MassCoordinates) -> f64 {
    let mut total = 0.0;
    let mut moment = 0.0;
    for key in alas_mass::breakdown::OEW_KEYS {
        let mass = masses.get(key).unwrap_or(0.0).max(0.0);
        if mass <= 0.0 {
            continue;
        }
        if let Some((_, xyz)) = coords.as_pairs().into_iter().find(|&(name, _)| name == key) {
            total += mass;
            moment += mass * xyz[2];
        }
    }
    if total > 0.0 {
        moment / total
    } else {
        0.0
    }
}

/// The shared ground plane's `z`, metres: the mass model's own
/// (`alas_mass::stations::ground_plane_z_m`), so CG heights and gear
/// stations stand on one plane.
pub(super) fn ground_z_m(fuselage: &Fuselage, config: &AlasConfig) -> f64 {
    alas_mass::stations::ground_plane_z_m(fuselage, &config.geometry, &config.landing_gear)
}

/// Fuselage lower-contour samples aft of `x_main_gear_m`, for the
/// tail-scrape angle: `z_c - height/2` at each cross-section whose
/// station is aft of the main gear.
pub(super) fn fuselage_lower_points_aft_of(
    fuselage: &Fuselage,
    x_main_gear_m: f64,
) -> Vec<alas_perf::landing_gear::geometry::FuselageLowerPoint> {
    fuselage
        .xsecs
        .iter()
        .filter(|xsec| xsec.xyz_c[0] > x_main_gear_m)
        .map(
            |xsec| alas_perf::landing_gear::geometry::FuselageLowerPoint {
                x_m: xsec.xyz_c[0],
                z_bottom_m: xsec.xyz_c[2] - xsec.height / 2.0,
            },
        )
        .collect()
}

/// This design's horizontal-tail volume coefficient `S_h * l_h / (S * c)`
/// for the scissor-plot estimate, `0.0` with no second (tail) wing.
pub(super) fn tail_volume_coefficient(plane: &Airplane, s_ref: f64, mac: f64) -> f64 {
    let Some(main_wing) = plane.wings.first() else {
        return 0.0;
    };
    let Some(h_stab) = plane.wings.get(1) else {
        return 0.0;
    };
    let l_h = (h_stab.aerodynamic_center(0.25)[0] - main_wing.aerodynamic_center(0.25)[0]).abs();
    let s_h = h_stab.projected_area();
    if s_ref <= 0.0 || mac <= 0.0 {
        0.0
    } else {
        s_h * l_h / (s_ref * mac)
    }
}
