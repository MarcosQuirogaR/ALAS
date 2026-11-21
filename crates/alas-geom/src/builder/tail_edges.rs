// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The built edges of the tail surfaces away from their straight lines.
//!
//! [`alas_config::TailEdgeStation`] states what they are. This module turns the
//! stations into the sections of the [`Wing`]: straight sections inboard of the
//! first station, then one section per station, ending on the tip, with sections on
//! the straight runs between them at the mesh's own strip width.
//! Inboard of the first station the edges run straight from the built root to it.
//! The exception is a leading edge the first station pulls forward of its line: it
//! is held at that station down to the root, because a line stepping back aft below
//! the station would cut a notch into the surface that the aircraft does not have.
//! The straight trapezoid the stations are measured from stays on the wing as its
//! reference planform ([`Wing::reference_planform`]). Frame: x aft, y to starboard,
//! z up, metres, aircraft axes.

use alas_config::geometry::MIN_TAIL_EDGE_STATION_SPACING;
use alas_config::{validate_tail_edge_stations, TailEdgeStation, WingThicknessStation};

use super::BuildError;
use crate::aircraft::wing::{Wing, WingXSec};
use crate::airfoil_library::{morph_airfoil, DEFAULT_MORPH_N_POINTS};

/// A straight-edged tail surface at its configured root line and tip, whatever
/// the root the built surface is carried down or trimmed to, in aircraft axes at
/// the tail scale of the build.
pub(super) struct StraightTail {
    /// Leading edge on the configured root line.
    pub root_le_m: [f64; 3],
    /// Chord on the configured root line.
    pub root_chord_m: f64,
    /// Incidence on the configured root line, degrees.
    pub root_twist_deg: f64,
    /// Leading edge at the tip.
    pub tip_le_m: [f64; 3],
    /// Chord at the tip.
    pub tip_chord_m: f64,
    /// Incidence at the tip, degrees.
    pub tip_twist_deg: f64,
    /// Coordinate the surface grows along: 1 for a tailplane, 2 for a fin.
    pub span_axis: usize,
}

impl StraightTail {
    /// Fraction of the root-to-tip run at `xyz_le`.
    fn fraction_at(&self, xyz_le: [f64; 3]) -> f64 {
        let axis = self.span_axis;
        (xyz_le[axis] - self.root_le_m[axis]) / (self.tip_le_m[axis] - self.root_le_m[axis])
    }

    /// `(leading edge, chord, twist)` on the straight edges at `fraction`.
    fn on_lines(&self, fraction: f64) -> ([f64; 3], f64, f64) {
        let along = |root: f64, tip: f64| root + fraction * (tip - root);
        (
            [
                along(self.root_le_m[0], self.tip_le_m[0]),
                along(self.root_le_m[1], self.tip_le_m[1]),
                along(self.root_le_m[2], self.tip_le_m[2]),
            ],
            along(self.root_chord_m, self.tip_chord_m),
            along(self.root_twist_deg, self.tip_twist_deg),
        )
    }
}

/// A run shorter than this share of a strip beyond a whole number of strips is not
/// given a strip of its own.
const MIN_STRIP_SHARE: f64 = 0.25;

/// How many strips divide a `run` of the configured root-to-tip fraction at about
/// `strip`.
fn strips_in(run: f64, strip: f64) -> usize {
    (run / strip - MIN_STRIP_SHARE).ceil().max(1.0) as usize
}

