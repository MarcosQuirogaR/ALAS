// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The five body curves of a loft with the length / scale remap of design 5.3.
//!
//! The data file stores each curve on `u = x_ref / L0` for the reference length
//! `L0`. A built loft has its own length and optional nose, tail, width and
//! height scales. The axial map from the built coordinate `x` (m aft of the
//! nose tip) to the reference coordinate is piecewise linear with its breaks
//! at the ends of the constant cabin cylinder, where every derivative of every
//! curve vanishes, so the jump of the map's slope cannot show in any curvature.
//! Without a cylinder only a uniform stretch is possible.

use alas_config::loft::MAX_RESIDUAL_TERMS;
use alas_config::LoftSpec;
use alas_math::clamped_bspline::{ClampedBSpline, MAX_DERIVATIVE};

use super::{LoftError, LoftParams};

/// A value and its first three derivatives with respect to built `x`.
pub(crate) type Jet = [f64; MAX_DERIVATIVE + 1];

/// All five curves at one station, as jets in built `x`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Jets {
    /// `A = a^2` (scaled by the width scale squared).
    pub a2: Jet,
    /// `E = e^2` (scaled by the height scale squared).
    pub e2: Jet,
    /// Centre height.
    pub zc: Jet,
    /// Exponent.
    pub n: Jet,
    /// Shoulder offset.
    pub k: Jet,
    /// Upper-arc residual coefficients (zero jets without a residual).
    pub b: [Jet; MAX_RESIDUAL_TERMS],
}

/// Which side of a knot a one-sided evaluation takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    /// The span ending at the point.
    Left,
    /// The span starting at the point.
    Right,
}

/// One linear piece of the axial map.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Piece {
    /// Built `x` at the start of the piece.
    x0: f64,
    /// Built `x` at the end of the piece.
    x1: f64,
    /// Reference `x` at the start of the piece.
    r0: f64,
    /// `d x_ref / d x` on the piece.
    slope: f64,
}

/// The body curves of one built loft.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Curves {
    a: ClampedBSpline,
    e: ClampedBSpline,
    zc: ClampedBSpline,
    n: ClampedBSpline,
    k: ClampedBSpline,
    residual: Vec<ClampedBSpline>,
    pieces: Vec<Piece>,
    ref_length: f64,
    width_scale: f64,
    height_scale: f64,
    zc_anchor: f64,
    flat_zone: Option<(f64, f64)>,
}

/// Tolerance, relative to the curve scale, for "constant" coefficients.
const FLAT_RELATIVE_TOLERANCE: f64 = 1e-6;
/// Shortest cylinder, as a fraction of the reference length, worth anchoring.
const MIN_FLAT_FRACTION: f64 = 1e-3;

fn build_spline(
    name: &'static str,
    spec: &alas_config::loft::SplineSpec,
    degree: usize,
) -> Result<ClampedBSpline, LoftError> {
    ClampedBSpline::new(spec.knots.clone(), spec.coeffs.clone(), degree).map_err(|source| {
        LoftError::Spline {
            curve: name,
            source,
        }
    })
}

/// The longest `u` interval on which all five splines are constant.
fn flat_zone_u(splines: &[&ClampedBSpline]) -> Option<(f64, f64)> {
    let mut common: Option<Vec<(f64, f64)>> = None;
    for s in splines {
        let scale = s.coeffs().iter().fold(1.0_f64, |m, c| m.max(c.abs()));
        let own = s.constant_intervals(FLAT_RELATIVE_TOLERANCE * scale);
        common = Some(match common {
            None => own,
            Some(prev) => {
                let mut out = Vec::new();
                for &(a0, a1) in &prev {
                    for &(b0, b1) in &own {
                        let (lo, hi) = (a0.max(b0), a1.min(b1));
                        if hi > lo {
                            out.push((lo, hi));
                        }
                    }
                }
                out
            }
        });
    }
    common?
        .into_iter()
        .filter(|(lo, hi)| hi - lo > MIN_FLAT_FRACTION)
        .max_by(|p, q| (p.1 - p.0).total_cmp(&(q.1 - q.0)))
}

