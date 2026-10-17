// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Lofted fuselage-class bodies: G2 B-spline profile curves and a closed-form
//! cross-section family (design 2026-10-10, sections 2 and 5).
//!
//! A [`Loft`] is built from a [`LoftSpec`] (the data-file schema owned by
//! `alas-config`) and [`LoftParams`] (built length, width / height / nose /
//! tail scales, origin). Five clamped B-spline curves of the axial coordinate
//! give the squared half-width `A = a^2`, squared half-height `E = e^2`, the
//! centre height `zc`, the superellipse exponent `n` and the shoulder offset
//! `k` of the [`Section`] at each station. Everything the consumers need is
//! analytic: section area and its axial derivative (area ruling), wetted area
//! and volume by composite Gauss-Legendre quadrature of smooth integrands,
//! half-width at a height by closed-form inversion, the exact tail-scrape
//! angle, and fairness diagnostics of the profile lines. [`Loft::sample_xsecs`]
//! gives the legacy station view for consumers that still read polylines.
//!
//! # Frame and units
//!
//! Metres and degrees. x aft of the nose tip, y right, z up, the fuselage frame
//! of the preset (the z datum is the preset's). Every query takes and returns
//! coordinates in the output frame, that is the loft frame translated by
//! [`LoftParams::origin`]; with the default zero origin the two coincide.
//! Arguments outside the body are clamped to its ends by the curve queries
//! and give zero area and width.
//!
//! # Scaling
//!
//! Length changes stretch the constant cabin cylinder (found from the curves:
//! the longest axial interval on which all five are constant); the nose and
//! tail translate rigidly, or are scaled in length by `nose_scale` and
//! `tail_scale`. `width_scale` multiplies `a`, `height_scale` multiplies `e`
//! and the offset of `zc` from the cabin centre. A body without a cylinder
//! can only be stretched uniformly.

mod body;
mod curves;
#[cfg(test)]
mod data_tests;
mod fairness;
mod integrals;
mod legacy;
mod nacelle;
mod nacelle_integrals;
mod nacelle_pieces;
#[cfg(test)]
mod nacelle_tests;
mod polyline;
mod residual;
#[cfg(test)]
mod residual_tests;
mod sampling;
mod section;
#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests;

use std::sync::OnceLock;

pub use alas_config::loft::Closure;
pub use alas_config::{LoftSpec, LoftSpecError};
pub use body::{SectionRates, TipRadii};
pub use fairness::{FairnessReport, LineFairness};
pub use legacy::{
    from_legacy, from_legacy_with, CurveFitReport, LegacyFit, LegacyFitOptions, LegacyFitReport,
};
pub use nacelle::{NacelleBodyKind, NacelleError, NacelleLoft};
pub use polyline::SampledPolyline;
pub use residual::ResidualDefect;
pub use section::{unit_area, unit_area_dn, Section};

/// Why a loft could not be built.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LoftError {
    /// The specification failed validation.
    #[error("invalid loft specification: {0}")]
    Spec(#[from] LoftSpecError),
    /// A curve could not be built.
    #[error("curve {curve}: {source}")]
    Spline {
        /// Curve name.
        curve: &'static str,
        /// Underlying error.
        source: alas_math::clamped_bspline::ClampedBSplineError,
    },
    /// A parameter is not finite and positive.
    #[error("loft parameter {0} must be finite and positive")]
    Parameter(&'static str),
    /// Nose or tail scaling needs a constant cabin cylinder to anchor to.
    #[error("nose and tail scaling need a constant cabin cylinder; this body has none")]
    NoCabinCylinder,
    /// The requested length leaves no room for the cabin cylinder.
    #[error("length {length_m} m does not exceed the nose and tail ({needed_m} m)")]
    LengthTooShort {
        /// Requested length.
        length_m: f64,
        /// Nose plus tail length at the requested scales.
        needed_m: f64,
    },
    /// The cross-section residual makes a section invalid (wider than the
    /// shoulder or not convex).
    #[error("residual section defect at x = {x_m} m: {defect}")]
    Residual {
        /// Station (loft frame) where the check failed, m.
        x_m: f64,
        /// What is wrong.
        defect: ResidualDefect,
    },
    /// Fitting the legacy laws failed.
    #[error("legacy fit failed: {0}")]
    Fit(String),
}

/// How a [`LoftSpec`] is instantiated.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoftParams {
    /// Built length, m.
    pub length_m: f64,
    /// Multiplier of the half-width, 1 for the reference width.
    pub width_scale: f64,
    /// Multiplier of the half-height and of the offset of `zc` from the cabin.
    pub height_scale: f64,
    /// Multiplier of the nose length (cabin cylinder anchored).
    pub nose_scale: f64,
    /// Multiplier of the tail length (cabin cylinder anchored).
    pub tail_scale: f64,
    /// Translation of the loft into the output frame, m.
    pub origin: [f64; 3],
}

