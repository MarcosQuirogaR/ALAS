// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Structural and range validation of a [`LoftSpec`].

use alas_math::clamped_bspline::{ClampedBSpline, ClampedBSplineError};

use super::spec::{Closure, LoftSpec, SplineSpec, LOFT_SCHEMA_VERSION};

/// Smallest exponent the section family accepts. Below 2 the curvature of the
/// section diverges at the crown, keel and widest point (as `delta^(n-2)`), so
/// the section is not G2 around its circumference.
pub const MIN_EXPONENT: f64 = 2.0;
/// Largest exponent the section family accepts.
pub const MAX_EXPONENT: f64 = 6.0;
/// The shoulder offset must stay strictly inside this bound (warp monotone).
pub const SHOULDER_LIMIT: f64 = 0.5;
/// Largest number of coefficients of one curve.
pub const MAX_COEFFS_PER_CURVE: usize = 200;

/// Tolerance on the sign of the squared extents, m^2.
const NEGATIVE_TOLERANCE: f64 = 1e-9;
/// Samples used for the range checks, besides the breakpoints.
const RANGE_SAMPLES: usize = 400;

/// What is wrong with one curve.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SplineProblem {
    /// The spline itself is not a valid clamped B-spline of the degree.
    #[error("{0}")]
    Invalid(#[from] ClampedBSplineError),
    /// Too many coefficients.
    #[error("{0} coefficients exceed the limit of {MAX_COEFFS_PER_CURVE}")]
    TooManyCoeffs(usize),
    /// The domain is not `[0, 1]`.
    #[error("knots must span [0, 1], found [{first}, {last}]")]
    Domain {
        /// First knot.
        first: f64,
        /// Last knot.
        last: f64,
    },
    /// An interior knot is repeated so often that curvature would jump.
    #[error("interior knot {knot} has multiplicity {multiplicity}; at most degree - 2 keeps the body G2")]
    Multiplicity {
        /// Knot value.
        knot: f64,
        /// Its multiplicity.
        multiplicity: usize,
    },
    /// A value outside the allowed range at parameter `u`.
    #[error("value {value} outside {range} at u = {u}")]
    Range {
        /// Parameter.
        u: f64,
        /// Offending value.
        value: f64,
        /// Description of the allowed range.
        range: &'static str,
    },
}

/// Why a [`LoftSpec`] was rejected.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LoftSpecError {
    /// The file is not valid JSON for the schema.
    #[error("loft data does not match schema v1: {0}")]
    Parse(String),
    /// No embedded loft has this id.
    #[error("no embedded loft named {0:?}")]
    UnknownEmbedded(String),
    /// Unsupported schema version.
    #[error("unsupported loft schema {found}; this build reads {LOFT_SCHEMA_VERSION}")]
    Schema {
        /// Version found.
        found: u32,
    },
    /// Degree not 3 or 5.
    #[error("spline degree {0} is not 3 or 5")]
    Degree(u8),
    /// Reference length not finite and positive.
    #[error("reference length {0} m must be finite and positive")]
    ReferenceLength(f64),
    /// A curve is invalid.
    #[error("curve {curve}: {problem}")]
    Curve {
        /// Curve name (`A`, `E`, `zc`, `n`, `k`).
        curve: &'static str,
        /// What is wrong.
        problem: SplineProblem,
    },
    /// A closure is inconsistent with its end values.
    #[error("{end} closure: {reason}")]
    Closure {
        /// `front` or `rear`.
        end: &'static str,
        /// What is wrong.
        reason: String,
    },
    /// The cabin stations are not ordered inside the body.
    #[error("cabin stations {start} .. {end} m do not lie inside 0 .. {length} m")]
    Cabin {
        /// Forward station.
        start: f64,
        /// Aft station.
        end: f64,
        /// Reference length.
        length: f64,
    },
    /// The optional cross-section residual is not usable.
    #[error("residual: {reason}")]
    Residual {
        /// What is wrong.
        reason: String,
    },
    /// The fairness budget is not usable.
    #[error("fairness budget tv_ratio_max {0} must be finite and at least 1")]
    Fairness(f64),
}

