// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! `from_legacy`: a [`LoftSpec`] fitted to the parametric fuselage laws
//! (design 5.1).
//!
//! The laws of [`FuselageConfig`] (`body_station` with the upper-deck hump
//! raised by `raise_crown`) are sampled at about 500 stations clustered towards
//! both ends and fitted with the penalised least-squares spline of
//! `alas_math::spline_fit` as the "drawing": `A = a^2` (plan line), and `zc`
//! with `E = e^2` on one shared knot vector (crown and keel lines); `n` on its
//! own; `k = 0` (the legacy sections have no shoulder). The fit:
//!
//! * pins the tip exactly: `A = E = 0`, a prescribed tip radius (`A' = E' =
//!   2 R`, `R` estimated from the law just behind the tip), `n = 2`, `k = 0`;
//!   a tail that closes to a point gets a 0.03 m radius;
//! * pins the constant cabin exactly with triple knots at its ends (C2, so the
//!   curvature slope may jump there) and `p' = p'' = 0` at both ends, `p''' = 0`
//!   on the cabin side of the forward end; the cabin sections equal the legacy
//!   ones and the length remap of the built loft is possible;
//! * puts triple knots on the slope breaks and curvature steps of the laws
//!   (`features`), so the spline does not ring around them;
//! * inserts simple knots where the error of the physical outline (crown,
//!   keel, plan: distance to the sampled polyline) is largest, then raises the
//!   penalty on the third derivative to the largest value that keeps every
//!   outline within the deviation bound.
//!
//! The legacy laws have infinite curvature at the cabin end of the tailcone;
//! the C2 fit rounds it. A body with an upper-deck hump has no constant cabin,
//! so no cylinder is pinned (and no length remap is then available).
//!
//! The stored fairness budget is absolute: the inflection counts of the legacy
//! laws themselves (same sign rule) plus a rounding allowance of 2 per line,
//! and a total-variation ratio limit of 3.

use alas_config::loft::{
    BodyCurves, CabinStations, Closure, Confidence, DrawingRef, FitRecord, ProvenanceKind,
    SourceRef, SplineSpec, LOFT_SCHEMA_VERSION,
};
use alas_config::{FuselageConfig, LoftSpec};
use alas_math::spline_fit::FitOutcome;

mod features;
mod fit;
mod law;
mod problem;
mod stages;

use fit::clamped_knots;
use law::law_budget;
use problem::Problem;

use super::LoftError;

/// Options of the legacy fit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LegacyFitOptions {
    /// Spline degree, 3 or 5 (triple knots at features need 5; with 3 simple
    /// knots are used).
    pub degree: u8,
    /// Insert knots while an outline error exceeds this, m.
    pub tolerance_m: f64,
    /// Penalty weight of the knot-insertion stage, relative to the data scale
    /// (small: the data decide the knots).
    pub lambda_rel: f64,
    /// Largest outline deviation the smoothing stage may accept, m.
    pub max_deviation_m: f64,
    /// Budget of knots inserted by error, per curve group.
    pub max_interior_knots: usize,
    /// Number of cosine-clustered plus uniform stations sampled.
    pub samples: usize,
    /// Tip radius given to a tail that closes to a point, m.
    pub rear_tip_radius_m: f64,
}

impl Default for LegacyFitOptions {
    fn default() -> Self {
        Self {
            degree: 5,
            tolerance_m: 0.01,
            lambda_rel: 1e-16,
            max_deviation_m: 0.03,
            max_interior_knots: 24,
            samples: 500,
            rear_tip_radius_m: 0.03,
        }
    }
}

/// Residual statistics of one fitted curve.
#[derive(Debug, Clone, PartialEq)]
pub struct CurveFitReport {
    /// `plan`, `crown`, `keel` or `n`.
    pub name: &'static str,
    /// Interior knots inserted by the error-driven growth.
    pub interior_knots: usize,
    /// RMS residual: metres of half-extent / height, or exponent units.
    pub rms: f64,
    /// Largest residual, same unit.
    pub max: f64,
}

/// What the fit achieved.
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyFitReport {
    /// Stations sampled.
    pub samples: usize,
    /// Per-curve statistics.
    pub curves: Vec<CurveFitReport>,
    /// RMS outline deviation over crown, keel and plan, m.
    pub rms_m: f64,
    /// Largest outline deviation over crown, keel and plan, m.
    pub max_m: f64,
    /// Stations of the law features that received triple knots, m.
    pub features_m: Vec<f64>,
}

/// A fitted specification and its report.
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyFit {
    /// The specification, validated.
    pub spec: LoftSpec,
    /// Fit statistics.
    pub report: LegacyFitReport,
}

