// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The installation of a centreline (tail) engine, a trijet's No. 2 engine.
//!
//! A podded wing engine is a body of revolution scaled from the engine
//! catalogue, hung under the wing by `z_m` and `inlet_x_offset_m`. The centre
//! engine of a DC-10 or an L-1011 is a different object: a long, non-circular
//! pod that carries the fin on its upper surface, and whose inlet duct and
//! nozzle fairing are part of the pod. This group states that pod directly.
//!
//! Frame: x metres aft of the pod's inlet face, y/z as the geometry axes of
//! the fuselage (z up). The pod's aft end is the tail end of the fuselage, so
//! its inlet station follows from [`CentreEngineInstallation::length_m`].

use serde::{Deserialize, Serialize};

/// A centreline engine pod: axis height, cross-section and outline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CentreEngineInstallation {
    /// Height of the pod axis at the inlet face, m, in the fuselage-geometry
    /// axes (z up, the frame `fuselage.tail_z_m` and `vstab_z_m` are
    /// measured in).
    pub axis_z_m: f64,
    /// Pitch of the pod axis, degrees, positive when the aft end is lower
    /// than the inlet face (a straight duct inclined nose-up).
    pub axis_pitch_deg: f64,
    /// Maximum pod width, m.
    pub width_m: f64,
    /// Maximum pod height, m (the cross-section is a superellipse of
    /// exponent two, so `width_m` and `height_m` are its axes).
    pub height_m: f64,
    /// Outline as `(station aft of the inlet face in metres, fraction of the
    /// maximum width and height)` pairs; the last station is the nozzle exit,
    /// which ends at the aft end of the fuselage.
    pub profile: Vec<(f64, f64)>,
}

impl CentreEngineInstallation {
    /// Pod length, inlet face to nozzle exit, m (zero for an empty profile).
    pub fn length_m(&self) -> f64 {
        self.profile.last().map_or(0.0, |&(station, _)| station)
    }

    /// Whether the dimensions are finite and positive and the outline is a
    /// strictly increasing list of fractions in `(0, 1]`.
    pub fn is_valid(&self) -> bool {
        self.axis_z_m.is_finite()
            && self.axis_pitch_deg.is_finite()
            && self.axis_pitch_deg.abs() < 15.0
            && self.width_m.is_finite()
            && self.width_m > 0.0
            && self.height_m.is_finite()
            && self.height_m > 0.0
            && self.profile.len() >= 2
            && self.profile.first().is_some_and(|&(x, _)| x == 0.0)
            && self
                .profile
                .windows(2)
                .all(|pair| pair[1].0 > pair[0].0 && pair[0].0.is_finite())
            && self
                .profile
                .iter()
                .all(|&(_, f)| f.is_finite() && f > 0.0 && f <= 1.0)
    }
}

/// Lip exponent of a pod expressed in the nacelle family (template, confidence C).
const POD_LIP_EXPONENT: f64 = 2.2;
/// End slope of the pod's tail cone over its mean slope (template, confidence C).
const POD_TAIL_SLOPE_FACTOR: f64 = 1.2;

impl CentreEngineInstallation {
    /// The pod in the nacelle family: a single body (no fan-nozzle step) whose
    /// section half-width is `width_m / 2` times the profile fraction and
    /// whose height is `height_m / width_m` times the width.
    ///
    /// The crest begins at the first station of fraction one and the cylinder
    /// ends at the last; the tail cone runs to the last station. The lip
    /// exponent and the tail-cone end slope are templates (the drawing shows
    /// only the outline stations); `None` when the pod or the result is not valid.
    pub fn nacelle_shape_v2(&self) -> Option<crate::engines::NacelleShapeV2> {
        use crate::engines::NacelleShapeV2;
        if !self.is_valid() {
            return None;
        }
        let r = 0.5 * self.width_m;
        let &(end_x, end_f) = self.profile.last()?;
        let crest = self.profile.iter().find(|&&(_, f)| f >= 1.0)?.0;
        let cylinder_end = self.profile.iter().rev().find(|&&(_, f)| f >= 1.0)?.0;
        let tail = end_x - cylinder_end;
        if tail <= 0.0 || cylinder_end <= crest {
            return None;
        }
        let mean_slope = (1.0 - end_f) * r / tail;
        let shape = NacelleShapeV2 {
            highlight_radius_ratio: self.profile[0].1,
            cowl_cylinder_end_ratio: cylinder_end / end_x,
            fan_cowl_length_m: end_x,
            fan_nozzle_radius_ratio: end_f,
            core_cowl_length_m: end_x,
            core_nozzle_radius_ratio: end_f,
            lip_length_ratio: crest / r,
            lip_exponent: POD_LIP_EXPONENT,
            boat_tail_angle_deg: (POD_TAIL_SLOPE_FACTOR * mean_slope).atan().to_degrees(),
            te_thickness_m: 0.0,
            plug_tip_radius_m: 0.04 * r,
            ellipticity: self.height_m / self.width_m,
            droop_m: 0.0,
            provenance: Vec::new(),
        };
        shape.is_valid(end_x, r).then_some(shape)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pod() -> CentreEngineInstallation {
        CentreEngineInstallation {
            axis_z_m: 4.6,
            axis_pitch_deg: 4.0,
            width_m: 2.8,
            height_m: 3.3,
            profile: vec![(0.0, 0.95), (0.5, 1.0), (11.0, 1.0), (13.8, 0.3)],
        }
    }

    #[test]
    fn length_is_the_last_station() {
        assert_eq!(pod().length_m(), 13.8);
        assert!(pod().is_valid());
    }

    #[test]
    fn a_non_monotone_or_oversized_outline_is_rejected() {
        let mut p = pod();
        p.profile[2].0 = 0.2;
        assert!(!p.is_valid());
        let mut p = pod();
        p.profile[1].1 = 1.2;
        assert!(!p.is_valid());
    }

    #[test]
    // A failed expect here is the assertion failing.
    #[allow(clippy::expect_used)]
    fn the_pod_is_expressible_in_the_nacelle_family() {
        let v2 = pod().nacelle_shape_v2().expect("valid pod");
        assert_eq!(v2.core_cowl_length_m, v2.fan_cowl_length_m);
        assert!((v2.ellipticity - 3.3 / 2.8).abs() < 1e-12);
        assert!((v2.lip_length_ratio - 0.5 / 1.4).abs() < 1e-12);
        let mut bad = pod();
        bad.profile = vec![(0.0, 0.9), (13.8, 0.3)];
        assert!(bad.nacelle_shape_v2().is_none());
    }
}
