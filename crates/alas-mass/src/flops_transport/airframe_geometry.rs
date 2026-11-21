// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Geometry read off the built airplane for the FLOPS airframe equations:
//! the lifting surfaces, their lofted thickness, dihedral and wetted area,
//! the nacelle dimensions, and the spanwise stations of the detailed wing
//! method.

use alas_config::{EngineConfig, GeometryConfig, WingConfig};
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
    /// Fuselage depth `DF`, m: the depth of the maximum-width body
    /// (`product::max_fuselage_width_depth`).
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

/// The spanwise stations of the wing pods of one semi-wing, fractions of the
/// semispan. The engine list holds both sides, and rear-fuselage engines stand
/// off the centreline on no wing.
pub(super) fn semi_wing_pod_stations(engine: &EngineConfig, semispan_m: f64) -> Vec<f64> {
    if engine.rear_fuselage_mount().is_some() {
        return Vec::new();
    }
    engine
        .spanwise_positions_m
        .iter()
        .filter(|y| **y > 0.0)
        .map(|y| y / semispan_m)
        .filter(|eta| *eta <= 1.0)
        .collect()
}

/// The fin mass factor of the built tail.
pub(super) fn fin_factor(
    horizontal: &Wing,
    vertical: &Wing,
    fuselage: &Fuselage,
    fin_count: usize,
) -> f64 {
    let (height_m, fin_span_m) = tailplane_on_fin_heights_m(horizontal, vertical, fuselage);
    super::structure::fin_mounted_tailplane_factor(
        horizontal.reference_area(),
        height_m,
        vertical.reference_planform().unfolded_area() / fin_count as f64,
        fin_span_m,
    )
}

/// The height of the tailplane root above the fin root, and the fin span, m.
///
/// The fin root is where the fin leaves the body: the higher of its planform
/// root and the fuselage top at that station, since a planform carried into the
/// body is not fin span. A tailplane at or below that height is carried by the
/// fuselage and gives a height of zero or less.
fn tailplane_on_fin_heights_m(
    horizontal: &Wing,
    vertical: &Wing,
    fuselage: &Fuselage,
) -> (f64, f64) {
    let fin = vertical.reference_planform();
    let (Some(fin_root), Some(fin_tip), Some(tailplane_root)) = (
        fin.xsecs.first(),
        fin.xsecs.last(),
        horizontal.reference_planform().xsecs.first().cloned(),
    ) else {
        return (0.0, 0.0);
    };
    let body_top = fuselage_top_at(fuselage, fin_root.xyz_le[0]);
    let root_z = fin_root.xyz_le[2].max(body_top);
    (
        tailplane_root.xyz_le[2] - root_z,
        fin_tip.xyz_le[2] - root_z,
    )
}

/// Top of the fuselage at station `x`, m, interpolated between sections and
/// held at the end sections beyond them.
fn fuselage_top_at(fuselage: &Fuselage, x: f64) -> f64 {
    let top = |section: &alas_geom::aircraft::fuselage::FuselageXSec| {
        section.xyz_c[2] + section.height / 2.0
    };
    let mut height = f64::NEG_INFINITY;
    for pair in fuselage.xsecs.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if x >= a.xyz_c[0] && x <= b.xyz_c[0] && b.xyz_c[0] > a.xyz_c[0] {
            let t = (x - a.xyz_c[0]) / (b.xyz_c[0] - a.xyz_c[0]);
            height = top(a) + t * (top(b) - top(a));
            break;
        }
    }
    if height.is_finite() {
        return height;
    }
    fuselage.xsecs.last().map_or(0.0, top)
}

/// Area-weighted average thickness-to-chord ratio over the lofted sections.
/// Each panel's area is its chord times its span in the plane of the sections
/// (y and z together), so a fin, whose sections stack in z, is weighted like a wing.
pub(super) fn average_thickness(wing: &Wing) -> f64 {
    let sample = linspace(0.0, 1.0, 101);
    let thickness = section_thickness(wing, &sample);
    let mut weighted = 0.0;
    let mut total = 0.0;
    for (pair, tc) in wing.xsecs.windows(2).zip(thickness.windows(2)) {
        let span =
            (pair[1].xyz_le[1] - pair[0].xyz_le[1]).hypot(pair[1].xyz_le[2] - pair[0].xyz_le[2]);
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
    let Some(tip) = wing.xsecs.last() else {
        return 0.0;
    };
    let (root_le_m, _) = wing.design_root();
    let dy = (tip.xyz_le[1] - root_le_m[1]).abs();
    if dy <= 0.0 {
        return 0.0;
    }
    // The builder has already rejected an invalid rise, so none is left.
    let tip_rise_m = wing_config.flight_tip_rise_m(dy).unwrap_or(0.0);
    ((tip.xyz_le[2] - tip_rise_m - root_le_m[2]) / dy)
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

// Test fixture failures are failed assertions.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::builder::AircraftBuilder;

    #[test]
    fn a_fin_has_the_thickness_of_its_sections_like_a_tailplane() {
        let entry = alas_config::presets::get("A320-200").expect("preset");
        let plane = AircraftBuilder::new(Some(entry.geometry.clone()))
            .build(Some(&entry.design_vector), true)
            .expect("builds");
        for name in ["Vertical Stabilizer", "Horizontal Stabilizer", "Main Wing"] {
            let surface = plane
                .wings
                .iter()
                .find(|wing| wing.name == name)
                .expect("surface");
            let sample = linspace(0.0, 1.0, 101);
            let sections = section_thickness(surface, &sample);
            let (low, high) = sections
                .iter()
                .fold((f64::INFINITY, 0.0_f64), |(l, h), &t| (l.min(t), h.max(t)));
            let average = average_thickness(surface);
            assert!(
                average.is_finite() && average >= low - 1e-12 && average <= high + 1e-12,
                "{name} {average}"
            );
        }
    }
}
