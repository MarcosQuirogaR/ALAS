// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The optional cross-section residual (schema 1, additive minor extension).
//!
//! The warped superellipse cannot draw a section whose upper arc is narrower
//! than an ellipse through the same crown and shoulder (the Boeing 747 upper
//! deck). The residual scales the half-width of the UPPER arc only, at fixed
//! height:
//!
//! ```text
//! y = a * eta * (1 + R(zeta; b))   for zeta in [0, 1] (above the widest point),
//! y = a * eta                      for zeta < 0,
//! R(t; b) = sum_{j=0..3} b_j B_{j+3,6}(t),     B_{i,6}(t) = C(6,i) t^i (1-t)^(6-i)
//! ```
//!
//! `R` is a degree-6 Bernstein polynomial whose first three coefficients are
//! pinned to zero, so `R = R' = R'' = 0` at the widest point: the join with the
//! untouched lower arc is C2 in `zeta`, hence G2. Because `z` is not touched,
//! the crown (`zeta = 1, y = 0`), the keel and the side silhouette are exactly
//! those of the base section; `R` leaves the widest point at `y = a` as long as
//! `h(zeta) (1 + R) <= 1` (checked by the evaluator). The four coefficients
//! `b_j(u)` are clamped B-splines of the body degree in `u = x / L`, zero
//! outside the region where the section needs the correction.

use serde::{Deserialize, Serialize};

use super::spec::SplineSpec;
use super::validate::{build, LoftSpecError};

/// At most this many coefficient curves per body.
pub const MAX_RESIDUAL_TERMS: usize = 4;
/// Bound on `abs(b_j)`.
pub const RESIDUAL_COEFF_LIMIT: f64 = 5.0;
/// `1 + R` must stay above this (the upper arc never collapses).
pub const MIN_WIDTH_FACTOR: f64 = 0.05;

/// Names used in error messages.
const NAMES: [&str; MAX_RESIDUAL_TERMS] =
    ["residual b0", "residual b1", "residual b2", "residual b3"];

/// The residual coefficient curves `b_0 .. b_{m-1}` (`m <= 4`; missing ones
/// are zero).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResidualSpec {
    /// One clamped B-spline of `u = x / reference_length_m` per coefficient.
    pub terms: Vec<SplineSpec>,
}

/// The four basis functions `B_{j+3,6}(t)`, `j = 0..3`, at `t` in `[0, 1]`.
#[must_use]
pub fn residual_basis(t: f64) -> [f64; MAX_RESIDUAL_TERMS] {
    let s = 1.0 - t;
    [
        20.0 * t.powi(3) * s.powi(3),
        15.0 * t.powi(4) * s * s,
        6.0 * t.powi(5) * s,
        t.powi(6),
    ]
}

/// The derivatives of [`residual_basis`] with respect to `t`.
#[must_use]
pub fn residual_basis_dt(t: f64) -> [f64; MAX_RESIDUAL_TERMS] {
    let s = 1.0 - t;
    [
        60.0 * t * t * s * s * (1.0 - 2.0 * t),
        30.0 * t.powi(3) * s * (2.0 - 3.0 * t),
        6.0 * t.powi(4) * (5.0 - 6.0 * t),
        6.0 * t.powi(5),
    ]
}

/// `R(t; b)`.
#[must_use]
pub fn residual_value(b: &[f64; MAX_RESIDUAL_TERMS], t: f64) -> f64 {
    let w = residual_basis(t);
    b.iter().zip(w).map(|(b, w)| b * w).sum()
}

/// `dR/dt`.
#[must_use]
pub fn residual_slope(b: &[f64; MAX_RESIDUAL_TERMS], t: f64) -> f64 {
    let w = residual_basis_dt(t);
    b.iter().zip(w).map(|(b, w)| b * w).sum()
}

