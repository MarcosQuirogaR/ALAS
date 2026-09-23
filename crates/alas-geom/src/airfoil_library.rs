// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/geometry/airfoils.py.
// Reference: alas @ rust-port-baseline.

//! Airfoil name resolution and parametric shaping.
//!
//! [`AirfoilLibrary::get`] is `AirfoilLibrary.get`: resolve a name through
//! the user registry, the Selig zip corpus ([`crate::selig`]), the built-in
//! reference sections ([`crate::airfoil_data`]), then native aerodynamic
//! model's NACA fallback ([`crate::aircraft::airfoil::Airfoil::from_name`]),
//! normalizing whichever one
//! answers. [`apply_bumps`] adds four localized Hicks-Henne-style
//! perturbations; [`morph_airfoil`] scales thickness and camber
//! independently; [`build_section`] is the pipeline the wing root and break
//! sections are actually built from, driven by a [`DesignVector`].
//!
//! # Thread safety
//!
//! Upstream guards its zip index's first-time population with a
//! `threading.Lock` and a double-checked `None` test, because two threads
//! racing into `_init_zip` could otherwise publish a partially populated
//! cache. [`crate::selig`] has no equivalent lock to reproduce: its own
//! one-time parse is cached behind a `std::sync::OnceLock`, which gives the
//! same guarantee (the corpus is fully built before any reader can observe
//! it) without a caller-visible lock.

use crate::aircraft::airfoil::{numpy_interp, Airfoil};
use crate::aircraft::spacing::linspace;
use crate::airfoil_data;
use crate::airfoil_io;
use crate::selig;
use alas_config::DesignVector;
use alas_math::CubicSplineError;

/// The chordwise centres (x/c) of the four bump functions [`apply_bumps`]
/// applies: `_BUMP_CENTERS` in the Python source.
struct BumpCenters {
    upper_front: f64,
    upper_rear: f64,
    lower_mid: f64,
    lower_rear: f64,
}

const BUMP_CENTERS: BumpCenters = BumpCenters {
    upper_front: 0.25,
    upper_rear: 0.75,
    lower_mid: 0.40,
    lower_rear: 0.85,
};

/// `apply_bumps`'s default `n_points_per_side`. Rust has no default-argument
/// syntax; [`build_section`] passes this explicitly, as upstream's
/// unqualified call does implicitly.
pub const DEFAULT_APPLY_BUMPS_N_POINTS_PER_SIDE: usize = 120;

/// `morph_airfoil`'s default `n_points`; see
/// [`DEFAULT_APPLY_BUMPS_N_POINTS_PER_SIDE`] for why this is a named
/// constant rather than a default parameter.
pub const DEFAULT_MORPH_N_POINTS: usize = 150;

/// Resolves airfoil names into [`Airfoil`]s, trying ALAS's own corpora
/// before native aerodynamic model's name resolution.
pub struct AirfoilLibrary;

impl AirfoilLibrary {
    /// Return an airfoil by name: `AirfoilLibrary.get`.
    ///
    /// Tries, in order: registered custom airfoils (case-insensitive), the
    /// Selig zip corpus (case-insensitive), built-in named reference sections
    /// (exact case), and native aerodynamic model's NACA-only fallback. Each hit is normalized with
    /// [`normalize_coordinates`] before being returned.
    ///
    /// Returns `None` when none of the four resolve `name`. Upstream's
    /// fourth branch does not raise on an unresolved name either:
    /// The reference `Airfoil(name)` constructor there constructs an `Airfoil` whose
    /// `coordinates` is `None`, but this crate's scoped
    /// [`Airfoil::from_name`] already collapses that outcome to `None`
    /// rather than a placeholder object, so propagating it here is the
    /// faithful continuation of the same collapse, not a new one. Every
    /// name this program's own configuration ever resolves through
    /// `AirfoilLibrary.get` reaches one of the first four branches (see
    /// `docs/PORTING.md`, Geometry), so this path is not reachable from this
    /// program's own inputs.
    pub fn get(name: &str) -> Option<Airfoil> {
        if let Some(airfoil) = airfoil_io::get(name) {
            return Some(airfoil);
        }
        if let Some((stem, coordinates)) = selig::get(name) {
            return Some(Airfoil::from_coordinates(
                stem,
                normalize_coordinates(coordinates),
            ));
        }
        if let Some(coordinates) = airfoil_data::get(name) {
            return Some(Airfoil::from_coordinates(
                name,
                normalize_coordinates(coordinates),
            ));
        }
        if let Some(airfoil) = Airfoil::from_name(name) {
            let normalized = normalize_coordinates(&airfoil.coordinates);
            return Some(Airfoil::from_coordinates(airfoil.name, normalized));
        }
        None
    }