/// `wing`, meshed on its straight edges, with the sections `stations` set on
/// `line`, offsets scaled by `scale`, and the straight edges kept as its reference
/// planform. Without stations `wing` is returned as it is.
///
/// Every station is a panel edge, so the lattice follows the same outline the
/// exports and renders show. A strip is one mesh panel of the configured
/// root-to-tip run, and the number of strips of each run comes from the station
/// fractions alone. The section count is then the same for every tail scale, fin
/// scale and fuselage length, so a design change moves the lattice but never
/// re-panels it, which would step every derivative read from it.
///
/// The built root moves along the configured run with those. A station it reaches
/// is dropped, as the surface no longer has that part, and its strips stay with the
/// run inboard of the next station.
///
/// # Errors
///
/// [`BuildError::TailPlacement`] for invalid stations or a chord the offsets close.
pub(super) fn with_drawn_edges(
    mut wing: Wing,
    stations: &[TailEdgeStation],
    line: &StraightTail,
    scale: f64,
) -> Result<Wing, BuildError> {
    let Some(first) = stations.first() else {
        return Ok(wing);
    };
    validate_tail_edge_stations(stations)
        .map_err(|error| BuildError::TailPlacement(error.to_string()))?;
    let (Some(root), Some(tip)) = (wing.xsecs.first().cloned(), wing.xsecs.last().cloned()) else {
        return Ok(wing);
    };
    let airfoil = tip.airfoil.clone();
    let reference = wing
        .reference_xsecs
        .take()
        .unwrap_or_else(|| vec![root.clone(), tip.clone()]);
    let root_fraction = line.fraction_at(root.xyz_le);
    let strip = 1.0 / wing.xsecs.len().saturating_sub(1).max(1) as f64;

    let mut inboard_strips = strips_in(first.span_fraction, strip);
    let mut kept = stations;
    while let Some((station, rest)) = kept.split_first() {
        if station.span_fraction > root_fraction + MIN_TAIL_EDGE_STATION_SPACING {
            break;
        }
        let next = rest.first().map_or(1.0, |next| next.span_fraction);
        if next > station.span_fraction {
            inboard_strips += strips_in(next - station.span_fraction, strip);
        }
        kept = rest;
    }

    let outer = kept.first().map_or(1.0, |station| station.span_fraction);
    let mut built = vec![root];
    let first_station = kept.first();
    let held_le_x_m = first_station
        .filter(|station| station.le_offset_m < 0.0)
        .map(|station| line.on_lines(station.span_fraction).0[0] + station.le_offset_m * scale);
    let (first_le_m, first_te_m) = first_station.map_or((0.0, 0.0), |station| {
        (
            station.le_offset_m.max(0.0) * scale,
            station.te_offset_m * scale,
        )
    });
    let mut shares: Vec<f64> = (1..inboard_strips)
        .map(|index| index as f64 / inboard_strips as f64)
        .collect();
    if let Some(x_le) = held_le_x_m {
        keep_a_section_on_the_hold_corner(&mut shares, line, x_le, root_fraction, outer);
    }
    built.extend(shares.into_iter().map(|share| {
        let (mut xyz_le, chord, twist) =
            line.on_lines(root_fraction + share * (outer - root_fraction));
        xyz_le[0] += share * first_le_m;
        let chord = chord + share * (first_te_m - first_le_m);
        WingXSec::new(xyz_le, chord, twist, airfoil.clone())
    }));
    if let Some(x_le) = held_le_x_m {
        hold_leading_edge(&mut built, x_le);
    }
    let mut previous = None::<(f64, WingXSec)>;
    for station in kept {
        let (mut xyz_le, chord, twist) = line.on_lines(station.span_fraction);
        xyz_le[0] += station.le_offset_m * scale;
        let chord = chord + (station.te_offset_m - station.le_offset_m) * scale;
        if chord <= 0.0 {
            return Err(BuildError::TailPlacement(format!(
                "{}: edge station {} closes the chord ({chord} m)",
                wing.name, station.span_fraction
            )));
        }
        let section = WingXSec::new(xyz_le, chord, twist, airfoil.clone());
        if let Some((fraction, inboard)) = &previous {
            let strips = strips_in(station.span_fraction - fraction, strip);
            push_run(&mut built, inboard, &section, strips);
        }
        built.push(section.clone());
        previous = Some((station.span_fraction, section));
    }
    match &previous {
        Some((fraction, _)) if *fraction >= 1.0 => {}
        Some((fraction, inboard)) => {
            push_run(&mut built, inboard, &tip, strips_in(1.0 - fraction, strip));
            built.push(tip);
        }
        None => built.push(tip),
    }
    wing.xsecs = built;
    wing.reference_xsecs = Some(reference);
    Ok(wing)
}