pub(super) fn build(
    curve: &'static str,
    spec: &SplineSpec,
    degree: u8,
) -> Result<ClampedBSpline, LoftSpecError> {
    let wrap = |problem: SplineProblem| LoftSpecError::Curve { curve, problem };
    if spec.coeffs.len() > MAX_COEFFS_PER_CURVE {
        return Err(wrap(SplineProblem::TooManyCoeffs(spec.coeffs.len())));
    }
    let spline = ClampedBSpline::new(spec.knots.clone(), spec.coeffs.clone(), usize::from(degree))
        .map_err(|e| wrap(SplineProblem::Invalid(e)))?;
    let (first, last) = spline.domain();
    if first != 0.0 || last != 1.0 {
        return Err(wrap(SplineProblem::Domain { first, last }));
    }
    let knots = spline.knots();
    let n = spline.num_coeffs();
    let degree = usize::from(degree);
    let mut run = 1usize;
    for i in (degree + 1)..n {
        run = if knots[i] == knots[i - 1] { run + 1 } else { 1 };
        if run > degree - 2 {
            return Err(wrap(SplineProblem::Multiplicity {
                knot: knots[i],
                multiplicity: run,
            }));
        }
    }
    Ok(spline)
}

fn sample_points(spline: &ClampedBSpline) -> Vec<f64> {
    let mut points = spline.breakpoints();
    points.extend((0..=RANGE_SAMPLES).map(|i| i as f64 / RANGE_SAMPLES as f64));
    points
}

fn check_range(
    curve: &'static str,
    spline: &ClampedBSpline,
    range: &'static str,
    ok: impl Fn(f64) -> bool,
) -> Result<(), LoftSpecError> {
    for u in sample_points(spline) {
        let value = spline.eval(u);
        if !value.is_finite() || !ok(value) {
            return Err(LoftSpecError::Curve {
                curve,
                problem: SplineProblem::Range { u, value, range },
            });
        }
    }
    Ok(())
}

/// The curves at one end of the body.
struct EndCurves<'a> {
    a: &'a ClampedBSpline,
    e: &'a ClampedBSpline,
    n: &'a ClampedBSpline,
    k: &'a ClampedBSpline,
    /// Parameter of the end, 0 or 1.
    u: f64,
    /// +1 at the nose, -1 at the tail: the sign of the slope into the body.
    inward: f64,
    length: f64,
}

/// Relative agreement required between `tip_radius_m` and the mean of the plan
/// and side radii `A'/2`, `E'/2`.
///
/// The cap is an elliptic paraboloid with those two radii of curvature; it is
/// G2 at the pole for any positive pair, so the two may differ (an ellipsoidal
/// nose has `a^2/c` in plan and `e^2/c` in profile) up to [`MAX_TIP_RADIUS_RATIO`].
const TIP_RADIUS_TOLERANCE: f64 = 1e-3;
/// Largest ratio of the plan and side tip radii.
pub const MAX_TIP_RADIUS_RATIO: f64 = 10.0;
/// Tolerance on `n = 2` and `k = 0` at a pole.
const POLE_TOLERANCE: f64 = 1e-9;