    /// Return the list of all available airfoil names in the Selig corpus and named registry.
    pub fn get_available_airfoils() -> Vec<&'static str> {
        let mut names = Vec::with_capacity(
            selig::stems().len() + airfoil_data::names().len() + airfoil_io::names().len(),
        );
        names.extend(selig::stems());
        names.extend_from_slice(airfoil_data::names());
        names.extend(airfoil_io::names());
        names.sort_unstable();
        names.dedup();
        names
    }
}

/// The index of `coords`'s leading-edge point: the smallest `x`, first
/// occurrence on a tie: `np.argmin(coords[:, 0])`.
///
/// A free function rather than a method on [`Airfoil`], because both
/// [`normalize_coordinates`] and [`morph_airfoil`] apply it to a raw
/// coordinate slice that may not be in an `Airfoil`'s stored convention yet:
/// that is the point of calling it at all.
fn le_index(coords: &[(f64, f64)]) -> usize {
    let mut best_index = 0;
    let mut best_x = f64::INFINITY;
    for (index, &(x, _)) in coords.iter().enumerate() {
        if x < best_x {
            best_x = x;
            best_index = index;
        }
    }
    best_index
}

/// `coords` sorted ascending by `x`: `points[np.argsort(points[:, 0])]`.
///
/// A stable sort. NumPy's default `argsort` is not guaranteed stable, but
/// every caller here only relies on the sort to place equal-`x` points
/// adjacently for interpolation or for the ascending/descending surface
/// order; real coordinate data has no repeated `x` within one segment to
/// make the distinction observable.
fn sorted_ascending_by_x(points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    sorted
}

/// The Euclidean distance between two coordinate pairs: `np.linalg.norm`
/// applied to their difference.
fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// Reorder an arbitrary-order coordinate array into upper TE -> LE -> lower
/// TE: `AirfoilLibrary.normalize_coordinates`.
///
/// Splits `coords` into two segments at its leading-edge index (the
/// smallest `x`), independently re-sorts each by ascending `x` to find its
/// midchord (`x = 0.5`) height by linear interpolation, and calls whichever
/// segment has the larger midchord height "upper". The upper segment is then
/// sorted descending by `x` and the lower ascending, and if the two
/// segments' innermost points (the upper segment's smallest-`x` point and
/// the lower segment's smallest-`x` point) are within `1e-6` of each other,
/// one copy is dropped before concatenating.
///
/// Because both segments are built to include `coords[le_idx]` (`seg1 =
/// coords[..=le_idx]`, `seg2 = coords[le_idx..]`), those two "innermost
/// points" are the same leading-edge row read out of each segment, so this
/// check is, in practice, always true for real input; it is translated as
/// the ordinary runtime comparison Python performs, not special-cased away.
pub fn normalize_coordinates(coords: &[(f64, f64)]) -> Vec<(f64, f64)> {
    if coords.len() < 3 {
        return coords.to_vec();
    }

    let le_idx = le_index(coords);
    let seg1 = &coords[..=le_idx];
    let seg2 = &coords[le_idx..];

    let seg1_sorted = sorted_ascending_by_x(seg1);
    let seg2_sorted = sorted_ascending_by_x(seg2);
    let seg1_x: Vec<f64> = seg1_sorted.iter().map(|point| point.0).collect();
    let seg1_y: Vec<f64> = seg1_sorted.iter().map(|point| point.1).collect();
    let seg2_x: Vec<f64> = seg2_sorted.iter().map(|point| point.0).collect();
    let seg2_y: Vec<f64> = seg2_sorted.iter().map(|point| point.1).collect();

    let y1_mid = numpy_interp(0.5, &seg1_x, &seg1_y);
    let y2_mid = numpy_interp(0.5, &seg2_x, &seg2_y);

    let (upper_seg, lower_seg) = if y1_mid >= y2_mid {
        (seg1, seg2)
    } else {
        (seg2, seg1)
    };

    let mut upper_sorted = sorted_ascending_by_x(upper_seg);
    upper_sorted.reverse();
    let lower_sorted = sorted_ascending_by_x(lower_seg);

    let mut coords_new = upper_sorted.clone();
    match (upper_sorted.last(), lower_sorted.first()) {
        (Some(&last_upper), Some(&first_lower)) if distance(last_upper, first_lower) < 1e-6 => {
            coords_new.extend(lower_sorted.iter().skip(1));
        }
        _ => coords_new.extend(lower_sorted.iter()),
    }
    coords_new
}

