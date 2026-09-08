// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Geometry read off the built airplane for the FLOPS airframe equations:
//! the lifting surfaces, their lofted thickness, dihedral and wetted area,
//! the nacelle dimensions, and the spanwise stations of the detailed wing
//! method.

use alas_config::GeometryConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::Fuselage;
use alas_geom::aircraft::spacing::linspace;
use alas_geom::aircraft::wing::Wing;

use super::wing_bending::{elliptical_load_intensity, WingStation};

/// Spanwise integration stations per semispan for the detailed wing method.
const DETAILED_WING_STATIONS: usize = 41;

pub(super) fn find_surface<'a>(plane: &'a Airplane, name: &str, index: usize) -> Option<&'a Wing> {
    plane
        .wings
        .iter()
        .find(|wing| wing.name == name)
        .or_else(|| plane.wings.get(index))
        .filter(|wing| wing.xsecs.len() >= 2)
}

fn section_thickness(wing: &Wing, sample: &[f64]) -> Vec<f64> {
    wing.xsecs
        .iter()
        .map(|xsec| xsec.airfoil.max_thickness(sample))
        .collect()
}

/// Area-weighted average thickness-to-chord ratio over the lofted sections.
pub(super) fn average_thickness(wing: &Wing) -> f64 {
    let sample = linspace(0.0, 1.0, 101);
    let thickness = section_thickness(wing, &sample);
    let mut weighted = 0.0;
    let mut total = 0.0;
    for (pair, tc) in wing.xsecs.windows(2).zip(thickness.windows(2)) {
        let span = (pair[1].xyz_le[1] - pair[0].xyz_le[1]).abs();
        let area = span * (pair[0].chord + pair[1].chord) / 2.0;
        weighted += area * (tc[0] + tc[1]) / 2.0;
        total += area;
    }
    if total > 0.0 {
        weighted / total
    } else {
        f64::NAN
    }
}

pub(super) fn dihedral_deg(wing: &Wing) -> f64 {
    let (Some(root), Some(tip)) = (wing.xsecs.first(), wing.xsecs.last()) else {
        return 0.0;
    };
    let dy = (tip.xyz_le[1] - root.xyz_le[1]).abs();
    if dy <= 0.0 {
        return 0.0;
    }
    ((tip.xyz_le[2] - root.xyz_le[2]) / dy).atan().to_degrees()
}

/// Approximate wetted area of a lifting surface: twice the planform,
/// inflated by a quarter of the thickness ratio (Torenbeek's thin-surface
/// rule), for the paint equation only.
pub(super) fn surface_wetted_area(wing: &Wing, thickness: f64) -> f64 {
    2.0 * wing.unfolded_area() * (1.0 + 0.25 * thickness.max(0.0))
}

fn nacelle_bodies(plane: &Airplane) -> Vec<&Fuselage> {
    plane
        .fuselages
        .iter()
        .filter(|body| body.name.contains("Nacelle"))
        .collect()
}

/// Average nacelle diameter and length, m, from the built bodies or from
/// the configured profile when none were built.
pub(super) fn nacelle_dimensions(plane: &Airplane, geometry: &GeometryConfig) -> (f64, f64) {
    let bodies = nacelle_bodies(plane);
    if bodies.is_empty() {
        let profile = &geometry.engine.nacelle_profile;
        let length = match (profile.first(), profile.last()) {
            (Some(first), Some(last)) => (last.0 - first.0).abs(),
            _ => 0.0,
        };
        return (2.0 * geometry.engine.radius_scale_m, length);
    }
    let count = bodies.len() as f64;
    let diameter = bodies
        .iter()
        .map(|body| body.xsecs.iter().map(|x| x.width).fold(0.0, f64::max))
        .sum::<f64>()
        / count;
    let length = bodies
        .iter()
        .map(|body| {
            let xs = body.xsecs.iter().map(|x| x.xyz_c[0]);
            xs.clone().fold(f64::NEG_INFINITY, f64::max) - xs.fold(f64::INFINITY, f64::min)
        })
        .sum::<f64>()
        / count;
    (diameter, length)
}

/// The detailed-method stations of the built main wing, from the
/// centreline outboard, with the load path along the quarter-chord line and
/// an elliptical load intensity.
pub(super) fn detailed_stations(wing: &Wing) -> Vec<WingStation> {
    let sample = linspace(0.0, 1.0, 101);
    let thickness = section_thickness(wing, &sample);
    let semispan = wing.reference_span() / if wing.symmetric { 2.0 } else { 1.0 };
    if semispan <= 0.0 {
        return Vec::new();
    }
    let root_y = wing.xsecs[0].xyz_le[1];
    let mut stations = Vec::with_capacity(DETAILED_WING_STATIONS);
    for k in 0..DETAILED_WING_STATIONS {
        let eta = k as f64 / (DETAILED_WING_STATIONS - 1) as f64;
        let y = root_y + eta * semispan;
        let mut segment = wing.xsecs.windows(2).zip(thickness.windows(2)).peekable();
        let mut station = None;
        while let Some((pair, tc)) = segment.next() {
            let (y0, y1) = (pair[0].xyz_le[1], pair[1].xyz_le[1]);
            let last = segment.peek().is_none();
            if (y <= y1 + 1e-12 || last) && y1 > y0 {
                let t = ((y - y0) / (y1 - y0)).clamp(0.0, 1.0);
                let chord = pair[0].chord + t * (pair[1].chord - pair[0].chord);
                let quarter0 = pair[0].xyz_le[0] + 0.25 * pair[0].chord;
                let quarter1 = pair[1].xyz_le[0] + 0.25 * pair[1].chord;
                station = Some(WingStation {
                    eta,
                    chord_per_semispan: chord / semispan,
                    thickness_to_chord: tc[0] + t * (tc[1] - tc[0]),
                    load_intensity: elliptical_load_intensity(eta),
                    load_path_sweep_deg: ((quarter1 - quarter0) / (y1 - y0)).atan().to_degrees(),
                });
                break;
            }
        }
        if let Some(station) = station {
            stations.push(station);
        }
    }
    stations
}
