// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from native aerodynamic model/geometry/airfoil/airfoil_families.py
// (get_kulfan_parameters) and native aerodynamic model/geometry/airfoil/kulfan_airfoil.py
// (KulfanAirfoil.upper_coordinates / lower_coordinates / to_airfoil).
// Upstream: native aerodynamic model 4.2.8, MIT.
// Reference: alas @ rust-port-baseline.

//! The Kulfan (CST with LEM) airfoil parameterization: eighteen numbers that
//! stand for a whole airfoil, and the least-squares fit that finds them.
//!
//! An airfoil arrives here as a few hundred vertices. NeuralFoil will not take
//! vertices; it is trained on Kulfan weights, and nothing else is a valid
//! input to it, so every path from a shape to a polar runs through this
//! module first. Without it the airfoil surrogate has no front door, and the
//! airfoil screening sweep (`alas/analysis/airfoil_screening.py`) has nothing
//! to score.
//!
//! # What the parameterization is
//!
//! A surface is a *class* function times a *shape* function. The class
//! function `x^N1 * (1 - x)^N2` supplies the two things every airfoil has and
//! no polynomial does on its own: a rounded leading edge (the square root at
//! `N1 = 0.5`) and a closing trailing edge (`N2 = 1`). The shape function is a
//! Bernstein polynomial whose coefficients are the weights, one vector per
//! surface. Two corrections ride on top: an open trailing edge, added as a
//! wedge linear in `x`, and Kulfan's leading-edge modification, a single mode
//! `x * (1 - x)^(n + 0.5)` that buys back the leading-edge camber the class
//! function's fixed exponent cannot express.
//!
//! References are on `get_kulfan_coordinates` upstream; the load-bearing two
//! are Kulfan's 2008 AIAA *Universal Parametric Geometry Representation
//! Method* for the class/shape transformation and his 2020 note for the
//! leading-edge modification.
//!
//! # Why the fit is linear, and what that buys
//!
//! Every unknown enters `y` linearly, so the fit is not an optimization
//! problem at all; it is one overdetermined linear system, one row per
//! vertex, eighteen columns for the eight-per-side default. Upstream offers
//! both readings: a `method="opti"` branch that hands the same objective to an
//! NLP solver, and the `method="least_squares"` default that writes the matrix
//! down. Only the default is reached (`Airfoil.to_kulfan_airfoil` does not pass
//! `method`), and only the default is translated: the `opti` branch would
//! need an interior-point solver to reproduce an answer this one gets in
//! closed form.
//!
//! [`alas_math::lstsq::least_squares`] is that solve. Its own doc records why
//! Householder QR rather than the normal equations, and why LAPACK's
//! rank-truncation branch is not reproduced: these matrices are conditioned
//! around 1.1e3, six orders clear of the cutoff, and full rank on every
//! section in this program's inputs.
//!
//! # Scope
//!
//! Both call sites that reach NeuralFoil: `airfoil_screening.py:251` through
//! `Airfoil.get_aero_from_neuralfoil`, and `visualization.py:2013` through
//! `neuralfoil.get_aero_from_coordinates`: normalize the airfoil first and
//! then fit with `normalize_coordinates=False`, so [`KulfanAirfoil::fit`]
//! takes coordinates that are already normalized and does not renormalize.
//! That is not only a scope decision but a correctness one: the class function
//! evaluates `x^0.5`, and several sections in this program's own inputs carry
//! vertices at slightly negative `x` before normalization (`naca4412` reaches
//! -2.98e-4), where that is NaN. Upstream has the same hole and the same
//! reason it never falls in.
//!
//! Left untranslated for the same "not reached" reason: the `opti` branch, the
//! standalone `get_kulfan_coordinates` (only native aerodynamic model's own airfoil
//! optimizer calls it; the reached forward map is [`KulfanAirfoil::to_airfoil`],
//! which goes through the two surface samplers), `KulfanAirfoil`'s
//! construct-from-a-name fallback, and `draw`. `get_aero_from_neuralfoil` is
//! the `alas-aero::neuralfoil` row, not this one.
//!
//! # An upstream quirk, reproduced
//!
//! `get_kulfan_parameters` takes `use_leading_edge_modification` and the
//! least-squares branch never reads it: the LEM column is in the matrix
//! whether or not it was asked for. Only the `opti` branch honours the flag.
//! Reproduced faithfully: [`KulfanAirfoil::fit`] has no such parameter,
//! since offering one that changed nothing would be worse than not offering
//! it, and recorded as a `deviation-candidate` in `docs/PORTING.md`.