/// One Hicks-Henne-style bump: `amp * sin(pi*x)^2.5 * exp(-10*(x-center)^2)`,
/// added only where `0.01 < x < 0.99`: the inner loop of `apply_bumps`'s
/// `add_bump`.
///
/// `amp == 0.0` returns `y_arr` unchanged without evaluating the
/// perturbation at all, matching Python's early return; this is what makes
/// [`apply_bumps`] with every amplitude at zero equal to a plain `repanel`.
fn add_bump(x_arr: &[f64], y_arr: &[f64], amp: f64, center_x: f64) -> Vec<f64> {
    if amp == 0.0 {
        return y_arr.to_vec();
    }
    const WIDTH: f64 = 2.5;
    x_arr
        .iter()
        .zip(y_arr.iter())
        .map(|(&x, &y)| {
            if x > 0.01 && x < 0.99 {
                let x_safe = x.clamp(0.0, 1.0);
                let perturbation = amp
                    * (std::f64::consts::PI * x_safe).sin().powf(WIDTH)
                    * (-10.0 * (x_safe - center_x).powi(2)).exp();
                y + perturbation
            } else {
                y
            }
        })
        .collect()
}

/// Add Hicks-Henne-style bumps to the upper and lower surfaces:
/// `apply_bumps`.
///
/// `bumps_upper` is `[front, rear]` at `x/c` 0.25 and 0.75; `bumps_lower` is
/// `[mid, rear]` at `x/c` 0.40 and 0.85: [`BUMP_CENTERS`]. `coords` is
/// repaneled to `n_points_per_side` points per surface first
/// ([`Airfoil::repanel`]), so the bump math always runs on a known,
/// cosine-spaced grid regardless of the input's original panelling.
///
/// # Errors
///
/// [`CubicSplineError`] if `repanel` fails, see [`Airfoil::repanel`].
pub fn apply_bumps(
    coords: &[(f64, f64)],
    bumps_upper: [f64; 2],
    bumps_lower: [f64; 2],
    n_points_per_side: usize,
) -> Result<Airfoil, CubicSplineError> {
    let repaneled =
        Airfoil::from_coordinates("scratch", coords.to_vec()).repanel(n_points_per_side)?;

    let mut x_up: Vec<f64> = repaneled.upper_coordinates().iter().map(|p| p.0).collect();
    let mut y_up: Vec<f64> = repaneled.upper_coordinates().iter().map(|p| p.1).collect();
    // native aerodynamic model returns the upper surface LE->TE or TE->LE depending on
    // version; normalise to ascending-x for the bump math, then restore
    // orientation before rebuilding the coordinate loop.
    let flip_up =
        matches!((x_up.first(), x_up.last()), (Some(&first), Some(&last)) if first > last);
    if flip_up {
        x_up.reverse();
        y_up.reverse();
    }

    let x_lo: Vec<f64> = repaneled.lower_coordinates().iter().map(|p| p.0).collect();
    let mut y_lo: Vec<f64> = repaneled.lower_coordinates().iter().map(|p| p.1).collect();

    y_up = add_bump(&x_up, &y_up, bumps_upper[0], BUMP_CENTERS.upper_front);
    y_up = add_bump(&x_up, &y_up, bumps_upper[1], BUMP_CENTERS.upper_rear);
    y_lo = add_bump(&x_lo, &y_lo, bumps_lower[0], BUMP_CENTERS.lower_mid);
    y_lo = add_bump(&x_lo, &y_lo, bumps_lower[1], BUMP_CENTERS.lower_rear);

    let upper_new: Vec<(f64, f64)> = if flip_up {
        x_up.into_iter().rev().zip(y_up.into_iter().rev()).collect()
    } else {
        x_up.into_iter().zip(y_up).collect()
    };
    let lower_new: Vec<(f64, f64)> = x_lo.into_iter().zip(y_lo).collect();

    let mut coords_new = upper_new.clone();
    match (upper_new.last(), lower_new.first()) {
        (Some(&last_upper), Some(&first_lower)) if distance(last_upper, first_lower) < 1e-6 => {
            coords_new.extend(lower_new.into_iter().skip(1));
        }
        _ => coords_new.extend(lower_new),
    }

    Ok(Airfoil::from_coordinates("bumped", coords_new))
}

