// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Spar-box and fuselage-station geometry for [`super::resolve`].
//!
//! `alas-mass` cannot depend on `alas-opt`, so the spar-box cross-section
//! integration `alas_opt::transport_planform::wingbox_section_metrics` and
//! `wingbox_volume_m3` perform is reimplemented here rather than shared,
//! kept to the same per-panel trapezoid treatment so a design assessed by
//! both crates gets the same tankable volume from either one. What is added
//! here and has no counterpart there is the first moment needed for a
//! centroid, and a section's mid-thickness height for the z coordinate.

use alas_config::StructuresConfig;
use alas_config::{FuselageConfig, WingConfig};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::Fuselage;
use alas_geom::aircraft::spacing::linspace;
use alas_geom::aircraft::wing::{Wing, WingXSec};

use super::types::TankLayoutError;

/// One litre in cubic metres, the SI definition rather than a measurement.
pub(super) const M3_PER_LITRE: f64 = 1.0e-3;

/// Sample count for the thickness integral, matching
/// `alas_opt::transport_planform::WINGBOX_THICKNESS_SAMPLES`.
const THICKNESS_SAMPLES: usize = 41;

/// The front and rear chord fractions of the full-span spar box.
///
/// Only full-span spars bound a tank: a partial-span centre spar
/// (`StructuresConfig::center_spar_enabled`) stiffens the root box without
/// running to the tip, so it cannot be a tank boundary without also
/// declaring the tank absent outboard of the kink, which nothing here
/// models.
pub(super) fn spar_box_limits(
    structures: &StructuresConfig,
) -> Result<(f64, f64), TankLayoutError> {
    let (fractions, full_span) = structures.resolved_spars();
    let mut front = f64::INFINITY;
    let mut rear = f64::NEG_INFINITY;
    for (fraction, full) in fractions.into_iter().zip(full_span) {
        if full && fraction.is_finite() && (0.0..=1.0).contains(&fraction) {
            front = front.min(fraction);
            rear = rear.max(fraction);
        }
    }
    (front.is_finite() && rear.is_finite() && rear > front)
        .then_some((front, rear))
        .ok_or(TankLayoutError::InvalidSparLayout)
}

/// Cross-section metrics of the spar box at one built wing station.
#[derive(Debug, Clone, Copy)]
pub(super) struct SectionBox {
    pub area_m2: f64,
    pub x_mid_m: f64,
    pub z_mid_m: f64,
    pub width_m: f64,
    pub depth_m: f64,
}

/// The spar-box cross-section of `section` between `front` and `rear` chord
/// fractions.
///
/// `x_mid_m` and `z_mid_m` locate the box's own centre: `x_mid_m` at the
/// midpoint of the box in chord, `z_mid_m` at the mean camber line there
/// (the mid-thickness height), both already in geometry axes since they add
/// the section's own leading-edge position.
pub(super) fn section_box(section: &WingXSec, front: f64, rear: f64) -> Option<SectionBox> {
    if !section.chord.is_finite() || section.chord <= 0.0 {
        return None;
    }
    let samples = linspace(front, rear, THICKNESS_SAMPLES);
    let thickness = section.airfoil.local_thickness(&samples);
    if thickness.len() != samples.len() || thickness.iter().any(|value| !value.is_finite()) {
        return None;
    }
    let thickness_integral = thickness
        .windows(2)
        .zip(samples.windows(2))
        .map(|(pair, x_pair)| (pair[0] + pair[1]) * (x_pair[1] - x_pair[0]) / 2.0)
        .sum::<f64>();
    let depth_m = thickness.iter().copied().fold(f64::NEG_INFINITY, f64::max) * section.chord;
    let width_m = (rear - front) * section.chord;
    let area_m2 = thickness_integral * section.chord.powi(2);
    let mid_fraction = (front + rear) / 2.0;
    let camber_mid = *section.airfoil.local_camber(&[mid_fraction]).first()?;
    let x_mid_m = section.xyz_le[0] + section.chord * mid_fraction;
    let z_mid_m = section.xyz_le[2] + section.chord * camber_mid;
    (depth_m.is_finite()
        && depth_m > 0.0
        && width_m.is_finite()
        && width_m > 0.0
        && area_m2.is_finite()
        && area_m2 > 0.0
        && x_mid_m.is_finite()
        && z_mid_m.is_finite())
    .then_some(SectionBox {
        area_m2,
        x_mid_m,
        z_mid_m,
        width_m,
        depth_m,
    })
}

/// Volume, first-moment and mean-extent accumulators over a spanwise
/// interval of one side of a wing.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct WingBoxIntegral {
    pub volume_m3: f64,
    pub x_moment_m4: f64,
    pub y_moment_m4: f64,
    pub z_moment_m4: f64,
    pub width_weighted_m2: f64,
    pub depth_weighted_m2: f64,
    pub overlap_total_m: f64,
}

impl WingBoxIntegral {
    /// Whether this integral covers no finite, positive volume.
    pub(super) fn is_degenerate(&self) -> bool {
        !(self.volume_m3.is_finite() && self.volume_m3 > 0.0 && self.overlap_total_m > 0.0)
    }

    /// Volume-weighted x centroid.
    pub(super) fn x_centroid_m(&self) -> f64 {
        self.x_moment_m4 / self.volume_m3
    }

    /// Volume-weighted y centroid (positive toward the wing's own tip).
    pub(super) fn y_centroid_m(&self) -> f64 {
        self.y_moment_m4 / self.volume_m3
    }

    /// Volume-weighted z centroid.
    pub(super) fn z_centroid_m(&self) -> f64 {
        self.z_moment_m4 / self.volume_m3
    }