use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::spacing::cosspace;
use alas_math::lstsq::{least_squares, LeastSquaresError};

/// Why a set of coordinates could not be reduced to Kulfan weights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum KulfanError {
    /// Fewer vertices than unknowns, so the fit is not overdetermined. Eight
    /// weights per side needs at least eighteen vertices; no airfoil anything
    /// here reads has fewer than thirty-five.
    #[error("{vertices} vertices cannot determine {unknowns} Kulfan parameters")]
    TooFewVertices {
        /// Vertices supplied.
        vertices: usize,
        /// Unknowns the fit has to resolve.
        unknowns: usize,
    },
    /// No weights per side were asked for, so there is no shape function.
    #[error("a Kulfan fit needs at least one weight per side")]
    NoWeights,
    /// The fit itself could not be solved. See [`LeastSquaresError`].
    #[error("the Kulfan least-squares fit failed: {0}")]
    Fit(#[from] LeastSquaresError),
}

/// An airfoil held as Kulfan (CST) parameters rather than as vertices.
///
/// The two weight vectors are the Bernstein coefficients of each surface's
/// shape function; `leading_edge_weight` scales the single leading-edge
/// modification mode, and `te_thickness` is the trailing-edge gap in `y/c`.
/// `n1` and `n2` are the class function's exponents: 0.5 and 1.0 for a
/// conventional airfoil, which is the only combination anything here uses and
/// the only one NeuralFoil is trained on.
#[derive(Debug, Clone, PartialEq)]
pub struct KulfanAirfoil {
    /// Bernstein coefficients of the lower surface, leading edge first.
    pub lower_weights: Vec<f64>,
    /// Bernstein coefficients of the upper surface, leading edge first.
    pub upper_weights: Vec<f64>,
    /// Strength of Kulfan's leading-edge modification mode.
    pub leading_edge_weight: f64,
    /// Trailing-edge gap, in `y/c`. Never negative: see [`KulfanAirfoil::fit`].
    pub te_thickness: f64,
    /// Class-function exponent at the leading edge.
    pub n1: f64,
    /// Class-function exponent at the trailing edge.
    pub n2: f64,
}

impl KulfanAirfoil {
    /// Fit Kulfan parameters to already-normalized coordinates, as
    /// `get_kulfan_parameters(..., method="least_squares")` does.
    ///
    /// `coordinates` run in Selig order (upper-surface trailing edge, around
    /// the leading edge, lower-surface trailing edge) with the leading edge
    /// identified as the first vertex of least `x`, exactly as upstream's
    /// `np.argmin` identifies it.
    ///
    /// The trailing-edge thickness is a fitted unknown like any other, and
    /// nothing in the linear system stops it going negative on a section whose
    /// trailing edge is closed. Upstream detects that after the fact and
    /// re-solves without the thickness column, pinning it to zero; so does
    /// this. The second solve is a different problem on a different matrix,
    /// not a clamp on the first one's answer, every other weight moves too.
    ///
    /// # Errors
    ///
    /// [`KulfanError`] when there are too few vertices to determine the
    /// parameters, when no weights were asked for, or when the fit's matrix
    /// turns out to be rank-deficient.
    pub fn fit(
        coordinates: &[(f64, f64)],
        n_weights_per_side: usize,
        n1: f64,
        n2: f64,
    ) -> Result<Self, KulfanError> {
        if n_weights_per_side == 0 {
            return Err(KulfanError::NoWeights);
        }
        let unknowns = 2 * n_weights_per_side + 2;
        if coordinates.len() < unknowns {
            return Err(KulfanError::TooFewVertices {
                vertices: coordinates.len(),
                unknowns,
            });
        }

        let matrix = fit_matrix(coordinates, n_weights_per_side, n1, n2);
        let targets: Vec<f64> = coordinates.iter().map(|&(_, y)| y).collect();

        let solved = least_squares(&matrix, &targets)?;
        let te_thickness = solved[unknowns - 1];
        if te_thickness >= 0.0 {
            return Ok(Self {
                lower_weights: solved[..n_weights_per_side].to_vec(),
                upper_weights: solved[n_weights_per_side..2 * n_weights_per_side].to_vec(),
                leading_edge_weight: solved[unknowns - 2],
                te_thickness,
                n1,
                n2,
            });
        }

        let pinned: Vec<Vec<f64>> = matrix
            .into_iter()
            .map(|mut row| {
                row.pop();
                row
            })
            .collect();
        let solved = least_squares(&pinned, &targets)?;
        Ok(Self {
            lower_weights: solved[..n_weights_per_side].to_vec(),
            upper_weights: solved[n_weights_per_side..2 * n_weights_per_side].to_vec(),
            // The dropped column was the last one, so the leading-edge mode is
            // now the final unknown: the same absolute index it occupied
            // before, which is why upstream's `x[-2]` and `x[-1]` name it in
            // the two branches.
            leading_edge_weight: solved[unknowns - 2],
            te_thickness: 0.0,
            n1,
            n2,
        })
    }