/// Where a fin's reference planform ends on `line`, as a fraction with 1 at the tip:
/// the configured end, else the tailplane seat of a T-tail, else nowhere short of
/// the tip.
///
/// The seat rule is an inference from the numbers, stated by no document: above the
/// seat stands the cap and bullet fairing, which belongs to the built surface but
/// not to the fin of the published areas. It holds on the A400M, whose fin to its
/// seat is 46.1 m2 (46.43 m2 in Jane's All the World's Aircraft 2008-09) and 52.6 m2
/// to its top, and on the ATR 72, 12.1 m2 (12.48 m2 published) and 13.7 m2. It does
/// not hold where the fin continues above the tailplane, which the configured end
/// then states.
pub(super) fn reference_fin_end(
    configured: Option<f64>,
    line: &StraightTail,
    seat: Option<[f64; 3]>,
) -> Option<f64> {
    configured.or_else(|| seat.map(|seat| line.fraction_at(seat)))
}

/// `fin` with its reference planform ending at `fraction` of `line`. An end at or
/// beyond the fin tip changes nothing.
pub(super) fn with_reference_fin_to(mut fin: Wing, line: &StraightTail, fraction: f64) -> Wing {
    let root = match (&fin.reference_xsecs, fin.xsecs.first()) {
        (Some(reference), _) => reference.first().cloned(),
        (None, first) => first.cloned(),
    };
    let Some(root) = root else {
        return fin;
    };
    if fraction <= line.fraction_at(root.xyz_le) || fraction >= 1.0 {
        return fin;
    }
    let (xyz_le, chord, twist) = line.on_lines(fraction);
    let tip = WingXSec::new(xyz_le, chord, twist, root.airfoil.clone());
    fin.reference_xsecs = Some(vec![root, tip]);
    fin
}

/// Move the section nearest where the leading-edge line reaches the held `x_le` onto
/// that point, so the corner is a panel edge at every mesh, keeping the section count.
fn keep_a_section_on_the_hold_corner(
    shares: &mut [f64],
    line: &StraightTail,
    x_le: f64,
    root_fraction: f64,
    outer: f64,
) {
    let (x_root, x_tip) = (line.root_le_m[0], line.tip_le_m[0]);
    let corner = ((x_le - x_root) / (x_tip - x_root) - root_fraction) / (outer - root_fraction);
    if !(corner > 0.0 && corner < 1.0) {
        return;
    }
    let nearest = shares
        .iter_mut()
        .min_by(|a, b| (**a - corner).abs().total_cmp(&(**b - corner).abs()));
    if let Some(share) = nearest {
        *share = corner;
    }
}

/// Bring every leading edge aft of `x_le` forward to it, trailing edge in place.
fn hold_leading_edge(sections: &mut [WingXSec], x_le: f64) {
    for section in sections {
        let aft = section.xyz_le[0] - x_le;
        if aft > 0.0 {
            section.xyz_le[0] = x_le;
            section.chord += aft;
        }
    }
}

/// The sections strictly between `inboard` and `outboard`, on the straight line
/// joining them, that divide that run into `strips`.
fn push_run(built: &mut Vec<WingXSec>, inboard: &WingXSec, outboard: &WingXSec, strips: usize) {
    for index in 1..strips {
        let s = index as f64 / strips as f64;
        let along = |a: f64, b: f64| a + s * (b - a);
        built.push(WingXSec::new(
            [
                along(inboard.xyz_le[0], outboard.xyz_le[0]),
                along(inboard.xyz_le[1], outboard.xyz_le[1]),
                along(inboard.xyz_le[2], outboard.xyz_le[2]),
            ],
            along(inboard.chord, outboard.chord),
            along(inboard.twist, outboard.twist),
            inboard.airfoil.clone(),
        ));
    }
}

