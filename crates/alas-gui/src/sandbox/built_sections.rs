// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The sections of the built aircraft, as the section editors draw them.
//!
//! Aircraft axes: x aft from the nose, y to starboard, z up (metres). The
//! editors never reconstruct a shape from parameters: they read the very
//! cross-sections the builder lofted, so a lofted fuselage (A380-800), a
//! hump, a custom wing section or a twisted tip appear exactly as in the 3-D
//! model and in every solver mesh. The classification of wings and bodies
//! into components is the scene's own ([`SceneComponent`]).

use alas_geom::aircraft::airplane::Airplane;
use alas_report::families::geometry::SceneComponent;

use super::scene::SECTION_POINTS;

/// One lifting-surface cross-section: where it sits and its resolved airfoil
/// outline (the polyline the 3-D model lofts).
#[derive(Debug, Clone, PartialEq)]
pub struct LiftingSection {
    /// Leading-edge point, aircraft axes.
    pub le: [f64; 3],
    /// Local chord, m.
    pub chord: f64,
    /// Twist about the leading edge, degrees.
    pub twist_deg: f64,
    /// The outline in aircraft axes (starboard half for a symmetric surface).
    pub outline: Vec<[f64; 3]>,
}

/// One body cross-section: a fuselage or nacelle station.
#[derive(Debug, Clone, PartialEq)]
pub struct BodyStation {
    /// Section centre, aircraft axes.
    pub centre: [f64; 3],
    /// Width (y extent), m.
    pub width: f64,
    /// Height (z extent), m.
    pub height: f64,
}

/// The sections of the first lifting surface of `component`, root to tip, as
/// the 3-D model draws them. Empty when the aircraft has none.
pub fn lifting_sections(plane: &Airplane, component: SceneComponent) -> Vec<LiftingSection> {
    let Some(wing) = plane
        .wings
        .iter()
        .find(|wing| SceneComponent::of_wing(wing) == component)
    else {
        return Vec::new();
    };
    let outlines = wing.section_outlines(Some(SECTION_POINTS));
    wing.xsecs
        .iter()
        .zip(outlines)
        .map(|(xsec, outline)| LiftingSection {
            le: xsec.xyz_le,
            chord: xsec.chord,
            twist_deg: xsec.twist,
            outline,
        })
        .collect()
}

/// Whether the surface of `component` is mirrored about y = 0.
pub fn is_symmetric(plane: &Airplane, component: SceneComponent) -> bool {
    plane
        .wings
        .iter()
        .find(|wing| SceneComponent::of_wing(wing) == component)
        .is_some_and(|wing| wing.symmetric)
}

/// The station lists of every body of `component` (one for the fuselage, one
/// per nacelle), nose to tail.
pub fn body_stations(plane: &Airplane, component: SceneComponent) -> Vec<Vec<BodyStation>> {
    plane
        .fuselages
        .iter()
        .enumerate()
        .filter(|(index, body)| SceneComponent::of_fuselage(*index, body) == component)
        .map(|(_, body)| {
            body.xsecs
                .iter()
                .map(|xsec| BodyStation {
                    centre: xsec.xyz_c,
                    width: xsec.width,
                    height: xsec.height,
                })
                .collect()
        })
        .collect()
}

/// Index of the station with the largest cross-section area: the cabin
/// section the cross-section view opens on.
pub fn largest_station(stations: &[BodyStation]) -> usize {
    stations
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| (a.width * a.height).total_cmp(&(b.width * b.height)))
        .map_or(0, |(index, _)| index)
}

/// The points of the cross-section ring the 3-D model draws at a station:
/// an ellipse of the station's width and height about its centre, in the
/// station's y-z plane.
pub fn station_ring(station: &BodyStation, segments: usize) -> Vec<[f64; 3]> {
    (0..segments)
        .map(|k| {
            let theta = std::f64::consts::TAU * (k as f64) / (segments as f64);
            [
                station.centre[0],
                station.centre[1] + station.width * 0.5 * theta.cos(),
                station.centre[2] + station.height * 0.5 * theta.sin(),
            ]
        })
        .collect()
}