    /// The upper surface sampled at `x_over_c`, as `(x, y)` pairs.
    pub fn upper_coordinates(&self, x_over_c: &[f64]) -> Vec<(f64, f64)> {
        self.surface(x_over_c, &self.upper_weights, 1.0)
    }

    /// The lower surface sampled at `x_over_c`, as `(x, y)` pairs.
    pub fn lower_coordinates(&self, x_over_c: &[f64]) -> Vec<(f64, f64)> {
        self.surface(x_over_c, &self.lower_weights, -1.0)
    }

    /// The section's thickness at each `x/c` station: `local_thickness`.
    ///
    /// `KulfanAirfoil` inherits `Airfoil.local_thickness`'s name and overrides
    /// what is underneath it: this samples the two class-times-shape surfaces
    /// analytically, where `alas-geom::aircraft::airfoil`'s interpolates a vertex
    /// list. The two agree to the accuracy of the fit and are not the same
    /// function, and it is this one that
    /// `KulfanAirfoil.get_aero_from_neuralfoil` reads for the `t/c` its
    /// wave-drag schedule is built on.
    pub fn local_thickness(&self, x_over_c: &[f64]) -> Vec<f64> {
        let upper = self.upper_coordinates(x_over_c);
        let lower = self.lower_coordinates(x_over_c);
        upper
            .iter()
            .zip(&lower)
            .map(|(&(_, upper_y), &(_, lower_y))| upper_y - lower_y)
            .collect()
    }