impl Curves {
    pub(crate) fn new(spec: &LoftSpec, params: &LoftParams) -> Result<Self, LoftError> {
        let degree = usize::from(spec.degree);
        let a = build_spline("A", &spec.curves.a, degree)?;
        let e = build_spline("E", &spec.curves.e, degree)?;
        let zc = build_spline("zc", &spec.curves.zc, degree)?;
        let n = build_spline("n", &spec.curves.n, degree)?;
        let k = build_spline("k", &spec.curves.k, degree)?;
        let mut residual = Vec::new();
        if let Some(r) = &spec.residual {
            // A residual whose coefficients are all exactly zero is no residual.
            if r.terms.iter().any(|t| t.coeffs.iter().any(|c| *c != 0.0)) {
                for t in &r.terms {
                    residual.push(build_spline("residual", t, degree)?);
                }
            }
        }
        let l0 = spec.reference_length_m;
        let mut all = vec![&a, &e, &zc, &n, &k];
        all.extend(residual.iter());
        let flat_zone = flat_zone_u(&all).map(|(lo, hi)| (lo * l0, hi * l0));
        let length = params.length_m;

        let mut pieces = Vec::new();
        let uniform = |pieces: &mut Vec<Piece>| {
            pieces.push(Piece {
                x0: 0.0,
                x1: length,
                r0: 0.0,
                slope: l0 / length,
            });
        };
        match flat_zone {
            Some((xa, xb)) => {
                let nose = params.nose_scale * xa;
                let tail = params.tail_scale * (l0 - xb);
                let middle = length - nose - tail;
                if middle <= 1e-6 * l0 {
                    return Err(LoftError::LengthTooShort {
                        length_m: length,
                        needed_m: nose + tail,
                    });
                }
                if xa > 0.0 {
                    pieces.push(Piece {
                        x0: 0.0,
                        x1: nose,
                        r0: 0.0,
                        slope: 1.0 / params.nose_scale,
                    });
                }
                pieces.push(Piece {
                    x0: nose,
                    x1: nose + middle,
                    r0: xa,
                    slope: (xb - xa) / middle,
                });
                if xb < l0 {
                    pieces.push(Piece {
                        x0: nose + middle,
                        x1: length,
                        r0: xb,
                        slope: 1.0 / params.tail_scale,
                    });
                }
            }
            None => {
                if params.nose_scale != 1.0 || params.tail_scale != 1.0 {
                    return Err(LoftError::NoCabinCylinder);
                }
                uniform(&mut pieces);
            }
        }

        let zc_anchor = match flat_zone {
            Some((xa, xb)) => zc.eval(0.5 * (xa + xb) / l0),
            None => zc.eval(0.5),
        };
        Ok(Self {
            a,
            e,
            zc,
            n,
            k,
            residual,
            pieces,
            ref_length: l0,
            width_scale: params.width_scale,
            height_scale: params.height_scale,
            zc_anchor,
            flat_zone,
        })
    }

    /// `true` if the body carries a (non-zero) cross-section residual.
    pub(crate) fn has_residual(&self) -> bool {
        !self.residual.is_empty()
    }

    /// Built `x` of the span boundaries of the residual coefficient curves.
    pub(crate) fn residual_breakpoints(&self) -> Vec<f64> {
        let mut all: Vec<f64> = self
            .residual
            .iter()
            .flat_map(|s| s.breakpoints())
            .map(|u| self.to_built(u * self.ref_length))
            .collect();
        all.sort_by(f64::total_cmp);
        all.dedup_by(|b, a| (*b - *a).abs() < 1e-9);
        all
    }

    /// The cylinder `(x_start, x_end)` in the built length, if any.
    pub(crate) fn flat_zone_built(&self) -> Option<(f64, f64)> {
        let (xa, xb) = self.flat_zone?;
        Some((self.to_built(xa), self.to_built(xb)))
    }

