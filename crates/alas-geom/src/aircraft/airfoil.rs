// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from reference geometry/geometry/airfoil/airfoil.py,
// reference geometry/geometry/airfoil/airfoil_families.py
// Upstream: reference geometry 4.2.8, MIT.
// Reference: alas @ rust-port-baseline.

//! reference geometry's `Airfoil`, scoped to the surface this program's Python
//! package actually calls: construction from explicit coordinates or from a
//! 4-digit NACA name, the upper/lower surface split, cosine-spaced
//! repaneling through a cubic spline, thickness sampling, and the
//! normalization every airfoil surrogate query starts with (see
//! [`Airfoil::normalize`] and the [`normalize`] submodule).
//!
//! Everything else `Airfoil` offers upstream -- XFoil/NeuralFoil polars,
//! Kulfan parameterization, plotting, `LE_radius`, `TE_angle`,
//! `TE_thickness` -- is out of scope; a prior research pass grepped every
//! call this program's Python package makes onto an `Airfoil` instance and
//! none of those reach this module. `docs/PORTING.md` records the scoping
//! decision. (`get_aero_from_neuralfoil` is reached, and is
//! `alas-aero::neuralfoil`'s row rather than this one: it is a query about an
//! airfoil, not a property of one, and it needs a hundred kilobytes of
//! network weights that have no business in a geometry crate.)
//!
//! # The NACA-only name-resolution fallback
//!
//! Upstream's `Airfoil(name, coordinates=None)` tries three sources in order:
//! the closed-form 4-digit NACA generator, then the UIUC coordinate database,
//! then (for `coordinates` given as a path) a `.dat` file on disk. Every
//! airfoil name this program's own configuration resolves through that
//! fallback (checked against `alas/config/presets.py`) is `"naca0012"`, which
//! only ever reaches the first branch, so [`Airfoil::from_name`] implements
//! only that one. A name that is not a parseable 4-digit NACA designation
//! returns `None` here -- matching the Python constructor's own
//! `except (ValueError, NotImplementedError):` on that branch -- rather than
//! falling through to a UIUC lookup or a file read, both of which are
//! unreached in practice and are not translated. A future caller that needs
//! an arbitrary named or file-backed airfoil should reach for
//! `alas-geom::selig` (the UIUC corpus) or extend this module deliberately,
//! not assume the fallback silently continues past NACA.
//!
//! # Repanel's spacing function
//!
//! Upstream's `repanel` takes `spacing_function_per_side` as a parameter,
//! defaulting to `np.cosspace`. Both call sites this crate serves
//! (`apply_bumps` in `airfoils.py`, and `mses_analysis.py`) use that default,
//! so [`Airfoil::repanel`] hardcodes cosine spacing rather than translating a
//! pluggable-function parameter with only one caller.

mod normalize;

pub use normalize::Normalization;

use alas_math::{Boundary, CubicSpline, CubicSplineError};

use super::spacing::cosspace;
#[cfg(test)]
use super::spacing::linspace;

/// `_default_n_points_per_side` in `airfoil_families.py`: how many points
/// [`Airfoil::from_name`] generates per surface when it resolves a NACA name.
const DEFAULT_N_POINTS_PER_SIDE: usize = 200;

/// An airfoil section: a name and its `(x, y)` coordinates.
///
/// Coordinates are expected in the standard airfoil order -- starting on the
/// upper surface at the trailing edge, forward over the upper surface, around
/// the nose, aft over the lower surface, back to the trailing edge -- which
/// is what [`Airfoil::le_index`], [`Airfoil::upper_coordinates`] and
/// [`Airfoil::lower_coordinates`] assume. Nothing in this module enforces
/// that order; it is a precondition inherited from upstream, which does not
/// enforce it either.
#[derive(Debug, Clone, PartialEq)]
pub struct Airfoil {
    /// The airfoil's name, e.g. `"naca2412"`.
    pub name: String,
    /// `(x, y)` coordinates, normalized to unit chord by convention but not
    /// enforced here.
    pub coordinates: Vec<(f64, f64)>,
}