    /// The maximum of [`KulfanAirfoil::local_thickness`] over
    /// `x_over_c_sample`: `max_thickness`.
    ///
    /// Upstream defaults the sample to `np.linspace(0, 1, 101)`; a caller
    /// that wants that grid builds it, as `alas-geom::aircraft::airfoil`'s
    /// counterpart also requires.
    pub fn max_thickness(&self, x_over_c_sample: &[f64]) -> f64 {
        self.local_thickness(x_over_c_sample)
            .into_iter()
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// Reconstruct a vertex airfoil, as `KulfanAirfoil.coordinates` is defined:
    /// cosine-spaced upper surface from the trailing edge to (but not
    /// including) the leading edge, then cosine-spaced lower surface from the
    /// leading edge back.
    ///
    /// The leading-edge vertex therefore comes from the lower surface's
    /// sampler and appears once, not twice.
    pub fn to_airfoil(&self, name: impl Into<String>, n_coordinates_per_side: usize) -> Airfoil {
        let mut upper_stations = cosspace(1.0, 0.0, n_coordinates_per_side);
        upper_stations.pop();
        let mut coordinates = self.upper_coordinates(&upper_stations);
        coordinates.extend(self.lower_coordinates(&cosspace(0.0, 1.0, n_coordinates_per_side)));
        Airfoil::from_coordinates(name, coordinates)
    }

    /// One surface: class function times shape function, plus half the
    /// trailing-edge wedge in `side`'s direction, plus the leading-edge mode.
    fn surface(&self, x_over_c: &[f64], weights: &[f64], side: f64) -> Vec<(f64, f64)> {
        let coefficients = bernstein_coefficients(weights.len());
        // Unlike the fit's own leading-edge column, this one does not clamp
        // `1 - x` at zero before raising it to a fractional power. Upstream
        // guards there and not here; reproduced as found.
        let lem_exponent = weights.len() as f64 + 0.5;

        x_over_c
            .iter()
            .map(|&x| {
                let class = x.powf(self.n1) * (1.0 - x).powf(self.n2);
                let shape: f64 = weights
                    .iter()
                    .zip(&coefficients)
                    .enumerate()
                    .map(|(index, (&weight, &coefficient))| {
                        weight * bernstein(x, index, weights.len(), coefficient)
                    })
                    .sum();
                let y = class * shape
                    + side * x * self.te_thickness / 2.0
                    + self.leading_edge_weight * x * (1.0 - x).powf(lem_exponent);
                (x, y)
            })
            .collect()
    }
}

/// The Bernstein basis coefficients `comb(n - 1, k)`, built by Pascal's rule.
///
/// `scipy.special.comb` returns these as exact `float64` integers at every
/// order this module reaches (checked against orders 3 and 7), so building
/// them by addition rather than by a gamma-function ratio reproduces the
/// values bit for bit and cannot drift on a different SciPy.
fn bernstein_coefficients(count: usize) -> Vec<f64> {
    let mut row = vec![1.0];
    for _ in 1..count {
        let mut next = vec![1.0; row.len() + 1];
        for index in 1..row.len() {
            next[index] = row[index - 1] + row[index];
        }
        row = next;
    }
    row
}

/// One Bernstein basis function of order `count - 1` at `x`.
fn bernstein(x: f64, index: usize, count: usize, coefficient: f64) -> f64 {
    // `powf` rather than `powi`: NumPy promotes the integer exponent array to
    // float64 and calls the platform `pow`, and repeated squaring does not
    // always land on the same last bit.
    coefficient * x.powf(index as f64) * (1.0 - x).powf((count - 1 - index) as f64)
}

/// The fit's design matrix: one row per vertex, `2 * n + 2` columns holding
/// the lower weights, the upper weights, the leading-edge mode and the
/// trailing-edge wedge in that order.
///
/// A vertex contributes to whichever surface it lies on and zero to the other,
/// which is what lets one system fit both surfaces at once.
fn fit_matrix(
    coordinates: &[(f64, f64)],
    n_weights_per_side: usize,
    n1: f64,
    n2: f64,
) -> Vec<Vec<f64>> {
    let leading_edge = leading_edge_index(coordinates);
    let coefficients = bernstein_coefficients(n_weights_per_side);
    let lem_exponent = n_weights_per_side as f64 + 0.5;

    coordinates
        .iter()
        .enumerate()
        .map(|(vertex, &(x, _))| {
            let is_upper = vertex <= leading_edge;
            let class = x.powf(n1) * (1.0 - x).powf(n2);

            let mut row = Vec::with_capacity(2 * n_weights_per_side + 2);
            for surface_is_upper in [false, true] {
                for (index, &coefficient) in coefficients.iter().enumerate() {
                    row.push(if is_upper == surface_is_upper {
                        class * bernstein(x, index, n_weights_per_side, coefficient)
                    } else {
                        0.0
                    });
                }
            }
            // The clamp is upstream's and is only on this column; the surface
            // samplers raise the same base to the same power without it.
            row.push(x * (1.0 - x).max(0.0).powf(lem_exponent));
            row.push(if is_upper { x / 2.0 } else { -x / 2.0 });
            row
        })
        .collect()
}

/// The first vertex of least `x`, which is what `np.argmin` returns and which
/// decides where the upper surface stops and the lower one starts.
fn leading_edge_index(coordinates: &[(f64, f64)]) -> usize {
    let mut best = 0;
    for (index, &(x, _)) in coordinates.iter().enumerate() {
        if x < coordinates[best].0 {
            best = index;
        }
    }
    best
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn naca0012_like() -> Vec<(f64, f64)> {
        // A symmetric analytic section, dense enough to overdetermine the fit.
        let mut coordinates = Vec::new();
        let n = 120;
        for index in 0..=n {
            let x = 1.0 - f64::from(index) / f64::from(n);
            coordinates.push((x, half_thickness(x)));
        }
        for index in 1..=n {
            let x = f64::from(index) / f64::from(n);
            coordinates.push((x, -half_thickness(x)));
        }
        coordinates
    }

    fn half_thickness(x: f64) -> f64 {
        0.6 * (0.2969 * x.sqrt() - 0.126 * x - 0.3516 * x * x + 0.2843 * x.powi(3)
            - 0.1015 * x.powi(4))
    }

    #[test]
    fn a_symmetric_section_fits_to_mirrored_weights_and_no_leading_edge_mode() {
        let fitted = KulfanAirfoil::fit(&naca0012_like(), 8, 0.5, 1.0).expect("a full-rank fit");
        for (upper, lower) in fitted.upper_weights.iter().zip(&fitted.lower_weights) {
            assert!(
                (upper + lower).abs() < 1e-12,
                "upper {upper:e} does not mirror lower {lower:e}"
            );
        }
        assert!(fitted.leading_edge_weight.abs() < 1e-12);
    }

    #[test]
    fn the_fit_reproduces_the_section_it_was_fitted_to() {
        // Parity checks the weights; this checks that they mean what they say,
        // by putting them back through the forward map at the same abscissae.
        let coordinates = naca0012_like();
        let fitted = KulfanAirfoil::fit(&coordinates, 8, 0.5, 1.0).expect("a full-rank fit");
        let stations: Vec<f64> = (1..20).map(|i| f64::from(i) / 20.0).collect();
        for (&x, &(_, y)) in stations.iter().zip(&fitted.upper_coordinates(&stations)) {
            assert!(
                (y - half_thickness(x)).abs() < 2e-4,
                "x = {x}: fitted {y:e}, section {:e}",
                half_thickness(x)
            );
        }
    }

    #[test]
    fn a_crossed_trailing_edge_pins_the_thickness_to_zero_rather_than_going_negative() {
        // The analytic section above ends open. Closing it past zero (the
        // surfaces cross near the trailing edge) is what drives the
        // unconstrained fit's last unknown negative, which is the only way to
        // reach the re-solve branch. Real sections reach it too, more gently:
        // `e63` and `sd7037` in this row's fixture both do.
        let crossed: Vec<(f64, f64)> = naca0012_like()
            .into_iter()
            .map(|(x, y)| (x, y - y.signum() * 3.0 * half_thickness(1.0) * x))
            .collect();
        let fitted = KulfanAirfoil::fit(&crossed, 8, 0.5, 1.0).expect("a full-rank fit");
        assert_eq!(fitted.te_thickness, 0.0);
    }

    #[test]
    fn the_class_function_vanishes_at_both_endpoints() {
        // Every airfoil closes at the trailing edge and meets the axis at the
        // leading edge, whatever its weights are; only the trailing-edge wedge
        // and the leading-edge mode survive there.
        let airfoil = KulfanAirfoil {
            lower_weights: vec![-0.2; 8],
            upper_weights: vec![0.2; 8],
            leading_edge_weight: 0.3,
            te_thickness: 0.01,
            n1: 0.5,
            n2: 1.0,
        };
        let upper = airfoil.upper_coordinates(&[0.0, 1.0]);
        assert_eq!(upper[0].1, 0.0);
        assert!((upper[1].1 - 0.005).abs() < 1e-15);
        let lower = airfoil.lower_coordinates(&[0.0, 1.0]);
        assert_eq!(lower[0].1, 0.0);
        assert!((lower[1].1 + 0.005).abs() < 1e-15);
    }

    #[test]
    fn the_reconstruction_visits_the_leading_edge_exactly_once() {
        let airfoil = KulfanAirfoil {
            lower_weights: vec![-0.2; 8],
            upper_weights: vec![0.2; 8],
            leading_edge_weight: 0.0,
            te_thickness: 0.0,
            n1: 0.5,
            n2: 1.0,
        };
        let rebuilt = airfoil.to_airfoil("test", 50);
        assert_eq!(rebuilt.coordinates.len(), 99);
        let at_leading_edge = rebuilt
            .coordinates
            .iter()
            .filter(|&&(x, _)| x == 0.0)
            .count();
        assert_eq!(at_leading_edge, 1);
    }

    #[test]
    fn too_few_vertices_is_an_error_not_a_panic() {
        let coordinates = vec![(1.0, 0.0), (0.0, 0.0), (1.0, -0.0)];
        assert_eq!(
            KulfanAirfoil::fit(&coordinates, 8, 0.5, 1.0),
            Err(KulfanError::TooFewVertices {
                vertices: 3,
                unknowns: 18
            })
        );
    }

    #[test]
    fn asking_for_no_weights_is_an_error_not_a_panic() {
        assert_eq!(
            KulfanAirfoil::fit(&naca0012_like(), 0, 0.5, 1.0),
            Err(KulfanError::NoWeights)
        );
    }

    #[test]
    fn the_leading_edge_is_the_first_vertex_of_least_x() {
        // A blunt section can carry two vertices at the same minimum; upstream
        // takes the first, which puts the tie on the upper surface.
        let coordinates = [(1.0, 0.01), (0.0, 0.002), (0.0, -0.002), (1.0, -0.01)];
        assert_eq!(leading_edge_index(&coordinates), 1);
    }

    #[test]
    fn the_bernstein_coefficients_are_the_binomial_row() {
        assert_eq!(bernstein_coefficients(1), vec![1.0]);
        assert_eq!(bernstein_coefficients(4), vec![1.0, 3.0, 3.0, 1.0]);
        assert_eq!(
            bernstein_coefficients(8),
            vec![1.0, 7.0, 21.0, 35.0, 35.0, 21.0, 7.0, 1.0]
        );
    }
}