/// Fit the legacy laws of `config` at fuselage length `length_m` with the
/// default options.
///
/// # Errors
///
/// [`LoftError::Fit`] when the laws have no nose / aft-body geometry to fit
/// (no room for a cabin) or the fit fails.
pub fn from_legacy(config: &FuselageConfig, length_m: f64) -> Result<LegacyFit, LoftError> {
    from_legacy_with(config, length_m, &LegacyFitOptions::default())
}

/// [`from_legacy`] with explicit options.
///
/// # Errors
///
/// As [`from_legacy`].
pub fn from_legacy_with(
    config: &FuselageConfig,
    length_m: f64,
    options: &LegacyFitOptions,
) -> Result<LegacyFit, LoftError> {
    let p = Problem::new(config, length_m, options)?;
    let plan = p.plan_stage()?;
    let lines = p.lines_stage()?;
    let exponent = p.exponent_stage()?;

    let spline_spec = |o: &FitOutcome, floor: Option<f64>| SplineSpec {
        knots: o.spline.knots().to_vec(),
        coeffs: o
            .spline
            .coeffs()
            .iter()
            .map(|&c| floor.map_or(c, |f| c.max(f)))
            .collect(),
    };
    let (r_front_a, r_front_e) = p.r_front;
    let mut spec = LoftSpec {
        schema: LOFT_SCHEMA_VERSION,
        preset: "legacy".to_owned(),
        body: "fuselage".to_owned(),
        reference_length_m: length_m,
        degree: options.degree,
        frame: "x aft of nose tip, y right, z up, metres; z datum of the preset fuselage frame"
            .to_owned(),
        curves: BodyCurves {
            a: spline_spec(&plan.curves[0], None),
            e: spline_spec(&lines.e2, None),
            zc: spline_spec(&lines.zc, None),
            // Raising every coefficient to at least 2 keeps n >= 2.
            n: spline_spec(&exponent.curves[0], Some(2.0)),
            k: SplineSpec {
                knots: clamped_knots(p.degree, &[]),
                coeffs: vec![0.0; p.degree + 1],
            },
        },
        front: Closure::Point {
            tip_radius_m: 0.5 * (r_front_a + r_front_e),
        },
        rear: if p.point_tail {
            Closure::Point {
                tip_radius_m: p.r_rear,
            }
        } else {
            Closure::Base
        },
        cabin: CabinStations {
            x_start_m: config.cabin_start_x_m.clamp(0.0, length_m),
            x_end_m: (length_m - config.tailcone_length_m).clamp(config.cabin_start_x_m, length_m),
        },
        fairness: law_budget(config, length_m),
        fit: FitRecord::default(),
        provenance: Vec::new(),
        residual: None,
    };

    let curves = vec![
        stat("plan", plan.inserted, &plan.errors),
        stat("crown", lines.inserted, &lines.crown_gap),
        stat("keel", lines.inserted, &lines.keel_gap),
        stat("n", exponent.inserted, &exponent.errors),
    ];
    let metric: Vec<f64> = plan
        .errors
        .iter()
        .chain(&lines.crown_gap)
        .chain(&lines.keel_gap)
        .copied()
        .collect();
    let rms_m = (metric.iter().map(|e| e * e).sum::<f64>() / metric.len() as f64).sqrt();
    let max_m = metric.iter().fold(0.0_f64, |m, e| m.max(*e));
    spec.fit = FitRecord {
        rms_m,
        max_m,
        lambda: options.lambda_rel,
        tool_version: "alas-geom from_legacy 2".to_owned(),
    };
    for group in ["A", "E", "zc", "n", "k", "closure", "cabin"] {
        spec.provenance.push(SourceRef {
            group: group.to_owned(),
            kind: ProvenanceKind::Template,
            drawing: DrawingRef::default(),
            view: String::new(),
            px_per_m: 0.0,
            n_samples: p.data.len() as u32,
            rms_m,
            max_m,
            confidence: Confidence::C,
            note: "sampled from the parametric fuselage laws; not a drawing measurement".to_owned(),
        });
    }
    spec.validate()?;
    Ok(LegacyFit {
        spec,
        report: LegacyFitReport {
            samples: p.data.len(),
            curves,
            rms_m,
            max_m,
            features_m: p.features,
        },
    })
}

fn stat(name: &'static str, inserted: usize, errors: &[f64]) -> CurveFitReport {
    CurveFitReport {
        name,
        interior_knots: inserted,
        rms: (errors.iter().map(|e| e * e).sum::<f64>() / errors.len().max(1) as f64).sqrt(),
        max: errors.iter().fold(0.0_f64, |m, e| m.max(*e)),
    }
}