/// Scale the section thickness of a built tail surface at the fractions of `line`.
/// A no-op without stations.
///
/// # Errors
///
/// [`BuildError::TailPlacement`] for fractions out of order or off `[0, 1]`, or a
/// scale off `(0.3, 2)`, the range of the main wing.
pub(super) fn with_section_thickness(
    mut wing: Wing,
    stations: &[WingThicknessStation],
    line: &StraightTail,
) -> Result<Wing, BuildError> {
    if stations.is_empty() {
        return Ok(wing);
    }
    let ordered = stations
        .windows(2)
        .all(|pair| pair[1].span_fraction > pair[0].span_fraction);
    let in_range = |station: &WingThicknessStation| {
        (0.0..=1.0).contains(&station.span_fraction)
            && station.thickness_scale > 0.3
            && station.thickness_scale < 2.0
    };
    if !ordered || !stations.iter().all(in_range) {
        return Err(BuildError::TailPlacement(format!(
            "fin thickness stations need increasing fractions in [0, 1] and scales in (0.3, 2): {stations:?}"
        )));
    }
    for xsec in &mut wing.xsecs {
        let scale = thickness_scale_at(stations, line.fraction_at(xsec.xyz_le));
        xsec.airfoil = morph_airfoil(
            &xsec.airfoil.coordinates,
            scale,
            1.0,
            DEFAULT_MORPH_N_POINTS,
        );
    }
    Ok(wing)
}

/// The scale at `fraction`: linear between stations, held beyond the ends.
fn thickness_scale_at(stations: &[WingThicknessStation], fraction: f64) -> f64 {
    let after = stations.partition_point(|station| station.span_fraction <= fraction);
    match (
        after.checked_sub(1).map(|index| &stations[index]),
        stations.get(after),
    ) {
        (Some(a), Some(b)) => {
            let share = (fraction - a.span_fraction) / (b.span_fraction - a.span_fraction);
            a.thickness_scale + share * (b.thickness_scale - a.thickness_scale)
        }
        (Some(only), None) | (None, Some(only)) => only.thickness_scale,
        (None, None) => 1.0,
    }
}

#[cfg(test)]
// A failed expect in a test is the assertion failing.
#[allow(clippy::expect_used)]
mod tests {
    use alas_config::{
        DesignVector, EmpennageConfig, GeometryConfig, TailEdgeStation, WingThicknessStation,
    };

    use crate::aircraft::wing::Wing;
    use crate::builder::AircraftBuilder;

    fn station(span_fraction: f64, le_offset_m: f64, te_offset_m: f64) -> TailEdgeStation {
        TailEdgeStation {
            span_fraction,
            le_offset_m,
            te_offset_m,
        }
    }

    fn built(empennage: EmpennageConfig) -> crate::aircraft::airplane::Airplane {
        let geometry = GeometryConfig {
            empennage,
            ..GeometryConfig::default()
        };
        AircraftBuilder::new(Some(geometry))
            .build(Some(&DesignVector::default()), false)
            .expect("builds")
    }

