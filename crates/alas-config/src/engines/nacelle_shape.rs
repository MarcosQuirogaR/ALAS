// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The drawing-fitted outline of a podded turbofan nacelle.
//!
//! A generic six-station profile (blunt lip, rapid expansion, constant
//! section, taper) draws every engine as the same short, rounded-off pod. A
//! real installation is a fan cowl of nearly constant diameter with a nearly
//! flat inlet lip, closing to the fan-nozzle exit, followed by a core cowl and
//! a plug. [`NacelleShape`] holds the few physical parameters that fix that
//! outline for one engine installation: where along the pod the fan nozzle
//! and the core nozzle sit, how large they are, and how large the lip is.
//!
//! Frame: stations are metres aft of the inlet face (the lip highlight plane),
//! radii are fractions of the maximum external radius. The silhouette is
//! axisymmetric, a body of revolution about the engine axis.
//!
//! The numbers are read off manufacturer general-arrangement drawings (plan
//! and side view of the installed nacelle, the larger of the two outlines at
//! the highlight plane being the one that is closer to circular); each entry's
//! `geometry_source` in `data/engines.json` names the drawing. They describe
//! the external cowl only: the inlet duct, fan and core are not modelled, so
//! the wetted area is the outer cowl, core cowl and plug.

use serde::{Deserialize, Serialize};

use crate::loft::Confidence;

/// Radius fraction at the tip of the core plug. Nonzero so the last station
/// keeps a finite perimeter, and small enough to read as a point.
const PLUG_TIP_RADIUS_RATIO: f64 = 0.04;

/// The lip rounds from the highlight to the maximum radius over this many
/// maximum radii of length.
const LIP_LENGTH_PER_RADIUS: f64 = 0.22;

/// Physical parameters of one drawing-fitted nacelle outline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NacelleShape {
    /// Inlet highlight (lip) radius over the maximum external radius, in the
    /// plane of the inlet face. A real lip is nearly as large as the cowl.
    pub highlight_radius_ratio: f64,
    /// Station, over `fan_cowl_length_m`, where the cylindrical part of the
    /// cowl ends and the boat-tail towards the fan nozzle begins.
    pub cowl_cylinder_end_ratio: f64,
    /// Inlet face to the fan-nozzle exit (trailing edge of the fan cowl), m.
    pub fan_cowl_length_m: f64,
    /// External radius at the fan-nozzle exit over the maximum radius.
    pub fan_nozzle_radius_ratio: f64,
    /// Inlet face to the core-nozzle exit, m. Not shorter than the fan cowl.
    pub core_cowl_length_m: f64,
    /// External radius at the core-nozzle exit over the maximum radius.
    pub core_nozzle_radius_ratio: f64,
}

impl NacelleShape {
    /// Whether the parameters describe a monotone, finite outline inside a
    /// pod of `length_m` (the inlet face to the plug tip).
    pub fn is_valid(&self, length_m: f64) -> bool {
        let ratio = |r: f64| r.is_finite() && r > 0.0 && r <= 1.0;
        ratio(self.highlight_radius_ratio)
            && ratio(self.fan_nozzle_radius_ratio)
            && ratio(self.core_nozzle_radius_ratio)
            && self.cowl_cylinder_end_ratio.is_finite()
            && self.cowl_cylinder_end_ratio > 0.0
            && self.cowl_cylinder_end_ratio < 1.0
            && self.fan_cowl_length_m.is_finite()
            && self.fan_cowl_length_m > 0.0
            && self.core_cowl_length_m >= self.fan_cowl_length_m
            && length_m.is_finite()
            && length_m > self.core_cowl_length_m
    }

    /// The outline as `(station in metres, radius fraction)` pairs from the
    /// inlet face to the plug tip, for a pod `length_m` long.
    ///
    /// Stations strictly increase and every fraction is at most one. The
    /// maximum radius is held from the end of the lip to the end of the
    /// cylindrical part of the fan cowl.
    pub fn profile(&self, length_m: f64, max_radius_m: f64) -> Vec<(f64, f64)> {
        let lip = (LIP_LENGTH_PER_RADIUS * max_radius_m).min(0.25 * self.fan_cowl_length_m);
        let highlight = self.highlight_radius_ratio;
        let cylinder_end =
            (self.cowl_cylinder_end_ratio * self.fan_cowl_length_m).max(2.0 * lip + 1.0e-3);
        let mut points = vec![
            (0.0, highlight),
            (0.5 * lip, highlight + 0.8 * (1.0 - highlight)),
            (lip, 1.0),
            (cylinder_end, 1.0),
            (self.fan_cowl_length_m, self.fan_nozzle_radius_ratio),
        ];
        if self.core_cowl_length_m > self.fan_cowl_length_m + 1.0e-3 {
            points.push((self.core_cowl_length_m, self.core_nozzle_radius_ratio));
        }
        points.push((
            length_m,
            PLUG_TIP_RADIUS_RATIO.min(self.core_nozzle_radius_ratio),
        ));
        points
    }
}