impl ResidualSpec {
    /// Check the term count, every curve (clamped B-spline of `degree` on
    /// `[0, 1]`, finite, `abs(b) <= 5`) and that `1 + R >= 0.05` everywhere.
    ///
    /// # Errors
    ///
    /// The first problem found, as a [`LoftSpecError`].
    pub fn validate(&self, degree: u8) -> Result<(), LoftSpecError> {
        if self.terms.is_empty() || self.terms.len() > MAX_RESIDUAL_TERMS {
            return Err(LoftSpecError::Residual {
                reason: format!(
                    "{} terms; between 1 and {MAX_RESIDUAL_TERMS} are allowed",
                    self.terms.len()
                ),
            });
        }
        let mut splines = Vec::new();
        for (name, term) in NAMES.iter().zip(&self.terms) {
            splines.push(build(name, term, degree)?);
        }
        let mut points: Vec<f64> = splines.iter().flat_map(|s| s.breakpoints()).collect();
        points.extend((0..=400).map(|i| f64::from(i) / 400.0));
        for u in points {
            let mut b = [0.0; MAX_RESIDUAL_TERMS];
            for (slot, s) in b.iter_mut().zip(&splines) {
                *slot = s.eval(u);
            }
            if b.iter()
                .any(|v| !v.is_finite() || v.abs() > RESIDUAL_COEFF_LIMIT)
            {
                return Err(LoftSpecError::Residual {
                    reason: format!(
                        "coefficients {b:?} at u = {u} are not finite or exceed {RESIDUAL_COEFF_LIMIT}"
                    ),
                });
            }
            for i in 0..=100 {
                let t = f64::from(i) / 100.0;
                if 1.0 + residual_value(&b, t) < MIN_WIDTH_FACTOR {
                    return Err(LoftSpecError::Residual {
                        reason: format!(
                            "1 + R = {} below {MIN_WIDTH_FACTOR} at u = {u}, zeta = {t}",
                            1.0 + residual_value(&b, t)
                        ),
                    });
                }
            }
        }
        Ok(())
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basis_is_pinned_at_the_widest_point_and_matches_its_derivative() {
        let w0 = residual_basis(0.0);
        assert!(w0.iter().all(|v| *v == 0.0));
        for t in [0.2, 0.5, 0.8, 0.97] {
            let h = 1e-6;
            let (p, m) = (residual_basis(t + h), residual_basis(t - h));
            let d = residual_basis_dt(t);
            for j in 0..4 {
                assert!(
                    ((p[j] - m[j]) / (2.0 * h) - d[j]).abs() < 1e-6,
                    "j={j} t={t}"
                );
            }
        }
        // R, R', R'' vanish at t = 0: the lowest basis function is cubic in t.
        let t = 1e-3;
        assert!(residual_basis(t)[0] < 21.0 * t.powi(3));
        // At the crown R = b3.
        assert!((residual_value(&[0.0, 0.0, 0.0, -0.3], 1.0) + 0.3).abs() < 1e-15);
    }

    #[test]
    fn terms_that_make_the_arc_collapse_are_rejected() {
        let flat = |c: f64| SplineSpec {
            knots: vec![0., 0., 0., 0., 0., 0., 1., 1., 1., 1., 1., 1.],
            coeffs: vec![c; 6],
        };
        let ok = ResidualSpec {
            terms: vec![flat(0.0), flat(0.1)],
        };
        assert!(ok.validate(5).is_ok());
        let collapse = ResidualSpec {
            terms: vec![flat(0.0), flat(0.0), flat(0.0), flat(-0.99)],
        };
        assert!(matches!(
            collapse.validate(5),
            Err(LoftSpecError::Residual { .. })
        ));
        let too_many = ResidualSpec {
            terms: vec![flat(0.0); 5],
        };
        assert!(too_many.validate(5).is_err());
        let huge = ResidualSpec {
            terms: vec![flat(9.0)],
        };
        assert!(huge.validate(5).is_err());
    }

    fn cubic_flat(c: f64) -> SplineSpec {
        SplineSpec {
            knots: vec![0., 0., 0., 0., 1., 1., 1., 1.],
            coeffs: vec![c; 4],
        }
    }

    #[test]
    fn the_residual_is_an_additive_optional_field_of_schema_v1() {
        use crate::geometry::loft::parse_loft_spec;
        let sphere = include_str!("../../../tests/fixtures/loft_test_sphere.json");
        let plain = parse_loft_spec(sphere).unwrap();
        assert!(plain.residual.is_none());
        // Without a residual the serialised file has no residual key at all.
        let json = serde_json::to_string(&plain).unwrap();
        assert!(!json.contains("residual"));
        // With one it round-trips and validates.
        let mut with = plain.clone();
        with.residual = Some(ResidualSpec {
            terms: vec![cubic_flat(0.0), cubic_flat(-0.05)],
        });
        let json = serde_json::to_string(&with).unwrap();
        assert!(json.contains("\"residual\""));
        let back = parse_loft_spec(&json).unwrap();
        assert_eq!(back, with);
        // Unknown fields inside the block are refused, too many terms fail
        // validation.
        let bad = json.replace("\"terms\"", "\"extra\":1,\"terms\"");
        assert!(parse_loft_spec(&bad).is_err());
        with.residual = Some(ResidualSpec {
            terms: vec![cubic_flat(0.0); 5],
        });
        assert!(matches!(
            with.validate(),
            Err(LoftSpecError::Residual { .. })
        ));
    }
}