impl Airfoil {
    /// An airfoil built from explicit coordinates, taken as given.
    pub fn from_coordinates(name: impl Into<String>, coordinates: Vec<(f64, f64)>) -> Self {
        Self {
            name: name.into(),
            coordinates,
        }
    }

    /// An airfoil resolved from `name` alone, via the 4-digit NACA analytical
    /// formula at [`DEFAULT_N_POINTS_PER_SIDE`] points per side.
    ///
    /// Returns `None` when `name` does not parse as a 4-digit NACA
    /// designation; see the module documentation for why nothing else is
    /// tried after that, unlike upstream.
    pub fn from_name(name: impl Into<String>) -> Option<Self> {
        let name = name.into();
        let coordinates = naca_coordinates(&name, DEFAULT_N_POINTS_PER_SIDE)?;
        Some(Self { name, coordinates })
    }

    /// The index of the leading-edge point: the coordinate with the smallest
    /// `x`, first occurrence on a tie -- `np.argmin(self.x())`.
    ///
    /// Returns `0` for an airfoil with no coordinates, which is the only way
    /// this can avoid panicking on that input; nothing in this crate
    /// constructs an `Airfoil` with empty coordinates.
    pub fn le_index(&self) -> usize {
        let mut best_index = 0;
        let mut best_x = f64::INFINITY;
        for (index, &(x, _)) in self.coordinates.iter().enumerate() {
            if x < best_x {
                best_x = x;
                best_index = index;
            }
        }
        best_index
    }

    /// The upper surface, trailing edge to leading edge inclusive.
    ///
    /// Includes the leading-edge point; see [`Airfoil::lower_coordinates`],
    /// which also includes it, for the duplicate this creates if both are
    /// concatenated naively.
    pub fn upper_coordinates(&self) -> &[(f64, f64)] {
        if self.coordinates.is_empty() {
            return &[];
        }
        &self.coordinates[..=self.le_index()]
    }

    /// The lower surface, leading edge to trailing edge inclusive.
    ///
    /// Includes the leading-edge point; see [`Airfoil::upper_coordinates`].
    pub fn lower_coordinates(&self) -> &[(f64, f64)] {
        &self.coordinates[self.le_index()..]
    }

    /// The airfoil's thickness (upper `y` minus lower `y`) at each `x/c`
    /// station in `x_over_c`, by piecewise-linear interpolation of each
    /// surface -- `local_thickness`.
    ///
    /// Stations outside a surface's own `x` range clamp to that surface's
    /// nearest endpoint value, matching `numpy.interp`'s default (no `left`
    /// or `right` override is passed upstream).
    pub fn local_thickness(&self, x_over_c: &[f64]) -> Vec<f64> {
        // Both surfaces run leading edge to trailing edge here, ascending in
        // x, which is what `numpy.interp` requires of its `xp` argument;
        // `upper_coordinates` is stored trailing-to-leading, so it is
        // reversed first, exactly as `Airfoil.local_thickness` reverses it.
        let mut upper: Vec<(f64, f64)> = self.upper_coordinates().to_vec();
        upper.reverse();
        let lower = self.lower_coordinates();

        let upper_x: Vec<f64> = upper.iter().map(|point| point.0).collect();
        let upper_y: Vec<f64> = upper.iter().map(|point| point.1).collect();
        let lower_x: Vec<f64> = lower.iter().map(|point| point.0).collect();
        let lower_y: Vec<f64> = lower.iter().map(|point| point.1).collect();

        x_over_c
            .iter()
            .map(|&x| numpy_interp(x, &upper_x, &upper_y) - numpy_interp(x, &lower_x, &lower_y))
            .collect()
    }

