// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Ground clearances of the nacelles and the wing tip: the drawn, unloaded
//! shape less the wing's static droop on the ground.
//!
//! The reference shape is whatever the caller built (the 1 g / drawn wing,
//! [`alas_config::WingShape::Flight`]); the droop is the cantilever deflection
//! of [`crate::wing_reconciliation::wing_ground_deflection`]. Heights are in
//! metres above the shared ground plane ([`crate::stations::ground_plane_z_m`]).
//!
//! The nacelle low point is the lowest nacelle cross-section bottom; the tip
//! low point is the tip leading edge less half the tip section thickness (the
//! lower surface under the thickest point), the feature the model's tip
//! reference carries. A tip fence or winglet is not modelled and is a known
//! difference to airport-planning tables that measure the fence bottom.
//!
//! Simplifications, deliberately: a nacelle is moved rigidly down by the wing
//! deflection at its centreline station (its roll from the local slope is below
//! 1 mm for 1 m of lateral extent at 3 deg of slope); the wing shortens by
//! `~ (w')^2 / 2` of its span, ignored; tyre and oleo deflection are those of
//! the ground plane the caller supplies.

use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::spacing::linspace;
use alas_struct::ground_deflection::GroundDeflection;

/// One nacelle's clearance, right-hand wing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NacelleClearance {
    /// Spanwise station of the nacelle centreline, m from the aircraft axis.
    pub y_m: f64,
    /// Low point above ground on the unloaded (drawn) wing, m.
    pub unloaded_m: f64,
    /// Wing droop at the nacelle station, m (positive down).
    pub droop_m: f64,
}

impl NacelleClearance {
    /// Low point above ground with the wing drooped, m.
    pub fn loaded_m(&self) -> f64 {
        self.unloaded_m - self.droop_m
    }
}

/// The wing-tip and nacelle clearances of one weight condition.
#[derive(Debug, Clone, PartialEq)]
pub struct GroundClearances {
    /// Nacelles of the right-hand wing, inboard first.
    pub nacelles: Vec<NacelleClearance>,
    /// Wing-tip station, m from the aircraft axis.
    pub tip_y_m: f64,
    /// Tip low point above ground on the unloaded wing, m.
    pub tip_unloaded_m: f64,
    /// Tip droop, m (positive down).
    pub tip_droop_m: f64,
}

impl GroundClearances {
    /// Tip low point above ground with the wing drooped, m.
    pub fn tip_loaded_m(&self) -> f64 {
        self.tip_unloaded_m - self.tip_droop_m
    }
}

/// Clearances of the built `plane` (unloaded shape) above `ground_z_m` after
/// the wing deflects by `deflection`.
pub fn ground_clearances(
    plane: &Airplane,
    ground_z_m: f64,
    deflection: &GroundDeflection,
) -> Option<GroundClearances> {
    let wing = plane
        .wings
        .iter()
        .find(|wing| wing.name == "Main Wing")
        .or_else(|| plane.wings.first())?;
    let tip = wing.xsecs.last()?;
    let thickness = tip.airfoil.max_thickness(&linspace(0.0, 1.0, 101));
    let mut nacelles: Vec<NacelleClearance> = plane
        .fuselages
        .iter()
        .filter(|body| body.name == "Nacelle R")
        .filter_map(|body| {
            let y_m = body.xsecs.first()?.xyz_c[1];
            let low = body
                .xsecs
                .iter()
                .map(|xsec| xsec.xyz_c[2] - xsec.height / 2.0)
                .fold(f64::INFINITY, f64::min);
            Some(NacelleClearance {
                y_m,
                unloaded_m: low - ground_z_m,
                droop_m: deflection.droop_at_m(y_m),
            })
        })
        .collect();
    nacelles.sort_by(|a, b| a.y_m.total_cmp(&b.y_m));
    Some(GroundClearances {
        nacelles,
        tip_y_m: tip.xyz_le[1],
        tip_unloaded_m: tip.xyz_le[2] - 0.5 * thickness * tip.chord - ground_z_m,
        tip_droop_m: deflection.droop_at_m(tip.xyz_le[1]),
    })
}

/// Bank angle at which a point at height `height_m` above the ground and
/// `lateral_arm_m` outboard of the rolling pivot touches the ground, degrees.
///
/// The aircraft rolls rigidly about the longitudinal line through the outboard
/// main-gear tyre contact; strut compression, tyre deflection and the wing's
/// change of shape with the rolling load are ignored (small, and of the
/// favourable sign for a lifting wing). Height and arm are in metres; the
/// result is `atan(h / d)` for `d > 0`, 90 deg when the point is on or inboard
/// of the pivot (it cannot strike by rolling that way), and NaN for a negative
/// height (already in contact) or non-finite inputs.
pub fn strike_bank_angle_deg(height_m: f64, lateral_arm_m: f64) -> f64 {
    if !height_m.is_finite() || !lateral_arm_m.is_finite() || height_m < 0.0 {
        return f64::NAN;
    }
    if lateral_arm_m <= 0.0 {
        return 90.0;
    }
    (height_m / lateral_arm_m).atan().to_degrees()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bank_angle_is_the_arctangent_of_height_over_arm() {
        // 2 m high, 4 m outboard of the pivot: atan(0.5).
        let angle = strike_bank_angle_deg(2.0, 4.0);
        assert!((angle - 26.565_051_177).abs() < 1e-6);
        assert_eq!(strike_bank_angle_deg(2.0, 0.0), 90.0);
        assert_eq!(strike_bank_angle_deg(2.0, -1.0), 90.0);
        assert!(strike_bank_angle_deg(-0.1, 3.0).is_nan());
        assert!(strike_bank_angle_deg(f64::NAN, 3.0).is_nan());
        assert!(strike_bank_angle_deg(0.0, 3.0).abs() < 1e-12);
    }

    #[test]
    fn droop_lowers_the_loaded_clearance() {
        let nacelle = NacelleClearance {
            y_m: 10.0,
            unloaded_m: 2.3,
            droop_m: 0.4,
        };
        assert!((nacelle.loaded_m() - 1.9).abs() < 1e-12);
    }
}
