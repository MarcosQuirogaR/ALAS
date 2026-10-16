// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The serde types of loft data-file schema v1 (frozen 2026-10-11).

use serde::{Deserialize, Serialize};

use super::residual::ResidualSpec;

/// The only schema version this build reads.
pub const LOFT_SCHEMA_VERSION: u32 = 1;

fn default_body() -> String {
    "fuselage".to_owned()
}

/// A clamped B-spline in `u = x / reference_length_m` on `[0, 1]`:
/// `knots.len() == coeffs.len() + degree + 1`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplineSpec {
    /// Non-decreasing clamped knot vector over `[0, 1]`.
    pub knots: Vec<f64>,
    /// Control coefficients, in the unit of the curve.
    pub coeffs: Vec<f64>,
}

/// The five curves of the section family.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BodyCurves {
    /// `A = a^2`, squared plan half-width, m^2.
    #[serde(rename = "A")]
    pub a: SplineSpec,
    /// `E = e^2`, squared half-height, m^2.
    #[serde(rename = "E")]
    pub e: SplineSpec,
    /// Section centre height, m.
    pub zc: SplineSpec,
    /// Superellipse exponent, `2 <= n <= 6`.
    pub n: SplineSpec,
    /// Shoulder offset, `abs(k) < 0.5`.
    pub k: SplineSpec,
}

/// How an end of the body closes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Closure {
    /// `A = E = 0` at the end, an elliptic-paraboloid cap with plan and side
    /// radii of curvature `dA/dx / 2` and `dE/dx / 2` at the tip.
    Point {
        /// Mean of the plan and side radii of curvature of the tip paraboloid, m;
        /// `validate` requires it to equal `(A'/2 + E'/2) / 2` at the end.
        tip_radius_m: f64,
    },
    /// A flat base with `A > 0` and `E > 0` at the end (tail-cone cap, APU
    /// exhaust); the base is not wetted skin and is reported separately.
    Base,
}

/// Axial stations of the payload cabin in the reference length, m. Payload
/// semantics are unchanged by the loft (the cabin fields of `FuselageConfig`
/// stay authoritative); these locate the cabin on the lofted body.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CabinStations {
    /// Forward end of the cabin, m aft of the nose tip.
    pub x_start_m: f64,
    /// Aft end of the cabin, m aft of the nose tip.
    pub x_end_m: f64,
}

/// Expected curvature sign changes of the crown, keel and plan lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InflectionBudget {
    /// Crown line `zc + e`.
    pub crown: u32,
    /// Keel line `zc - e`.
    pub keel: u32,
    /// Plan half-width line `a`.
    pub plan: u32,
}

/// What a fitted body is allowed to do before it counts as unfair.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FairnessBudget {
    /// Allowed inflections per line.
    pub inflections: InflectionBudget,
    /// Largest ratio of a line's curvature total variation to the minimum for
    /// a profile with the same range and end values (1 is two clean lobes).
    pub tv_ratio_max: f64,
}

/// What the fit achieved against its drawing traces.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FitRecord {
    /// RMS deviation to the accepted samples, m.
    #[serde(default)]
    pub rms_m: f64,
    /// Largest deviation to the accepted samples, m.
    #[serde(default)]
    pub max_m: f64,
    /// Roughness penalty weight of the fit.
    #[serde(default)]
    pub lambda: f64,
    /// Name and version of the fitting tool.
    #[serde(default)]
    pub tool_version: String,
}

/// Where a parameter group came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProvenanceKind {
    /// Traced from a drawing.
    Measured,
    /// A dimension printed on a drawing.
    Dimension,
    /// A class template or estimate, not read from this aircraft's drawing.
    #[default]
    Template,
}

/// Confidence of a source: `A` vector PDF, `B` clean raster, `C` photograph,
/// render or stand-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Confidence {
    /// Vector PDF drawing.
    A,
    /// Clean raster drawing.
    B,
    /// Photograph, render or stand-in.
    #[default]
    C,
}

/// A drawing in the reference library.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DrawingRef {
    /// Library identifier.
    #[serde(default)]
    pub id: String,
    /// Drawing title.
    #[serde(default)]
    pub title: String,
    /// Page, figure or section of the document; empty if none. Older files
    /// wrote an integer page, which is read as its decimal text.
    #[serde(default, deserialize_with = "page_text")]
    pub page: String,
    /// SHA-256 of the source file.
    #[serde(default)]
    pub sha256: String,
    /// Library path.
    #[serde(default)]
    pub path: String,
}

/// A page given as a number or as free text (`"2-2-0 p2 (side)"`).
fn page_text<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Page {
        Number(u64),
        Text(String),
    }
    Ok(match Page::deserialize(deserializer)? {
        Page::Number(0) => String::new(),
        Page::Number(n) => n.to_string(),
        Page::Text(s) => s,
    })
}

/// Provenance of one parameter group (`A`, `E`, `zc`, `n`, `k`, `closure`,
/// `cabin`). Extra fields written by a fitting tool are ignored on read.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SourceRef {
    /// Parameter group the entry describes.
    pub group: String,
    /// Measured, dimension or template.
    #[serde(default)]
    pub kind: ProvenanceKind,
    /// Source drawing.
    #[serde(default)]
    pub drawing: DrawingRef,
    /// View the trace came from (`side`, `top`, `front`).
    #[serde(default)]
    pub view: String,
    /// Calibration of the drawing, pixels per metre.
    #[serde(default)]
    pub px_per_m: f64,
    /// Number of traced samples behind the group.
    #[serde(default)]
    pub n_samples: u32,
    /// RMS deviation of the group to its samples, m.
    #[serde(default)]
    pub rms_m: f64,
    /// Largest deviation of the group to its samples, m.
    #[serde(default)]
    pub max_m: f64,
    /// Confidence of the source.
    #[serde(default)]
    pub confidence: Confidence,
    /// Free-text note.
    #[serde(default)]
    pub note: String,
}

/// One lofted body, as stored in `data/lofts/<id>.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoftSpec {
    /// Schema version; must be [`LOFT_SCHEMA_VERSION`].
    pub schema: u32,
    /// Preset the body was fitted for.
    pub preset: String,
    /// Body kind; `fuselage` for the main body.
    #[serde(default = "default_body")]
    pub body: String,
    /// Length of the fit, m; the curves live on `u = x / reference_length_m`.
    pub reference_length_m: f64,
    /// Spline degree: 3 (G2) or 5 (G2 with continuous curvature slope).
    pub degree: u8,
    /// Free-text statement of the frame and datum.
    #[serde(default)]
    pub frame: String,
    /// The five curves.
    pub curves: BodyCurves,
    /// Nose closure.
    pub front: Closure,
    /// Tail closure.
    pub rear: Closure,
    /// Cabin stations in the reference length.
    pub cabin: CabinStations,
    /// Fairness budget the body must meet.
    pub fairness: FairnessBudget,
    /// Fit statistics.
    #[serde(default)]
    pub fit: FitRecord,
    /// Provenance records.
    #[serde(default)]
    pub provenance: Vec<SourceRef>,
    /// Optional upper-arc cross-section residual (schema 1, additive minor
    /// extension); absent means the plain warped superellipse.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub residual: Option<ResidualSpec>,
}