/// Scale an airfoil's thickness and camber independently: `morph_airfoil`.
///
/// Splits `coords` at its leading-edge index (raw slicing, like
/// [`normalize_coordinates`]'s split, not [`Airfoil::upper_coordinates`],
/// since `coords` here is a raw array that may not share an `Airfoil`'s
/// storage convention), resamples both halves onto a shared
/// `linspace(0, 1, n_points)` grid, decomposes into thickness and camber,
/// scales each, and reassembles.
pub fn morph_airfoil(
    coords: &[(f64, f64)],
    thickness_scale: f64,
    camber_scale: f64,
    n_points: usize,
) -> Airfoil {
    let le_idx = le_index(coords);
    let upper = &coords[..=le_idx];
    let lower = &coords[le_idx..];

    let x_grid = linspace(0.0, 1.0, n_points);

    let upper_sorted = sorted_ascending_by_x(upper);
    let lower_sorted = sorted_ascending_by_x(lower);
    let upper_x: Vec<f64> = upper_sorted.iter().map(|point| point.0).collect();
    let upper_y: Vec<f64> = upper_sorted.iter().map(|point| point.1).collect();
    let lower_x: Vec<f64> = lower_sorted.iter().map(|point| point.0).collect();
    let lower_y: Vec<f64> = lower_sorted.iter().map(|point| point.1).collect();

    let y_upper: Vec<f64> = x_grid
        .iter()
        .map(|&x| numpy_interp(x, &upper_x, &upper_y))
        .collect();
    let y_lower: Vec<f64> = x_grid
        .iter()
        .map(|&x| numpy_interp(x, &lower_x, &lower_y))
        .collect();

    let mut new_y_upper = Vec::with_capacity(n_points);
    let mut new_y_lower = Vec::with_capacity(n_points);
    for i in 0..n_points {
        let thickness = (y_upper[i] - y_lower[i]) * thickness_scale;
        let camber = ((y_upper[i] + y_lower[i]) / 2.0) * camber_scale;
        new_y_upper.push(camber + thickness / 2.0);
        new_y_lower.push(camber - thickness / 2.0);
    }

    // Upper surface descending in x (`x_grid[::-1]`), then the lower
    // surface ascending, skipping its first point since it repeats the
    // shared leading-edge station (`x_grid[1:]`).
    let mut coordinates: Vec<(f64, f64)> = x_grid
        .iter()
        .rev()
        .zip(new_y_upper.iter().rev())
        .map(|(&x, &y)| (x, y))
        .collect();
    coordinates.extend(
        x_grid
            .iter()
            .zip(new_y_lower.iter())
            .skip(1)
            .map(|(&x, &y)| (x, y)),
    );

    Airfoil::from_coordinates("morphed", coordinates)
}