/// The core cowl starts, behind the fan-nozzle exit, at this fraction of the
/// way from the core-nozzle radius to the fan-nozzle radius. Template
/// (confidence C): the CFM56-5B drawing shows the core cowl at roughly 60 %
/// of the maximum radius under the fan-nozzle exit, which this reproduces for
/// the 0.8 / 0.37 radius ratios of that engine.
pub const CORE_COWL_START_FRACTION: f64 = 0.55;

/// Where one group of [`NacelleShapeV2`] numbers comes from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NacelleParamSource {
    /// Field names of [`NacelleShapeV2`] this entry covers.
    pub fields: Vec<String>,
    /// Drawing and reading, or the template rule when nothing was drawn.
    pub source: String,
    /// `A` vector drawing, `B` clean raster, `C` estimate or template.
    pub confidence: Confidence,
}

/// The G2 nacelle outline: the six numbers of [`NacelleShape`] plus the seven
/// that the lofted revolve needs (design 2026-10-10, section 3.1).
///
/// Frame and units as [`NacelleShape`]: metres aft of the inlet face, radii
/// over the maximum external radius `R` (= `nacelle_max_radius_m`, the
/// half-width of the plan view). The lofted revolve is
/// `alas_geom::loft::NacelleLoft`; this type is the data only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NacelleShapeV2 {
    /// As [`NacelleShape::highlight_radius_ratio`]; must be below 1 (the lip
    /// has a vertical tangent at the highlight).
    pub highlight_radius_ratio: f64,
    /// As [`NacelleShape::cowl_cylinder_end_ratio`].
    pub cowl_cylinder_end_ratio: f64,
    /// As [`NacelleShape::fan_cowl_length_m`].
    pub fan_cowl_length_m: f64,
    /// As [`NacelleShape::fan_nozzle_radius_ratio`].
    pub fan_nozzle_radius_ratio: f64,
    /// As [`NacelleShape::core_cowl_length_m`]. Equal to the fan cowl length:
    /// one body (a turboprop nacelle or a tail-engine pod), no fan-nozzle step.
    pub core_cowl_length_m: f64,
    /// As [`NacelleShape::core_nozzle_radius_ratio`].
    pub core_nozzle_radius_ratio: f64,
    /// Length from the highlight to the crest (maximum radius) over `R`.
    pub lip_length_ratio: f64,
    /// Exponent of the lip superellipse, 2 to 4 (2 is an ellipse).
    pub lip_exponent: f64,
    /// Slope angle of the boat-tail at the fan-nozzle exit, degrees, aft end
    /// pointing inward (positive).
    pub boat_tail_angle_deg: f64,
    /// Thickness of the fan-cowl trailing edge, m (a blunt annulus).
    pub te_thickness_m: f64,
    /// Radius of the flat disc that ends the plug, m.
    pub plug_tip_radius_m: f64,
    /// Section height over width (1 is circular); the width stays `2 r`.
    pub ellipticity: f64,
    /// Downward shift of the section centre at maximum radius, m, scaled
    /// with the local radius along the body (0: axisymmetric).
    pub droop_m: f64,
    /// Source and confidence of every number above.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance: Vec<NacelleParamSource>,
}

impl NacelleShapeV2 {
    /// The six pre-existing numbers as a [`NacelleShape`].
    pub fn legacy(&self) -> NacelleShape {
        NacelleShape {
            highlight_radius_ratio: self.highlight_radius_ratio,
            cowl_cylinder_end_ratio: self.cowl_cylinder_end_ratio,
            fan_cowl_length_m: self.fan_cowl_length_m,
            fan_nozzle_radius_ratio: self.fan_nozzle_radius_ratio,
            core_cowl_length_m: self.core_cowl_length_m,
            core_nozzle_radius_ratio: self.core_nozzle_radius_ratio,
        }
    }