    #[test]
    fn fin_thickness_stations_thin_the_sections_and_leave_the_chords_and_areas() {
        let plain = built(EmpennageConfig::default());
        let thin = |span_fraction, thickness_scale| WingThicknessStation {
            span_fraction,
            thickness_scale,
        };
        let drawn = built(EmpennageConfig {
            vstab_thickness_stations: vec![thin(0.2, 1.0), thin(0.8, 0.5)],
            ..EmpennageConfig::default()
        });
        let (a, b) = (&plain.wings[2], &drawn.wings[2]);
        assert_eq!(a.reference_area(), b.reference_area());
        assert_eq!(a.xsecs.len(), b.xsecs.len());
        let sample: Vec<f64> = (1..40).map(|index| f64::from(index) / 40.0).collect();
        let tip = |wing: &Wing| {
            wing.xsecs
                .last()
                .expect("tip")
                .airfoil
                .max_thickness(&sample)
        };
        for (x, y) in a.xsecs.iter().zip(&b.xsecs) {
            assert_eq!(x.xyz_le, y.xyz_le);
            assert_eq!(x.chord, y.chord);
        }
        assert!(
            (tip(b) / tip(a) - 0.5).abs() < 0.01,
            "{} {}",
            tip(b),
            tip(a)
        );
        assert_eq!(plain.wings[1], drawn.wings[1]);
        let geometry = GeometryConfig {
            empennage: EmpennageConfig {
                vstab_thickness_stations: vec![thin(0.8, 1.0), thin(0.2, 0.5)],
                ..EmpennageConfig::default()
            },
            ..GeometryConfig::default()
        };
        assert!(AircraftBuilder::new(Some(geometry))
            .build(Some(&DesignVector::default()), false)
            .is_err());
    }

    #[test]
    fn stations_move_the_built_sections_and_keep_the_straight_reference_planform() {
        let plain = built(EmpennageConfig::default());
        let drawn = built(EmpennageConfig {
            hstab_edge_stations: vec![station(0.95, 0.2, 0.0), station(1.0, 0.8, 0.0)],
            vstab_edge_stations: vec![station(0.9, 0.0, -0.1), station(1.0, 0.5, -1.0)],
            ..EmpennageConfig::default()
        });
        for index in [1, 2] {
            let (a, b) = (&plain.wings[index], &drawn.wings[index]);
            let reference = b.reference_planform();
            // The area is a sum over the panels, and the drawn stations change the panel list, so
            // the two sums agree to rounding (1e-16 relative), not to the bit.
            assert!(
                (a.reference_area() - b.reference_area()).abs() <= 1e-12 * a.reference_area(),
                "{} vs {}",
                a.reference_area(),
                b.reference_area()
            );
            assert_eq!(a.taper_ratio(), b.taper_ratio());
            assert_eq!(a.mean_sweep_angle(0.25), b.mean_sweep_angle(0.25));
            assert!((a.unfolded_area() - reference.unfolded_area()).abs() < 1e-9);
            let ac = a.aerodynamic_center(0.25);
            let ac_reference = reference.aerodynamic_center(0.25);
            assert!((ac[0] - ac_reference[0]).abs() < 1e-9);
            assert!(a.unfolded_area() != b.unfolded_area());
            assert_eq!(b.section_outlines(Some(30)).len(), b.xsecs.len());
        }
        let htp = &drawn.wings[1];
        let line = plain.wings[1].xsecs.last().expect("tip");
        let tip = htp.xsecs.last().expect("built tip");
        assert_eq!(tip.xyz_le[1], line.xyz_le[1]);
        assert!((tip.xyz_le[0] - line.xyz_le[0] - 0.8).abs() < 1e-12);
        assert!((tip.chord - line.chord + 0.8).abs() < 1e-12);
        let fin = &drawn.wings[2];
        let line = plain.wings[2].xsecs.last().expect("tip");
        let tip = fin.xsecs.last().expect("built tip");
        assert_eq!(tip.xyz_le[2], line.xyz_le[2]);
        assert!((tip.chord - line.chord + 1.5).abs() < 1e-12);
    }

    #[test]
    fn the_runs_between_stations_keep_the_strip_width_of_the_straight_mesh() {
        let plain = built(EmpennageConfig::default());
        let drawn = built(EmpennageConfig {
            vstab_edge_stations: vec![station(0.05, 0.1, 0.0), station(0.95, 0.0, -0.1)],
            ..EmpennageConfig::default()
        });
        let height = |wing: &crate::aircraft::wing::Wing| {
            wing.xsecs
                .windows(2)
                .map(|pair| pair[1].xyz_le[2] - pair[0].xyz_le[2])
                .fold(0.0, f64::max)
        };
        assert!(height(&drawn.wings[2]) < 1.3 * height(&plain.wings[2]));
    }