    /// The airfoil's mean camber line ordinate (upper `y` plus lower `y`,
    /// halved) at each `x/c` station in `x_over_c`, by the same
    /// piecewise-linear per-surface interpolation as [`Airfoil::local_thickness`]
    /// -- `local_camber`. Stations outside a surface's own `x` range clamp
    /// the same way.
    pub fn local_camber(&self, x_over_c: &[f64]) -> Vec<f64> {
        let mut upper: Vec<(f64, f64)> = self.upper_coordinates().to_vec();
        upper.reverse();
        let lower = self.lower_coordinates();

        let upper_x: Vec<f64> = upper.iter().map(|point| point.0).collect();
        let upper_y: Vec<f64> = upper.iter().map(|point| point.1).collect();
        let lower_x: Vec<f64> = lower.iter().map(|point| point.0).collect();
        let lower_y: Vec<f64> = lower.iter().map(|point| point.1).collect();

        x_over_c
            .iter()
            .map(|&x| {
                (numpy_interp(x, &upper_x, &upper_y) + numpy_interp(x, &lower_x, &lower_y)) / 2.0
            })
            .collect()
    }

    /// The maximum of [`Airfoil::local_thickness`] over `x_over_c_sample`.
    ///
    /// Upstream defaults `x_over_c_sample` to `np.linspace(0, 1, 101)`; Rust
    /// has no default-argument syntax to carry that in the signature, so a
    /// caller that wants the upstream default builds it explicitly (this
    /// module's `spacing::linspace` produces the same grid).
    pub fn max_thickness(&self, x_over_c_sample: &[f64]) -> f64 {
        self.local_thickness(x_over_c_sample)
            .into_iter()
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// A copy of this airfoil, resampled to `n_points_per_side` cosine-spaced
    /// points on each surface (`n_points_per_side * 2 - 1` points in total,
    /// since the leading-edge point is shared).
    ///
    /// Each surface is fit with a cubic spline in streamwise arc length
    /// (`alas_math::CubicSpline`, built for exactly this call) and resampled
    /// at [`cosspace`]-spaced arc-length stations. The upper surface is
    /// pinned to a zero second derivative at the trailing edge and a first
    /// derivative of `(0, -1)` at the leading edge; the lower surface takes
    /// the same two conditions in the opposite order -- both matching
    /// `Airfoil.repanel`'s `scipy.interpolate.CubicSpline(..., bc_type=...)`
    /// calls exactly.
    ///
    /// # Errors
    ///
    /// [`CubicSplineError`] if either surface has fewer than two points or a
    /// repeated point (zero arc-length step), which is what upstream reports
    /// as "your Airfoil has a duplicate point" for the same underlying
    /// `CubicSpline` failure.
    pub fn repanel(&self, n_points_per_side: usize) -> Result<Self, CubicSplineError> {
        let upper = self.upper_coordinates(); // trailing edge -> leading edge
        let lower = self.lower_coordinates(); // leading edge -> trailing edge

        let upper_distances = cumulative_arc_length(upper);
        let lower_distances = cumulative_arc_length(lower);

        let upper_values: Vec<Vec<f64>> = upper.iter().map(|&(x, y)| vec![x, y]).collect();
        let lower_values: Vec<Vec<f64>> = lower.iter().map(|&(x, y)| vec![x, y]).collect();

        let upper_spline = CubicSpline::new(
            &upper_distances,
            &upper_values,
            Boundary::SecondDerivative(&[0.0, 0.0]),
            Boundary::FirstDerivative(&[0.0, -1.0]),
        )?;
        let lower_spline = CubicSpline::new(
            &lower_distances,
            &lower_values,
            Boundary::FirstDerivative(&[0.0, -1.0]),
            Boundary::SecondDerivative(&[0.0, 0.0]),
        )?;

        let upper_length = upper_distances.last().copied().unwrap_or(0.0);
        let lower_length = lower_distances.last().copied().unwrap_or(0.0);
        let upper_stations = cosspace(0.0, upper_length, n_points_per_side);
        let lower_stations = cosspace(0.0, lower_length, n_points_per_side);

        let new_upper: Vec<(f64, f64)> = upper_stations
            .iter()
            .map(|&station| {
                let point = upper_spline.evaluate(station);
                (point[0], point[1])
            })
            .collect();
        let new_lower: Vec<(f64, f64)> = lower_stations
            .iter()
            .map(|&station| {
                let point = lower_spline.evaluate(station);
                (point[0], point[1])
            })
            .collect();

        // Drop the duplicate leading-edge point the lower surface's first
        // entry would otherwise repeat -- `np.concatenate((new_upper,
        // new_lower[1:, :]), axis=0)`.
        let mut coordinates = new_upper;
        coordinates.extend(new_lower.into_iter().skip(1));

        Ok(Self {
            name: self.name.clone(),
            coordinates,
        })
    }

    /// A new airfoil blending this one with `other` -- `blend_with_another_airfoil`.
    ///
    /// Both airfoils are repaneled to `n_points_per_side` first (so the two
    /// coordinate arrays line up point-for-point), then each coordinate is a
    /// linear interpolation weighted `(1 - blend_fraction, blend_fraction)`.
    /// `blend_fraction = 0` reproduces `self`; `blend_fraction = 1` reproduces
    /// `other`. The name is built to match Python's
    /// `f"{a_fraction*100:.0f}% {self.name}, {b_fraction*100:.0f}% {other.name}"`
    /// exactly, including its rounding at the halfway point.
    ///
    /// # Errors
    ///
    /// [`CubicSplineError`] if either airfoil's `repanel` fails.
    pub fn blend_with_another_airfoil(
        &self,
        other: &Airfoil,
        blend_fraction: f64,
        n_points_per_side: usize,
    ) -> Result<Self, CubicSplineError> {
        let foil_a = self.repanel(n_points_per_side)?;
        let foil_b = other.repanel(n_points_per_side)?;
        let a_fraction = 1.0 - blend_fraction;
        let b_fraction = blend_fraction;

        let name = format!(
            "{:.0}% {}, {:.0}% {}",
            a_fraction * 100.0,
            self.name,
            b_fraction * 100.0,
            other.name,
        );

        let coordinates = foil_a
            .coordinates
            .iter()
            .zip(foil_b.coordinates.iter())
            .map(|(&(ax, ay), &(bx, by))| {
                (
                    a_fraction * ax + b_fraction * bx,
                    a_fraction * ay + b_fraction * by,
                )
            })
            .collect();

        Ok(Self { name, coordinates })
    }

    /// The airfoil as the text of a Selig-format `.dat` file -- `write_dat`.
    ///
    /// The name on the first line, then one `x y` line per coordinate formatted
    /// as Python's `"%f %f"` (six decimal places, the `%f` default), joined by
    /// newlines with no trailing newline -- byte-for-byte what
    /// `Airfoil.write_dat(include_name=True)` returns. Writing it to a path is
    /// the caller's job: upstream both writes the file and returns the string,
    /// and the one consumer here (`alas-aero::mses`, feeding `mset`) writes it
    /// itself, so this crate stays clear of file I/O. Scoped to
    /// `include_name=True`, the only form that consumer reaches -- `mset` reads
    /// a named `.dat`.
    pub fn write_dat(&self) -> String {
        let mut lines = Vec::with_capacity(self.coordinates.len() + 1);
        lines.push(self.name.clone());
        for &(x, y) in &self.coordinates {
            lines.push(format!("{x:.6} {y:.6}"));
        }
        lines.join("\n")
    }
}

/// The cumulative streamwise arc length from `points[0]`, one entry per
/// point: `[0, |p1-p0|, |p1-p0|+|p2-p1|, ...]` -- `np.diff` + `np.linalg.norm`
/// (per row) + `np.cumsum`, prefixed with the implicit zero at the first
/// point.
fn cumulative_arc_length(points: &[(f64, f64)]) -> Vec<f64> {
    if points.is_empty() {
        return Vec::new();
    }
    let mut distances = Vec::with_capacity(points.len());
    distances.push(0.0);
    let mut accumulated = 0.0;
    for pair in points.windows(2) {
        let dx = pair[1].0 - pair[0].0;
        let dy = pair[1].1 - pair[0].1;
        accumulated += dx.hypot(dy);
        distances.push(accumulated);
    }
    distances
}

/// Piecewise-linear interpolation matching `numpy.interp(x, xp, fp)`'s
/// default behaviour: `xp` assumed sorted ascending, and `x` outside
/// `[xp[0], xp[-1]]` clamped to the nearest endpoint's `fp` value rather than
/// extrapolated.
///
/// A linear scan rather than a binary search: every caller in this module
/// passes at most a few hundred points, and a scan needs no fallible
/// `f64` ordering to implement without panicking.
fn numpy_interp(x: f64, xp: &[f64], fp: &[f64]) -> f64 {
    let Some(&first_x) = xp.first() else {
        return f64::NAN; // `numpy.interp` itself raises on empty `xp`; nothing here calls it that way.
    };
    let last = xp.len() - 1;
    if x <= first_x {
        return fp[0];
    }
    if x >= xp[last] {
        return fp[last];
    }
    for i in 1..xp.len() {
        if x <= xp[i] {
            let (x0, x1) = (xp[i - 1], xp[i]);
            let (y0, y1) = (fp[i - 1], fp[i]);
            if x1 == x0 {
                return y0;
            }
            return y0 + (y1 - y0) * (x - x0) / (x1 - x0);
        }
    }
    fp[last]
}

/// The coordinates of a 4-digit NACA airfoil, or `None` if `name` does not
/// parse as one -- `get_NACA_coordinates(name=..., n_points_per_side=...)`,
/// scoped to its `name`-driven branch (this crate has no caller that supplies
/// `max_camber`/`camber_loc`/`thickness` directly).
///
/// Follows Wikipedia's "Equation for a cambered 4-digit NACA airfoil", as
/// upstream's comment cites: <https://en.wikipedia.org/wiki/NACA_airfoil#Equation_for_a_cambered_4-digit_NACA_airfoil>.
pub fn naca_coordinates(name: &str, n_points_per_side: usize) -> Option<Vec<(f64, f64)>> {
    let (max_camber, camber_loc, thickness) = parse_naca4(name)?;
    Some(generate_naca4(
        max_camber,
        camber_loc,
        thickness,
        n_points_per_side,
    ))
}

/// Parse a 4-digit NACA designation into `(max_camber, camber_loc,
/// thickness)`, each already scaled to a fraction of chord.
///
/// Mirrors `get_NACA_coordinates`'s parsing exactly, including its use of
/// `str.split("naca")` (every occurrence, not just a prefix match) and its
/// blanket rejection of anything but exactly 4 digits after that split --
/// `NotImplementedError` upstream for 5-digit and other NACA families, folded
/// into the same `None` here as an unparseable name, since this module has no
/// other family to fall back to.
fn parse_naca4(name: &str) -> Option<(f64, f64, f64)> {
    let lowered = name.to_lowercase();
    let trimmed = lowered.trim();
    if !trimmed.contains("naca") {
        return None;
    }
    let digits = trimmed.split("naca").nth(1)?;
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if digits.len() != 4 {
        return None;
    }

    let bytes = digits.as_bytes();
    let max_camber = f64::from(bytes[0] - b'0') * 0.01;
    let camber_loc = f64::from(bytes[1] - b'0') * 0.1;
    let thickness_hundredths: u32 = digits[2..].parse().ok()?;
    let thickness = f64::from(thickness_hundredths) * 0.01;

    Some((max_camber, camber_loc, thickness))
}

/// The closed-form 4-digit NACA section, cosine-spaced at `n_points_per_side`
/// points per surface.
fn generate_naca4(
    max_camber: f64,
    camber_loc: f64,
    thickness: f64,
    n_points_per_side: usize,
) -> Vec<(f64, f64)> {
    let x_t = cosspace(0.0, 1.0, n_points_per_side);

    let y_t: Vec<f64> = x_t
        .iter()
        .map(|&x| {
            5.0 * thickness
                * (0.2969 * x.sqrt() - 0.1260 * x - 0.3516 * x.powi(2) + 0.2843 * x.powi(3)
                    - 0.1015 * x.powi(4))
        })
        .collect();

    // Prevents a divide-by-zero for symmetric sections like naca0012, where
    // the second digit (camber location) is legitimately zero.
    let camber_loc = if camber_loc == 0.0 { 0.5 } else { camber_loc };

    let y_c: Vec<f64> = x_t
        .iter()
        .map(|&x| camber(x, max_camber, camber_loc))
        .collect();
    let camber_slope: Vec<f64> = x_t
        .iter()
        .map(|&x| camber_slope(x, max_camber, camber_loc))
        .collect();
    let theta: Vec<f64> = camber_slope.iter().map(|&slope| slope.atan()).collect();

    let mut x_upper = Vec::with_capacity(n_points_per_side);
    let mut y_upper = Vec::with_capacity(n_points_per_side);
    let mut x_lower = Vec::with_capacity(n_points_per_side);
    let mut y_lower = Vec::with_capacity(n_points_per_side);
    for i in 0..n_points_per_side {
        let (sin_theta, cos_theta) = theta[i].sin_cos();
        x_upper.push(x_t[i] - y_t[i] * sin_theta);
        y_upper.push(y_c[i] + y_t[i] * cos_theta);
        x_lower.push(x_t[i] + y_t[i] * sin_theta);
        y_lower.push(y_c[i] - y_t[i] * cos_theta);
    }

    // Upper surface runs trailing edge to leading edge (reversed); the lower
    // surface's shared leading-edge point is dropped so the two halves join
    // without a duplicate.
    x_upper.reverse();
    y_upper.reverse();

    let mut coordinates: Vec<(f64, f64)> = x_upper.into_iter().zip(y_upper).collect();
    coordinates.extend(x_lower.into_iter().skip(1).zip(y_lower.into_iter().skip(1)));
    coordinates
}

/// The mean-camber-line ordinate at `x/c`, piecewise about `camber_loc`.
fn camber(x: f64, max_camber: f64, camber_loc: f64) -> f64 {
    if x <= camber_loc {
        max_camber / camber_loc.powi(2) * (2.0 * camber_loc * x - x.powi(2))
    } else {
        max_camber / (1.0 - camber_loc).powi(2)
            * ((1.0 - 2.0 * camber_loc) + 2.0 * camber_loc * x - x.powi(2))
    }
}

/// The mean-camber-line slope (`dyc/dx`) at `x/c`, piecewise about
/// `camber_loc`.
fn camber_slope(x: f64, max_camber: f64, camber_loc: f64) -> f64 {
    if x <= camber_loc {
        2.0 * max_camber / camber_loc.powi(2) * (camber_loc - x)
    } else {
        2.0 * max_camber / (1.0 - camber_loc).powi(2) * (camber_loc - x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repanel_produces_two_n_minus_one_points() {
        let airfoil = Airfoil::from_name("naca2412").expect("naca2412 parses");
        for n_points_per_side in [10, 50, 120] {
            let repaneled = airfoil
                .repanel(n_points_per_side)
                .expect("naca2412 has no duplicate points");
            assert_eq!(
                repaneled.coordinates.len(),
                n_points_per_side * 2 - 1,
                "n_points_per_side={n_points_per_side}"
            );
        }
    }

    #[test]
    fn le_index_picks_the_actual_minimum_x_point() {
        let airfoil = Airfoil::from_coordinates(
            "probe",
            vec![
                (1.0, 0.1),
                (0.5, 0.05),
                (-0.2, 0.0),
                (0.5, -0.05),
                (1.0, -0.1),
            ],
        );
        assert_eq!(airfoil.le_index(), 2);
    }

    #[test]
    fn le_index_breaks_ties_by_first_occurrence() {
        // Matches `np.argmin`, which returns the first index attaining the
        // minimum rather than the last.
        let airfoil = Airfoil::from_coordinates(
            "probe",
            vec![(1.0, 0.0), (0.0, 0.1), (0.0, -0.1), (1.0, 0.0)],
        );
        assert_eq!(airfoil.le_index(), 1);
    }

    #[test]
    fn upper_and_lower_coordinates_both_include_the_leading_edge_point() {
        let airfoil = Airfoil::from_name("naca2412").expect("naca2412 parses");
        let le_index = airfoil.le_index();
        let le_point = airfoil.coordinates[le_index];

        assert_eq!(
            *airfoil
                .upper_coordinates()
                .last()
                .expect("non-empty upper surface"),
            le_point
        );
        assert_eq!(
            *airfoil
                .lower_coordinates()
                .first()
                .expect("non-empty lower surface"),
            le_point
        );
    }

    #[test]
    fn naca_name_parsing_rejects_a_non_naca_name() {
        assert!(naca_coordinates("clarky", 50).is_none());
        assert!(Airfoil::from_name("clarky").is_none());
    }

    #[test]
    fn naca_name_parsing_rejects_a_non_four_digit_number() {
        // 5-digit NACA families raise `NotImplementedError` upstream; this
        // module has nothing else to try, so both fold into `None`.
        assert!(naca_coordinates("naca23012", 50).is_none());
        assert!(naca_coordinates("naca12", 50).is_none());
        assert!(naca_coordinates("naca", 50).is_none());
        assert!(naca_coordinates("nacaXXXX", 50).is_none());
    }

    #[test]
    fn naca_name_parsing_is_case_and_whitespace_insensitive() {
        let lower = naca_coordinates("naca0012", 20).expect("lowercase parses");
        let upper = naca_coordinates("NACA0012", 20).expect("uppercase parses");
        let padded = naca_coordinates("  naca0012  ", 20).expect("padded parses");
        assert_eq!(lower, upper);
        assert_eq!(lower, padded);
    }

    #[test]
    fn a_symmetric_naca_section_has_positive_thickness_away_from_the_edges() {
        let coordinates = naca_coordinates("naca0012", 50).expect("naca0012 parses");
        let airfoil = Airfoil::from_coordinates("naca0012", coordinates);
        let thickness = airfoil.local_thickness(&[0.25, 0.5, 0.75]);
        for value in thickness {
            assert!(value > 0.0);
        }
    }

    #[test]
    fn max_thickness_of_naca_xx12_is_close_to_twelve_percent() {
        let coordinates = naca_coordinates("naca0012", 200).expect("naca0012 parses");
        let airfoil = Airfoil::from_coordinates("naca0012", coordinates);
        let sample = linspace(0.0, 1.0, 101);
        let max_thickness = airfoil.max_thickness(&sample);
        // The 4-digit NACA formula's stated thickness is approximate (the
        // polynomial does not hit exactly `thickness` at its peak); this
        // checks the port lands in the same neighborhood upstream does,
        // leaving the fixture to pin the exact value.
        assert!(
            (max_thickness - 0.12).abs() < 0.005,
            "max_thickness={max_thickness}"
        );
    }

    #[test]
    fn local_thickness_clamps_outside_the_data_range_like_numpy_interp() {
        // A diamond-shaped probe with a single leading-edge point at x=0 and
        // a closed trailing edge at x=1, in standard airfoil order.
        let airfoil = Airfoil::from_coordinates(
            "diamond",
            vec![(1.0, 0.0), (0.5, 0.1), (0.0, 0.0), (0.5, -0.1), (1.0, 0.0)],
        );
        let thickness = airfoil.local_thickness(&[-1.0, 0.0, 0.5, 1.0, 2.0]);
        assert_eq!(
            thickness[0], thickness[1],
            "a station below the data range clamps to x=0's value"
        );
        assert_eq!(
            thickness[3], thickness[4],
            "a station above the data range clamps to x=1's value"
        );
        assert!((thickness[2] - 0.2).abs() < 1e-12);
    }

    #[test]
    fn blend_with_another_airfoil_at_zero_reproduces_self_and_at_one_reproduces_the_other() {
        let a = Airfoil::from_name("naca0012").expect("naca0012 parses");
        let b = Airfoil::from_name("naca2412").expect("naca2412 parses");

        let at_zero = a
            .blend_with_another_airfoil(&b, 0.0, 20)
            .expect("both sections repanel cleanly");
        assert_eq!(at_zero.name, "100% naca0012, 0% naca2412");
        let a_repaneled = a.repanel(20).expect("naca0012 repanels cleanly");
        assert_eq!(at_zero.coordinates, a_repaneled.coordinates);

        let at_one = a
            .blend_with_another_airfoil(&b, 1.0, 20)
            .expect("both sections repanel cleanly");
        assert_eq!(at_one.name, "0% naca0012, 100% naca2412");
        let b_repaneled = b.repanel(20).expect("naca2412 repanels cleanly");
        assert_eq!(at_one.coordinates, b_repaneled.coordinates);
    }
}