impl LoftParams {
    /// Parameters with every scale 1 and zero origin, at `length_m`.
    #[must_use]
    pub fn new(length_m: f64) -> Self {
        Self {
            length_m,
            width_scale: 1.0,
            height_scale: 1.0,
            nose_scale: 1.0,
            tail_scale: 1.0,
            origin: [0.0; 3],
        }
    }

    /// The reference instantiation of `spec`: its own length, unscaled.
    #[must_use]
    pub fn reference(spec: &LoftSpec) -> Self {
        Self::new(spec.reference_length_m)
    }

    fn validate(&self) -> Result<(), LoftError> {
        for (name, v) in [
            ("length_m", self.length_m),
            ("width_scale", self.width_scale),
            ("height_scale", self.height_scale),
            ("nose_scale", self.nose_scale),
            ("tail_scale", self.tail_scale),
        ] {
            if !(v.is_finite() && v > 0.0) {
                return Err(LoftError::Parameter(name));
            }
        }
        if self.origin.iter().any(|v| !v.is_finite()) {
            return Err(LoftError::Parameter("origin"));
        }
        Ok(())
    }
}

/// Wetted area and volume, computed once per loft.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Integrals {
    wetted_m2: f64,
    volume_m3: f64,
}

/// A lofted body: curves, section family and cached integrals.
#[derive(Debug, Clone)]
pub struct Loft {
    spec: LoftSpec,
    params: LoftParams,
    curves: curves::Curves,
    integrals: OnceLock<Integrals>,
}

impl PartialEq for Loft {
    fn eq(&self, other: &Self) -> bool {
        self.spec == other.spec && self.params == other.params
    }
}

impl Loft {
    /// Build a loft from a specification.
    ///
    /// # Errors
    ///
    /// [`LoftError`] when the specification or parameters are invalid or the
    /// requested length leaves no cabin cylinder.
    pub fn new(spec: &LoftSpec, params: LoftParams) -> Result<Self, LoftError> {
        spec.validate()?;
        params.validate()?;
        let curves = curves::Curves::new(spec, &params)?;
        let loft = Self {
            spec: spec.clone(),
            params,
            curves,
            integrals: OnceLock::new(),
        };
        loft.check_residual()?;
        Ok(loft)
    }

    /// Check every section of a body with a residual: the corrected upper arc
    /// may not exceed the shoulder and the section must be convex. Stations:
    /// six per span of the residual coefficient curves.
    fn check_residual(&self) -> Result<(), LoftError> {
        if !self.curves.has_residual() {
            return Ok(());
        }
        let breaks = self.curves.residual_breakpoints();
        for w in breaks.windows(2) {
            for i in 0..=6 {
                let x = w[0] + (w[1] - w[0]) * f64::from(i) / 6.0;
                let (_, j) = self.local_state(x + self.params.origin[0]);
                body::section_from(&j, 0.0)
                    .check_residual()
                    .map_err(|defect| LoftError::Residual { x_m: x, defect })?;
            }
        }
        Ok(())
    }

    /// The loft at the specification's own length and scales.
    ///
    /// # Errors
    ///
    /// As [`Loft::new`].
    pub fn from_reference(spec: &LoftSpec) -> Result<Self, LoftError> {
        Self::new(spec, LoftParams::reference(spec))
    }

    /// The specification this loft was built from.
    #[must_use]
    pub fn spec(&self) -> &LoftSpec {
        &self.spec
    }

    /// The instantiation parameters.
    #[must_use]
    pub fn params(&self) -> &LoftParams {
        &self.params
    }

    /// Built length, m.
    #[must_use]
    pub fn length_m(&self) -> f64 {
        self.params.length_m
    }

    /// Output-frame `x` of the nose tip and of the tail end.
    #[must_use]
    pub fn x_range(&self) -> (f64, f64) {
        let x0 = self.params.origin[0];
        (x0, x0 + self.params.length_m)
    }

    /// Output-frame `x` interval of the constant cabin cylinder, if the body
    /// has one.
    #[must_use]
    pub fn cabin_cylinder(&self) -> Option<(f64, f64)> {
        let (a, b) = self.curves.flat_zone_built()?;
        let x0 = self.params.origin[0];
        Some((x0 + a, x0 + b))
    }

    /// A copy of this loft translated by `delta` in the output frame. The
    /// cached integrals are invariant under translation and carry over.
    #[must_use]
    pub fn translated(&self, delta: [f64; 3]) -> Self {
        let mut copy = self.clone();
        for (o, d) in copy.params.origin.iter_mut().zip(delta) {
            *o += d;
        }
        copy
    }

    /// Cache for the integrals; used by the integral module.
    fn integrals_cell(&self) -> &OnceLock<Integrals> {
        &self.integrals
    }
}