    fn piece_of(&self, x: f64) -> &Piece {
        self.pieces
            .iter()
            .find(|p| x < p.x1)
            .unwrap_or_else(|| &self.pieces[self.pieces.len() - 1])
    }

    /// `(x_ref, d x_ref / d x)` at built `x` (clamped into the body).
    fn to_ref(&self, x: f64) -> (f64, f64) {
        let p = self.piece_of(x);
        (p.r0 + p.slope * (x - p.x0), p.slope)
    }

    /// Built `x` of reference coordinate `x_ref`.
    pub(crate) fn to_built(&self, x_ref: f64) -> f64 {
        let p = self
            .pieces
            .iter()
            .find(|p| x_ref < p.r0 + p.slope * (p.x1 - p.x0))
            .unwrap_or_else(|| &self.pieces[self.pieces.len() - 1]);
        p.x0 + (x_ref - p.r0) / p.slope
    }

    /// Built length of the body.
    pub(crate) fn length(&self) -> f64 {
        self.pieces.last().map_or(0.0, |p| p.x1)
    }

    /// Built `x` of every span boundary of every curve and of every map break,
    /// ascending and de-duplicated.
    pub(crate) fn breakpoints(&self) -> Vec<f64> {
        let mut all: Vec<f64> = Vec::new();
        let splines = [&self.a, &self.e, &self.zc, &self.n, &self.k]
            .into_iter()
            .chain(self.residual.iter());
        for s in splines {
            all.extend(
                s.breakpoints()
                    .into_iter()
                    .map(|u| self.to_built(u * self.ref_length)),
            );
        }
        all.extend(self.pieces.iter().flat_map(|p| [p.x0, p.x1]));
        all.sort_by(f64::total_cmp);
        let tol = 1e-9 * self.length().max(1.0);
        all.dedup_by(|b, a| (*b - *a).abs() < tol);
        all
    }

    fn jet(&self, spline: &ClampedBSpline, x: f64, side: Option<Side>) -> Jet {
        let (x_ref, slope) = self.to_ref(x);
        let u = x_ref / self.ref_length;
        let raw = match side {
            None => spline.derivatives(u),
            Some(side) => {
                let (lo, hi) = spline.domain();
                let eps = 1e-9;
                let probe = match side {
                    Side::Left => (u - eps).max(lo),
                    Side::Right => (u + eps).min(hi),
                };
                spline.derivatives_in_span(spline.span_of(probe), u.clamp(lo, hi))
            }
        };
        let f = slope / self.ref_length;
        [raw[0], raw[1] * f, raw[2] * f * f, raw[3] * f * f * f]
    }

    /// All curves at built `x`; `side` selects a one-sided limit at a knot.
    pub(crate) fn jets(&self, x: f64, side: Option<Side>) -> Jets {
        let x = x.clamp(0.0, self.length());
        let mut a2 = self.jet(&self.a, x, side);
        let mut e2 = self.jet(&self.e, x, side);
        let mut zc = self.jet(&self.zc, x, side);
        let n = self.jet(&self.n, x, side);
        let k = self.jet(&self.k, x, side);
        let mut b = [[0.0; MAX_DERIVATIVE + 1]; MAX_RESIDUAL_TERMS];
        for (slot, s) in b.iter_mut().zip(&self.residual) {
            *slot = self.jet(s, x, side);
        }
        let (w2, h2) = (self.width_scale.powi(2), self.height_scale.powi(2));
        for v in &mut a2 {
            *v *= w2;
        }
        for v in &mut e2 {
            *v *= h2;
        }
        zc[0] = self.zc_anchor + self.height_scale * (zc[0] - self.zc_anchor);
        for v in &mut zc[1..] {
            *v *= self.height_scale;
        }
        Jets {
            a2,
            e2,
            zc,
            n,
            k,
            b,
        }
    }
}