fn check_closure(
    end: &'static str,
    closure: Closure,
    c: &EndCurves<'_>,
) -> Result<(), LoftSpecError> {
    let (a, e) = (c.a.eval(c.u), c.e.eval(c.u));
    let reason = match closure {
        Closure::Point { tip_radius_m } => {
            // The surface is G2 at the pole only for a paraboloid cap: A and E
            // vanish linearly (dA/dx = 2 R_plan > 0, dE/dx = 2 R_side > 0), the
            // section there is a circle (n = 2) and has no shoulder (k = 0).
            let plan = c.inward * c.a.derivatives(c.u)[1] / (2.0 * c.length);
            let side = c.inward * c.e.derivatives(c.u)[1] / (2.0 * c.length);
            let mean = 0.5 * (plan + side);
            if !(tip_radius_m.is_finite() && tip_radius_m > 0.0) {
                Some(format!("tip radius {tip_radius_m} m must be positive"))
            } else if a.abs() > NEGATIVE_TOLERANCE || e.abs() > NEGATIVE_TOLERANCE {
                Some(format!(
                    "a point closure needs A = E = 0 at the end, found A = {a}, E = {e}"
                ))
            } else if (c.n.eval(c.u) - 2.0).abs() > POLE_TOLERANCE
                || c.k.eval(c.u).abs() > POLE_TOLERANCE
            {
                Some("a point closure needs n = 2 and k = 0 at the end".to_owned())
            } else if !(plan > 0.0 && side > 0.0) {
                Some(format!(
                    "a point closure needs A' and E' pointing into the body, found radii {plan} and {side} m"
                ))
            } else if (mean - tip_radius_m).abs() > TIP_RADIUS_TOLERANCE * tip_radius_m
                || plan.max(side) > MAX_TIP_RADIUS_RATIO * plan.min(side)
            {
                Some(format!(
                    "tip_radius_m {tip_radius_m} is not the mean of A'/2 = {plan} and E'/2 = {side} m, or they differ by more than a factor {MAX_TIP_RADIUS_RATIO}"
                ))
            } else {
                None
            }
        }
        Closure::Base => {
            if a <= NEGATIVE_TOLERANCE || e <= NEGATIVE_TOLERANCE {
                Some(format!(
                    "a base closure needs A > 0 and E > 0 at the end, found A = {a}, E = {e}"
                ))
            } else {
                None
            }
        }
    };
    reason.map_or(Ok(()), |reason| Err(LoftSpecError::Closure { end, reason }))
}