/// Produce the working wing section from a design vector: `build_section`.
///
/// Pipeline: `base_coords` -> [`apply_bumps`] -> [`morph_airfoil`], reading
/// the four bump amplitudes and the two morph scales off `dv`. This is the
/// section used at the wing root and break.
///
/// # Errors
///
/// [`CubicSplineError`] if [`apply_bumps`]'s `repanel` step fails.
pub fn build_section(
    dv: &DesignVector,
    base_coords: &[(f64, f64)],
) -> Result<Airfoil, CubicSplineError> {
    let bumped = apply_bumps(
        base_coords,
        [dv.bump_upper_front, dv.bump_upper_rear],
        [dv.bump_lower_mid, dv.bump_lower_rear],
        DEFAULT_APPLY_BUMPS_N_POINTS_PER_SIDE,
    )?;
    Ok(morph_airfoil(
        &bumped.coordinates,
        dv.airfoil_thickness_scale,
        dv.airfoil_camber_scale,
        DEFAULT_MORPH_N_POINTS,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_returns_none_for_a_name_none_of_the_four_branches_resolve() {
        assert!(AirfoilLibrary::get("not-a-real-airfoil-name").is_none());
    }

    #[test]
    fn get_resolves_corpus_reference_and_naca_branches() {
        // Selig zip corpus (case-insensitive).
        let tip = AirfoilLibrary::get("NACA2410").expect("naca2410 is in the selig corpus");
        assert_eq!(tip.name, "naca2410");
        // Built-in named coordinates (exact case).
        let root = AirfoilLibrary::get("SC2-0714").expect("SC2-0714 is a named section");
        assert_eq!(root.name, "SC2-0714");
        // native aerodynamic model NACA fallback.
        let tail = AirfoilLibrary::get("naca0012").expect("naca0012 parses as 4-digit NACA");
        assert_eq!(tail.name, "naca0012");
    }

    #[test]
    fn normalize_coordinates_is_a_no_op_on_an_already_correctly_ordered_loop() {
        // A minimal upper-TE -> LE -> lower-TE loop, already canonical.
        let coords = vec![
            (1.0, 0.05),
            (0.5, 0.1),
            (0.0, 0.0),
            (0.5, -0.1),
            (1.0, -0.05),
        ];
        let normalized = normalize_coordinates(&coords);
        assert_eq!(normalized, coords);
    }

    #[test]
    fn normalize_coordinates_drops_exactly_one_duplicate_leading_edge_point() {
        // Both split segments contain coords[le_idx], double-counting it, so
        // the two segments together hold one more point than `coords` does
        // (3 + 3 = 6 for this 5-point loop). The drop branch removes exactly
        // that one duplicate, landing back at `coords.len()`, not fewer
        // (which would mean a real point was lost) and not that
        // uncorrected `+1` (which would mean the duplicate survived).
        let coords = vec![
            (1.0, 0.05),
            (0.5, 0.1),
            (0.0, 0.0),
            (0.5, -0.1),
            (1.0, -0.05),
        ];
        let normalized = normalize_coordinates(&coords);
        assert_eq!(normalized.len(), coords.len());
    }

    #[test]
    fn normalize_coordinates_picks_upper_as_the_segment_with_the_larger_midchord_height() {
        let coords = vec![
            (1.0, 0.05),
            (0.5, 0.1),
            (0.0, 0.0),
            (0.5, -0.1),
            (1.0, -0.05),
        ];
        let normalized = normalize_coordinates(&coords);
        // The upper surface leads (descending x from the trailing edge).
        assert_eq!(normalized[0], (1.0, 0.05));
        assert!(normalized[1].1 > 0.0);
    }

    #[test]
    fn normalize_coordinates_leaves_a_short_input_untouched() {
        let coords = vec![(1.0, 0.0), (0.0, 0.0)];
        assert_eq!(normalize_coordinates(&coords), coords);
    }

    #[test]
    fn apply_bumps_with_every_amplitude_zero_matches_a_plain_repanel() {
        let base = AirfoilLibrary::get("SC2-0714").expect("SC2-0714 is a named section");
        let bumped = apply_bumps(&base.coordinates, [0.0, 0.0], [0.0, 0.0], 40)
            .expect("SC2-0714 repanels cleanly");
        let repaneled = Airfoil::from_coordinates("scratch", base.coordinates.clone())
            .repanel(40)
            .expect("SC2-0714 repanels cleanly");
        assert_eq!(bumped.coordinates.len(), repaneled.coordinates.len());
        for (bumped_point, repaneled_point) in bumped.coordinates.iter().zip(&repaneled.coordinates)
        {
            assert!((bumped_point.0 - repaneled_point.0).abs() < 1e-12);
            assert!((bumped_point.1 - repaneled_point.1).abs() < 1e-12);
        }
    }

    #[test]
    fn morph_airfoil_with_unit_scales_reproduces_the_resampled_surfaces() {
        let base = AirfoilLibrary::get("SC2-0714").expect("SC2-0714 is a named section");
        let morphed = morph_airfoil(&base.coordinates, 1.0, 1.0, 60);
        assert_eq!(morphed.coordinates.len(), 2 * 60 - 1);
        // The chord still runs from 1.0 down to 0.0 and back to 1.0.
        assert!((morphed.coordinates[0].0 - 1.0).abs() < 1e-12);
        assert!((morphed.coordinates[59].0 - 0.0).abs() < 1e-12);
        assert!((morphed.coordinates.last().expect("non-empty").0 - 1.0).abs() < 1e-12);
    }

    #[test]
    fn build_section_at_the_default_design_vector_only_repanels_and_rescales_by_one() {
        let base = AirfoilLibrary::get("SC2-0714").expect("SC2-0714 is a named section");
        let dv = DesignVector::default();
        let section =
            build_section(&dv, &base.coordinates).expect("SC2-0714 repanels and morphs cleanly");
        assert_eq!(section.coordinates.len(), 2 * DEFAULT_MORPH_N_POINTS - 1);
    }
}