    /// Overlap-length-weighted mean box width, m.
    pub(super) fn mean_width_m(&self) -> f64 {
        self.width_weighted_m2 / self.overlap_total_m
    }

    /// Overlap-length-weighted mean box depth, m.
    pub(super) fn mean_depth_m(&self) -> f64 {
        self.depth_weighted_m2 / self.overlap_total_m
    }
}

/// Integrate the spar-box volume and first moment of `wing` between
/// `start_y_m` and `end_y_m`, on the one side the wing's own cross-sections
/// describe.
///
/// Each panel's contribution is its overlap with `[start_y_m, end_y_m]`
/// times the average of its two endpoint sections -- the same treatment
/// `alas_opt::transport_planform::wingbox_volume_m3` gives the volume term,
/// extended here to the moment terms by the same averaging. This
/// under-resolves a boundary that falls strictly inside a panel rather than
/// on one of the wing's own stations; a wing lofted with enough stations
/// that no configured tank boundary crosses one keeps that error small.
pub(super) fn integrate_wing_box(
    wing: &Wing,
    front: f64,
    rear: f64,
    start_y_m: f64,
    end_y_m: f64,
) -> Option<WingBoxIntegral> {
    let mut integral = WingBoxIntegral::default();
    for pair in wing.xsecs.windows(2) {
        let y_a = pair[0].xyz_le[1];
        let y_b = pair[1].xyz_le[1];
        let segment_start = y_a.min(y_b);
        let segment_end = y_a.max(y_b);
        let overlap = (segment_end.min(end_y_m) - segment_start.max(start_y_m)).max(0.0);
        if overlap <= 0.0 {
            continue;
        }
        let inboard = section_box(&pair[0], front, rear)?;
        let outboard = section_box(&pair[1], front, rear)?;
        integral.volume_m3 += overlap * (inboard.area_m2 + outboard.area_m2) / 2.0;
        integral.x_moment_m4 += overlap
            * (inboard.area_m2 * inboard.x_mid_m + outboard.area_m2 * outboard.x_mid_m)
            / 2.0;
        integral.y_moment_m4 += overlap * (inboard.area_m2 * y_a + outboard.area_m2 * y_b) / 2.0;
        integral.z_moment_m4 += overlap
            * (inboard.area_m2 * inboard.z_mid_m + outboard.area_m2 * outboard.z_mid_m)
            / 2.0;
        integral.width_weighted_m2 += overlap * (inboard.width_m + outboard.width_m) / 2.0;
        integral.depth_weighted_m2 += overlap * (inboard.depth_m + outboard.depth_m) / 2.0;
        integral.overlap_total_m += overlap;
    }
    Some(integral)
}

/// The root and tip y coordinates and semispan of the one side `wing`'s own
/// cross-sections describe.
pub(super) fn semispan_bounds(wing: &Wing) -> Option<(f64, f64, f64)> {
    let root_y = wing.xsecs.first()?.xyz_le[1];
    let tip_y = wing.xsecs.last()?.xyz_le[1];
    let semispan = tip_y - root_y;
    (semispan.is_finite() && semispan > 0.0).then_some((root_y, tip_y, semispan))
}

/// The y coordinate of the side of body: [`WingConfig::side_of_body_span_fraction`]
/// of the semispan when configured, otherwise the fuselage half-width, which
/// is where the wing structurally meets the body on an aircraft that never
/// named an explicit station.
pub(super) fn side_of_body_y_m(
    wing_config: &WingConfig,
    fuselage_config: &FuselageConfig,
    root_y_m: f64,
    semispan_m: f64,
) -> f64 {
    let fraction = wing_config
        .side_of_body_span_fraction
        .unwrap_or_else(|| (fuselage_config.diameter_m / 2.0) / semispan_m.max(1.0e-9));
    root_y_m + fraction.clamp(0.0, 1.0) * semispan_m
}

/// The horizontal stabiliser: the wing named `"Horizontal Stabilizer"`, or
/// failing that the second wing the builder always places there.
pub(super) fn horizontal_stabilizer(plane: &Airplane) -> Option<&Wing> {
    plane
        .wings
        .iter()
        .find(|wing| wing.name == "Horizontal Stabilizer")
        .or_else(|| plane.wings.get(1))
}

/// The interpolated `(x, y, z)` of `fuselage` at `x_fraction` of its length,
/// nose to tail.
pub(super) fn fuselage_station(fuselage: &Fuselage, x_fraction: f64) -> Option<[f64; 3]> {
    let first = fuselage.xsecs.first()?;
    let last = fuselage.xsecs.last()?;
    let length_m = last.xyz_c[0] - first.xyz_c[0];
    if !length_m.is_finite() || length_m <= 0.0 {
        return None;
    }
    let target_x = first.xyz_c[0] + x_fraction.clamp(0.0, 1.0) * length_m;
    for pair in fuselage.xsecs.windows(2) {
        let (inboard, outboard) = (pair[0], pair[1]);
        if target_x < inboard.xyz_c[0] || target_x > outboard.xyz_c[0] {
            continue;
        }
        let span_m = (outboard.xyz_c[0] - inboard.xyz_c[0]).max(1.0e-12);
        let blend = ((target_x - inboard.xyz_c[0]) / span_m).clamp(0.0, 1.0);
        let mut xyz = [0.0; 3];
        for (axis, coordinate) in xyz.iter_mut().enumerate() {
            *coordinate = inboard.xyz_c[axis] * (1.0 - blend) + outboard.xyz_c[axis] * blend;
        }
        return Some(xyz);
    }
    Some(last.xyz_c)
}