impl LoftSpec {
    /// Check the schema version, degree, every curve (knot vector, counts,
    /// clamping, continuity, ranges: `A, E >= 0`, `2 <= n <= 6`,
    /// `abs(k) < 0.5`, finite `zc`), the closures, the cabin stations and the
    /// fairness budget.
    ///
    /// # Errors
    ///
    /// The first problem found, as a [`LoftSpecError`].
    pub fn validate(&self) -> Result<(), LoftSpecError> {
        if self.schema != LOFT_SCHEMA_VERSION {
            return Err(LoftSpecError::Schema { found: self.schema });
        }
        if self.degree != 3 && self.degree != 5 {
            return Err(LoftSpecError::Degree(self.degree));
        }
        if !(self.reference_length_m.is_finite() && self.reference_length_m > 0.0) {
            return Err(LoftSpecError::ReferenceLength(self.reference_length_m));
        }
        let a = build("A", &self.curves.a, self.degree)?;
        let e = build("E", &self.curves.e, self.degree)?;
        let zc = build("zc", &self.curves.zc, self.degree)?;
        let n = build("n", &self.curves.n, self.degree)?;
        let k = build("k", &self.curves.k, self.degree)?;
        if let Some(residual) = &self.residual {
            residual.validate(self.degree)?;
        }
        check_range("A", &a, "A >= 0", |v| v >= -NEGATIVE_TOLERANCE)?;
        check_range("E", &e, "E >= 0", |v| v >= -NEGATIVE_TOLERANCE)?;
        check_range("zc", &zc, "finite", |_| true)?;
        check_range("n", &n, "2 <= n <= 6", |v| {
            (MIN_EXPONENT - 1e-9..=MAX_EXPONENT + 1e-9).contains(&v)
        })?;
        check_range("k", &k, "abs(k) < 0.5", |v| v.abs() < SHOULDER_LIMIT)?;
        let end = |u: f64, inward: f64| EndCurves {
            a: &a,
            e: &e,
            n: &n,
            k: &k,
            u,
            inward,
            length: self.reference_length_m,
        };
        check_closure("front", self.front, &end(0.0, 1.0))?;
        check_closure("rear", self.rear, &end(1.0, -1.0))?;
        let cabin = self.cabin;
        let length = self.reference_length_m;
        if !(cabin.x_start_m.is_finite()
            && cabin.x_end_m.is_finite()
            && cabin.x_start_m >= 0.0
            && cabin.x_start_m <= cabin.x_end_m
            && cabin.x_end_m <= length)
        {
            return Err(LoftSpecError::Cabin {
                start: cabin.x_start_m,
                end: cabin.x_end_m,
                length,
            });
        }
        let tv = self.fairness.tv_ratio_max;
        if !(tv.is_finite() && tv >= 1.0) {
            return Err(LoftSpecError::Fairness(tv));
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
    use crate::geometry::loft::parse_loft_spec;

    fn sphere() -> LoftSpec {
        parse_loft_spec(include_str!(
            "../../../tests/fixtures/loft_test_sphere.json"
        ))
        .unwrap()
    }

    fn rejected(spec: &LoftSpec) -> LoftSpecError {
        spec.validate().unwrap_err()
    }

    #[test]
    fn loft_validation_accepts_the_sphere_and_rejects_a_wrong_schema_or_degree() {
        assert!(sphere().validate().is_ok());
        let mut s = sphere();
        s.schema = 2;
        assert!(matches!(rejected(&s), LoftSpecError::Schema { found: 2 }));
        let mut s = sphere();
        s.degree = 4;
        assert!(matches!(rejected(&s), LoftSpecError::Degree(4)));
        let mut s = sphere();
        s.reference_length_m = f64::NAN;
        assert!(matches!(rejected(&s), LoftSpecError::ReferenceLength(_)));
    }

    #[test]
    fn loft_validation_checks_the_knot_vector() {
        let mut s = sphere();
        s.curves.a.knots.pop();
        assert!(matches!(
            rejected(&s),
            LoftSpecError::Curve {
                curve: "A",
                problem: SplineProblem::Invalid(_)
            }
        ));
        let mut s = sphere();
        s.curves.zc.knots = vec![0.0, 0.0, 0.0, 0.0, 0.5, 0.5, 0.5, 0.5];
        assert!(matches!(
            rejected(&s),
            LoftSpecError::Curve {
                curve: "zc",
                problem: SplineProblem::Invalid(_)
            } | LoftSpecError::Curve {
                curve: "zc",
                problem: SplineProblem::Domain { .. }
            }
        ));
        let mut s = sphere();
        s.curves.n.knots = vec![0.0, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0, 2.0];
        assert!(matches!(
            rejected(&s),
            LoftSpecError::Curve {
                curve: "n",
                problem: SplineProblem::Domain { .. }
            }
        ));
    }

    #[test]
    fn loft_validation_limits_interior_knot_multiplicity_to_keep_g2() {
        let mut s = sphere();
        s.curves.k.knots = vec![0.0, 0.0, 0.0, 0.0, 0.5, 0.5, 1.0, 1.0, 1.0, 1.0];
        s.curves.k.coeffs = vec![0.0; 6];
        assert!(matches!(
            rejected(&s),
            LoftSpecError::Curve {
                curve: "k",
                problem: SplineProblem::Multiplicity { .. }
            }
        ));
        // A quintic may repeat an interior knot up to three times.
        s.degree = 5;
        let clamped5 = |coeffs: usize, interior: &[f64]| {
            let mut knots = vec![0.0; 6];
            knots.extend_from_slice(interior);
            knots.extend(vec![1.0; 6]);
            assert_eq!(knots.len(), coeffs + 6);
            knots
        };
        for curve in [
            &mut s.curves.a,
            &mut s.curves.e,
            &mut s.curves.zc,
            &mut s.curves.n,
            &mut s.curves.k,
        ] {
            curve.knots = clamped5(9, &[0.5, 0.5, 0.5]);
            let v = curve.coeffs[1];
            curve.coeffs = vec![v; 9];
        }
        s.curves.a.coeffs = vec![1.0; 9];
        s.curves.e.coeffs = vec![1.0; 9];
        s.curves.zc.coeffs = vec![1.0; 9];
        s.curves.n.coeffs = vec![2.0; 9];
        s.curves.k.coeffs = vec![0.0; 9];
        s.front = Closure::Base;
        s.rear = Closure::Base;
        assert!(s.validate().is_ok(), "{:?}", s.validate());
        s.curves.a.knots = clamped5(10, &[0.5, 0.5, 0.5, 0.5]);
        s.curves.a.coeffs = vec![1.0; 10];
        assert!(matches!(
            rejected(&s),
            LoftSpecError::Curve {
                curve: "A",
                problem: SplineProblem::Multiplicity { .. }
            }
        ));
    }

    #[test]
    fn loft_validation_bounds_the_extents_exponent_and_shoulder() {
        let mut s = sphere();
        s.curves.a.coeffs[1] = -1.0;
        assert!(matches!(
            rejected(&s),
            LoftSpecError::Curve {
                curve: "A",
                problem: SplineProblem::Range { .. }
            }
        ));
        let mut s = sphere();
        s.curves.n.coeffs = vec![1.9; 4];
        assert!(matches!(
            rejected(&s),
            LoftSpecError::Curve {
                curve: "n",
                problem: SplineProblem::Range { .. }
            }
        ));
        let mut s = sphere();
        s.curves.n.coeffs = vec![6.5; 4];
        assert!(matches!(
            rejected(&s),
            LoftSpecError::Curve { curve: "n", .. }
        ));
        let mut s = sphere();
        s.curves.k.coeffs = vec![0.5; 4];
        assert!(matches!(
            rejected(&s),
            LoftSpecError::Curve { curve: "k", .. }
        ));
        let mut s = sphere();
        s.curves.zc.coeffs[2] = f64::INFINITY;
        assert!(matches!(
            rejected(&s),
            LoftSpecError::Curve { curve: "zc", .. }
        ));
    }

    #[test]
    fn loft_validation_checks_closures_cabin_and_fairness() {
        let mut s = sphere();
        s.front = Closure::Base;
        assert!(matches!(
            rejected(&s),
            LoftSpecError::Closure { end: "front", .. }
        ));
        let mut s = sphere();
        s.rear = Closure::Point { tip_radius_m: -1.0 };
        assert!(matches!(
            rejected(&s),
            LoftSpecError::Closure { end: "rear", .. }
        ));
        let mut s = sphere();
        s.curves.a.coeffs[0] = 0.5;
        assert!(matches!(
            rejected(&s),
            LoftSpecError::Closure { end: "front", .. }
        ));
        let mut s = sphere();
        s.cabin.x_end_m = 3.0;
        assert!(matches!(rejected(&s), LoftSpecError::Cabin { .. }));
        let mut s = sphere();
        s.cabin.x_start_m = 1.5;
        assert!(matches!(rejected(&s), LoftSpecError::Cabin { .. }));
        let mut s = sphere();
        s.fairness.tv_ratio_max = 0.5;
        assert!(matches!(rejected(&s), LoftSpecError::Fairness(_)));
    }

    #[test]
    fn loft_validation_requires_a_regular_pole() {
        // n(0) != 2 or k(0) != 0: curvature at the pole depends on direction.
        let mut s = sphere();
        s.curves.n.coeffs = vec![3.0, 2.0, 2.0, 2.0];
        assert!(matches!(
            rejected(&s),
            LoftSpecError::Closure { end: "front", .. }
        ));
        let mut s = sphere();
        s.curves.k.coeffs = vec![0.3, 0.0, 0.0, 0.0];
        assert!(matches!(
            rejected(&s),
            LoftSpecError::Closure { end: "front", .. }
        ));
        // A cone tip (A' = 0) has no finite tip radius.
        let mut s = sphere();
        s.curves.a.coeffs = vec![0.0, 0.0, 1.3333333333333333, 0.0];
        assert!(matches!(
            rejected(&s),
            LoftSpecError::Closure { end: "front", .. }
        ));
        // The stated radius must match the curves at either end.
        let mut s = sphere();
        s.front = Closure::Point { tip_radius_m: 0.5 };
        assert!(matches!(
            rejected(&s),
            LoftSpecError::Closure { end: "front", .. }
        ));
        let mut s = sphere();
        s.rear = Closure::Point { tip_radius_m: 1.2 };
        assert!(matches!(
            rejected(&s),
            LoftSpecError::Closure { end: "rear", .. }
        ));
        // A tail whose area grows towards the end is not a tail tip.
        let mut s = sphere();
        s.curves.e.coeffs = vec![0.0, 1.3333333333333333, 0.0, 0.0];
        assert!(matches!(
            rejected(&s),
            LoftSpecError::Closure { end: "rear", .. }
        ));
    }

    #[test]
    fn loft_validation_rejects_exponents_below_two() {
        let mut s = sphere();
        s.curves.n.coeffs = vec![2.0, 1.99, 2.0, 2.0];
        assert!(matches!(
            rejected(&s),
            LoftSpecError::Curve {
                curve: "n",
                problem: SplineProblem::Range { .. }
            }
        ));
    }
}
