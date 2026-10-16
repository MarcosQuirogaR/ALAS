// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Geometry read off the built airplane for the FLOPS airframe equations:
//! the lifting surfaces, their lofted thickness, dihedral and wetted area,
//! the nacelle dimensions, and the spanwise stations of the detailed wing
//! method.

use alas_config::{GeometryConfig, WingConfig};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::Fuselage;
use alas_geom::aircraft::spacing::linspace;
use alas_geom::aircraft::wing::Wing;

use super::wing_bending::{elliptical_load_intensity, WingStation};

/// Spanwise integration stations per semispan for the detailed wing method.
const DETAILED_WING_STATIONS: usize = 41;

/// The built main wing and primary fuselage, with the fuselage measures the
/// structural equations read.
pub(super) struct BuiltFuselage<'a> {
    /// The primary fuselage.
    pub fuselage: &'a Fuselage,
    /// The main wing.
    pub wing: &'a Wing,
    /// Maximum fuselage width `WF`, m.
    pub width_m: f64,
    /// Maximum fuselage depth `DF`, m.
    pub depth_m: f64,
    /// Fuselage length `XL`, first to last section, m.
    pub length_m: f64,
}

impl<'a> BuiltFuselage<'a> {
    /// Measure `fuselage` beside `wing`.
    pub(super) fn of(wing: &'a Wing, fuselage: &'a Fuselage) -> Self {
        let (width_m, depth_m) = super::product::max_fuselage_width_depth(fuselage);
        let length_m = fuselage.xsecs.last().map_or(0.0, |x| x.xyz_c[0])
            - fuselage.xsecs.first().map_or(0.0, |x| x.xyz_c[0]);
        Self {
            fuselage,
            wing,
            width_m,
            depth_m,
            length_m,
        }
    }
}

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

/// Root-to-tip leading-edge dihedral of the main wing in its static ground
/// shape, deg. The built wing is the 1 g flight shape, so the configured
/// static-to-1 g tip rise is taken back off its tip
/// ([`alas_config::WingShape`]); equation 66's dihedral term is a ground-roll
/// clearance under the outboard engine, measured on the wing as it stands.
pub(super) fn ground_dihedral_deg(wing: &Wing, wing_config: &WingConfig) -> f64 {
    let (Some(root), Some(tip)) = (wing.xsecs.first(), wing.xsecs.last()) else {
        return 0.0;
    };
    let dy = (tip.xyz_le[1] - root.xyz_le[1]).abs();
    if dy <= 0.0 {
        return 0.0;
    }
    // The builder has already rejected an invalid rise, so none is left.
    let tip_rise_m = wing_config.flight_tip_rise_m(dy).unwrap_or(0.0);
    ((tip.xyz_le[2] - tip_rise_m - root.xyz_le[2]) / dy)
        .atan()
        .to_degrees()
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

/// Radius, as a fraction of the maximum, below which the tail of a nacelle
/// outline is an exhaust plug cone rather than nacelle structure.
const PLUG_RADIUS_FRACTION: f64 = 0.25;

/// Equivalent diameter `sqrt(width * height)` and length of one outline
/// `(station, width, height)`: the maximum diameter (the width itself for
/// the circular catalogue nacelles) and the distance from the first station
/// to the last one still at least [`PLUG_RADIUS_FRACTION`] of it. FLOPS
/// equation 69 prices a nacelle from its structure; the plug cone behind the
/// core nozzle is not part of it. An outline that ends above that fraction
/// (the generic catalogue pods) is measured to its end.
fn outline_dimensions(stations: &[(f64, f64, f64)]) -> (f64, f64) {
    let diameter = |&(_, w, h): &(f64, f64, f64)| (w * h).sqrt();
    let maximum = stations.iter().map(diameter).fold(0.0, f64::max);
    let first = stations.iter().map(|s| s.0).fold(f64::INFINITY, f64::min);
    let last = stations
        .iter()
        .filter(|s| diameter(s) >= PLUG_RADIUS_FRACTION * maximum)
        .map(|s| s.0)
        .fold(f64::NEG_INFINITY, f64::max);
    (maximum, last - first)
}

/// Average nacelle diameter and length, m, from the built bodies or from
/// the configured profile when none were built.
///
/// A centreline pod that carries its own installation outline
/// (`engine.centre_engine`) is mostly aft-fuselage and fin-root fairing: its
/// engine nacelle is the catalogue one, so that is what it contributes here;
/// its wetted area still counts in the drag and paint estimates.
pub(super) fn nacelle_dimensions(plane: &Airplane, geometry: &GeometryConfig) -> (f64, f64) {
    let engine = &geometry.engine;
    let catalogue = || {
        let radius = engine.radius_scale_m;
        let stations: Vec<_> = engine
            .nacelle_profile
            .iter()
            .map(|&(x, f)| (x, 2.0 * radius * f, 2.0 * radius * f))
            .collect();
        (2.0 * radius, outline_dimensions(&stations).1)
    };
    let bodies = nacelle_bodies(plane);
    if bodies.is_empty() {
        return catalogue();
    }
    let count = bodies.len() as f64;
    let (mut diameter, mut length) = (0.0, 0.0);
    for body in &bodies {
        let (d, l) = if body.name == "Nacelle Center" && engine.centre_engine.is_some() {
            catalogue()
        } else {
            let stations: Vec<_> = body
                .xsecs
                .iter()
                .map(|x| (x.xyz_c[0], x.width, x.height))
                .collect();
            outline_dimensions(&stations)
        };
        diameter += d / count;
        length += l / count;
    }
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