    /// Whether the numbers are finite and inside the ranges the revolve can
    /// build, for a pod `length_m` long (inlet face to the aft end) with
    /// maximum radius `max_radius_m`. The profile's monotonicity is checked
    /// by the builder, which owns the geometry.
    pub fn is_valid(&self, length_m: f64, max_radius_m: f64) -> bool {
        let r = max_radius_m;
        let ratio = |v: f64| v.is_finite() && v > 0.0 && v <= 1.0;
        let lip_end = self.lip_length_ratio * r;
        let cylinder_end = self.cowl_cylinder_end_ratio * self.fan_cowl_length_m;
        r.is_finite()
            && r > 0.0
            && ratio(self.fan_nozzle_radius_ratio)
            && ratio(self.core_nozzle_radius_ratio)
            && self.highlight_radius_ratio.is_finite()
            && self.highlight_radius_ratio > 0.0
            && self.highlight_radius_ratio <= 0.999
            && self.cowl_cylinder_end_ratio.is_finite()
            && self.cowl_cylinder_end_ratio > 0.0
            && self.cowl_cylinder_end_ratio < 1.0
            && self.fan_cowl_length_m.is_finite()
            && self.core_cowl_length_m >= self.fan_cowl_length_m
            && length_m.is_finite()
            && length_m >= self.core_cowl_length_m
            && self.lip_length_ratio.is_finite()
            && self.lip_length_ratio >= 0.05
            && lip_end < cylinder_end
            && (2.0..=4.0).contains(&self.lip_exponent)
            && self.boat_tail_angle_deg.is_finite()
            && self.boat_tail_angle_deg > 0.0
            && self.boat_tail_angle_deg < 45.0
            && self.te_thickness_m.is_finite()
            && self.te_thickness_m >= 0.0
            && self.te_thickness_m < 0.5 * self.fan_nozzle_radius_ratio * r
            && self.plug_tip_radius_m.is_finite()
            && self.plug_tip_radius_m > 0.0
            && self.plug_tip_radius_m
                < self
                    .core_nozzle_radius_ratio
                    .min(self.fan_nozzle_radius_ratio)
                    * r
            && self.ellipticity.is_finite()
            && (0.5..=2.0).contains(&self.ellipticity)
            && self.droop_m.is_finite()
            && self.droop_m.abs() < r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> NacelleShape {
        NacelleShape {
            highlight_radius_ratio: 0.93,
            cowl_cylinder_end_ratio: 0.5,
            fan_cowl_length_m: 3.2,
            fan_nozzle_radius_ratio: 0.8,
            core_cowl_length_m: 4.5,
            core_nozzle_radius_ratio: 0.37,
        }
    }

    #[test]
    fn the_outline_runs_from_the_inlet_face_to_the_plug_tip_and_never_exceeds_the_maximum() {
        let shape = sample();
        assert!(shape.is_valid(5.0));
        let profile = shape.profile(5.0, 1.02);
        assert_eq!(profile.first().map(|p| p.0), Some(0.0));
        assert_eq!(profile.last().map(|p| p.0), Some(5.0));
        assert!(profile.windows(2).all(|pair| pair[0].0 < pair[1].0));
        assert!(profile.iter().all(|&(_, r)| r > 0.0 && r <= 1.0));
        assert_eq!(profile.iter().filter(|&&(_, r)| r == 1.0).count(), 2);
    }

    #[test]
    fn the_lip_is_a_blunt_face_not_a_taper_to_a_small_radius() {
        let profile = sample().profile(5.0, 1.02);
        assert!(
            profile[0].1 > 0.9,
            "highlight radius ratio {}",
            profile[0].1
        );
    }

    #[test]
    fn inconsistent_parameters_are_rejected() {
        let mut shape = sample();
        shape.core_cowl_length_m = 3.0;
        assert!(!shape.is_valid(5.0));
        let mut shape = sample();
        shape.fan_nozzle_radius_ratio = 1.2;
        assert!(!shape.is_valid(5.0));
        assert!(!sample().is_valid(4.5));
    }

    fn sample_v2() -> NacelleShapeV2 {
        NacelleShapeV2 {
            highlight_radius_ratio: 0.93,
            cowl_cylinder_end_ratio: 0.5,
            fan_cowl_length_m: 3.2,
            fan_nozzle_radius_ratio: 0.8,
            core_cowl_length_m: 4.5,
            core_nozzle_radius_ratio: 0.37,
            lip_length_ratio: 0.4,
            lip_exponent: 2.2,
            boat_tail_angle_deg: 11.0,
            te_thickness_m: 0.02,
            plug_tip_radius_m: 0.04,
            ellipticity: 1.0,
            droop_m: 0.0,
            provenance: Vec::new(),
        }
    }

    #[test]
    fn the_v2_shape_keeps_the_six_legacy_numbers_and_rejects_out_of_range_ones() {
        let v2 = sample_v2();
        assert_eq!(v2.legacy(), sample());
        assert!(v2.is_valid(5.05, 1.02));
        let mut bad = sample_v2();
        bad.highlight_radius_ratio = 1.0;
        assert!(!bad.is_valid(5.05, 1.02));
        let mut bad = sample_v2();
        bad.lip_exponent = 1.5;
        assert!(!bad.is_valid(5.05, 1.02));
        let mut bad = sample_v2();
        bad.boat_tail_angle_deg = f64::NAN;
        assert!(!bad.is_valid(5.05, 1.02));
        assert!(!v2.is_valid(4.0, 1.02));
    }

    #[test]
    // A failed unwrap here is the assertion failing.
    #[allow(clippy::unwrap_used)]
    fn the_v2_shape_round_trips_through_json() {
        let v2 = sample_v2();
        let text = serde_json::to_string(&v2).unwrap();
        assert_eq!(serde_json::from_str::<NacelleShapeV2>(&text).unwrap(), v2);
    }
}