    #[test]
    fn a_leading_edge_pulled_forward_is_held_down_to_the_root_and_closes_the_dorsal() {
        let plain = built(EmpennageConfig {
            vstab_dorsal_length_m: Some(4.0),
            vstab_dorsal_height_m: Some(3.0),
            ..EmpennageConfig::default()
        });
        let empennage = EmpennageConfig {
            vstab_dorsal_length_m: Some(4.0),
            vstab_dorsal_height_m: Some(3.0),
            vstab_edge_stations: vec![station(0.3, -0.5, 0.0), station(0.6, 0.0, 0.0)],
            ..EmpennageConfig::default()
        };
        let drawn = built(empennage);
        let fin = &drawn.wings[2];
        let at_station = fin
            .xsecs
            .iter()
            .position(|section| section.xyz_le[0] < plain.wings[2].xsecs[0].xyz_le[0] + 1e-9);
        assert!(at_station.is_some());
        for pair in fin.xsecs.windows(2) {
            assert!(pair[1].xyz_le[0] >= pair[0].xyz_le[0] - 1e-12);
        }
        let root_te =
            |wing: &crate::aircraft::wing::Wing| wing.xsecs[0].xyz_le[0] + wing.xsecs[0].chord;
        assert!((root_te(fin) - root_te(&plain.wings[2])).abs() < 1e-9);
        let dorsal = drawn
            .wings
            .iter()
            .find(|wing| wing.is_dorsal_fin())
            .expect("dorsal");
        let fin_le_x = |z: f64| {
            fin.xsecs
                .windows(2)
                .find(|pair| z <= pair[1].xyz_le[2])
                .map(|pair| {
                    let s = (z - pair[0].xyz_le[2]) / (pair[1].xyz_le[2] - pair[0].xyz_le[2]);
                    pair[0].xyz_le[0] + s.max(0.0) * (pair[1].xyz_le[0] - pair[0].xyz_le[0])
                })
        };
        for section in &dorsal.reference_planform().xsecs {
            let te = section.xyz_le[0] + section.chord;
            let on_fin = fin_le_x(section.xyz_le[2]).expect("under the fin tip");
            assert!(
                te <= on_fin + 1e-6,
                "the skin runs into the fin at z {}",
                section.xyz_le[2]
            );
        }
    }

    #[test]
    fn the_section_count_does_not_depend_on_the_tail_scale() {
        let empennage = EmpennageConfig {
            vstab_edge_stations: vec![
                station(0.01, -0.4, 0.0),
                station(0.3, -0.5, 0.0),
                station(0.6, 0.0, 0.0),
            ],
            ..EmpennageConfig::default()
        };
        let geometry = GeometryConfig {
            empennage,
            ..GeometryConfig::default()
        };
        let builder = AircraftBuilder::new(Some(geometry));
        let counts: Vec<usize> = (0..=40)
            .map(|index| {
                let design = DesignVector {
                    tail_scale: 0.6 + 0.02 * f64::from(index),
                    ..DesignVector::default()
                };
                let plane = builder.build(Some(&design), false).expect("builds");
                plane.wings[2].xsecs.len()
            })
            .collect();
        assert!(counts.iter().all(|count| *count == counts[0]), "{counts:?}");
    }

    #[test]
    fn a_station_that_closes_the_chord_is_rejected() {
        let geometry = GeometryConfig {
            empennage: EmpennageConfig {
                hstab_edge_stations: vec![station(1.0, 50.0, 0.0)],
                ..EmpennageConfig::default()
            },
            ..GeometryConfig::default()
        };
        assert!(AircraftBuilder::new(Some(geometry))
            .build(Some(&DesignVector::default()), false)
            .is_err());
    }
}
