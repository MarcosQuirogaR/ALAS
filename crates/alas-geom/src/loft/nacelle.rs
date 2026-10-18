// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Nacelles and pods as a revolve of a G2 composite profile (design 3.1).
//!
//! # Profile
//!
//! Frame: x aft of the inlet highlight plane, r the local radius, metres.
//! The fan-cowl body is, in order,
//!
//! 1. the lip: a superellipse (exponent `n`, semi-axes `0.85 l_lip` and
//!    `R - r_hl`) from the highlight, where the tangent is vertical, to 60 %
//!    of its quarter turn;
//! 2. a quintic Hermite to the crest `(l_lip, R)` with slope and curvature
//!    zero, matched in position, slope and curvature to the lip at its start;
//! 3. the cylinder `r = R` to the end of the cylindrical cowl;
//! 4. the boat-tail, a quintic from the cylinder end to the fan-nozzle exit
//!    `(x_fn, r_fn)`, slope zero to `-tan(beta)`, curvature zero at both ends.
//!
//! The fan nozzle is a G0 step (an open annulus): the core body is separate.
//! It starts at `(x_fn, r_cn0)` with
//! `r_cn0 = r_cn + CORE_COWL_START_FRACTION (r_fn - r_cn)`, and is a quintic
//! to the core-nozzle exit (end slope 1.5 times the mean slope) followed by a
//! quintic plug to a flat tip disc of radius `r_tip`. When the core and fan
//! lengths are equal there is one body (a turboprop nacelle, a tail pod) whose
//! boat-tail runs into the plug without a step.
//!
//! # Revolve
//!
//! Section at a station: `a = r`, `e = ellipticity r`, `zc = -droop r / R`
//! (the [`Section`] family with `n = 2`, `k = 0`). Wetted area (lateral skin
//! only: no inlet duct, trailing-edge ring, step annulus or tip disc) and
//! volume are composite Gauss-Legendre quadratures of smooth integrands.

use std::f64::consts::FRAC_PI_2;
use std::sync::OnceLock;

use alas_config::engines::{NacelleShapeV2, CORE_COWL_START_FRACTION};

use super::nacelle_pieces::Piece;
use super::Section;

/// Lip superellipse semi-axis along x, over the lip length to the crest.
const LIP_AXIS_FRACTION: f64 = 0.85;
/// Fraction of the lip's quarter turn after which the quintic takes over.
const LIP_JOIN_QUARTER_TURN: f64 = 0.6;
/// Core-cowl end slope over its mean slope.
const CORE_END_SLOPE_FACTOR: f64 = 1.5;
/// Gap in x at the fan-nozzle step in the polyline view (zero in the revolve).
const POLYLINE_STEP_GAP_M: f64 = 1.0e-3;
/// Monotonicity tolerance of the radius slope on sampled profiles.
const SLOPE_TOLERANCE: f64 = 1.0e-9;

/// Why a nacelle could not be built.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum NacelleError {
    /// The shape numbers are outside the ranges the revolve can build.
    #[error(
        "nacelle shape is not valid for length {length_m} m and maximum radius {max_radius_m} m"
    )]
    Invalid {
        /// Total length.
        length_m: f64,
        /// Maximum radius.
        max_radius_m: f64,
    },
    /// The assembled profile is not a single-valued outline that rises to
    /// the crest and then falls (lip overshoot, boat-tail angle too large for
    /// the drawn radius drop, core cowl not tapering...).
    #[error("nacelle profile is not monotone: {0}")]
    NotMonotone(&'static str),
}

/// Which body of a nacelle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NacelleBodyKind {
    /// Lip, cylinder and boat-tail (and the plug, for a single body).
    FanCowl,
    /// Core cowl and plug behind the fan-nozzle step.
    CoreCowl,
}

/// One body of revolution of the nacelle.
#[derive(Debug, Clone)]
pub(super) struct Body {
    pub kind: NacelleBodyKind,
    pub pieces: Vec<Piece>,
    /// Number of leading pieces on which the radius rises (lip and crest).
    pub rising: usize,
}

/// Exact integrals, computed once.
#[derive(Debug, Clone, Copy)]
pub(super) struct NacelleIntegrals {
    pub wetted_m2: f64,
    pub volume_m3: f64,
}

/// A nacelle as a revolve of a G2 composite profile.
#[derive(Debug, Clone)]
pub struct NacelleLoft {
    pub(super) shape: NacelleShapeV2,
    pub(super) length_m: f64,
    pub(super) max_radius_m: f64,
    pub(super) bodies: Vec<Body>,
    pub(super) integrals: OnceLock<NacelleIntegrals>,
}

impl NacelleLoft {
    /// Build the nacelle of `shape` for a pod `length_m` long (highlight to
    /// the aft end) with maximum radius `max_radius_m`.
    ///
    /// # Errors
    ///
    /// [`NacelleError`] when the numbers are out of range or the profile is
    /// not monotone.
    pub fn new(
        shape: &NacelleShapeV2,
        length_m: f64,
        max_radius_m: f64,
    ) -> Result<Self, NacelleError> {
        if !shape.is_valid(length_m, max_radius_m) {
            return Err(NacelleError::Invalid {
                length_m,
                max_radius_m,
            });
        }
        let bodies = build_bodies(shape, length_m, max_radius_m)?;
        Ok(Self {
            shape: shape.clone(),
            length_m,
            max_radius_m,
            bodies,
            integrals: OnceLock::new(),
        })
    }

    /// The shape this nacelle was built from.
    #[must_use]
    pub fn shape(&self) -> &NacelleShapeV2 {
        &self.shape
    }

    /// Length from the highlight to the aft end, m.
    #[must_use]
    pub fn length_m(&self) -> f64 {
        self.length_m
    }

    /// Maximum radius (half-width) `R`, m.
    #[must_use]
    pub fn max_radius_m(&self) -> f64 {
        self.max_radius_m
    }

    /// Number of bodies (1, or 2 with the fan-nozzle step).
    #[must_use]
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    /// Kind of body `index`.
    #[must_use]
    pub fn body_kind(&self, index: usize) -> Option<NacelleBodyKind> {
        self.bodies.get(index).map(|b| b.kind)
    }

    /// Axial extent of body `index`.
    #[must_use]
    pub fn body_x_range(&self, index: usize) -> Option<(f64, f64)> {
        let body = self.bodies.get(index)?;
        Some((
            body.pieces.first()?.x_span().0,
            body.pieces.last()?.x_span().1,
        ))
    }

    /// The section at the radius `r`.
    fn section_at_radius(&self, r: f64) -> Section {
        Section::new(
            r,
            self.shape.ellipticity * r,
            -self.shape.droop_m * r / self.max_radius_m,
            2.0,
            0.0,
        )
    }

    /// Radius of body `index` at station `x` (`None` outside the body). The
    /// profile is single-valued in x, so the inversion is a bisection.
    #[must_use]
    pub fn radius_at(&self, index: usize, x: f64) -> Option<f64> {
        let body = self.bodies.get(index)?;
        for piece in &body.pieces {
            let (x0, x1) = piece.x_span();
            if x < x0 || x > x1 {
                continue;
            }
            let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
            for _ in 0..80 {
                let mid = 0.5 * (lo + hi);
                if piece.eval(mid).x < x {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            return Some(piece.eval(0.5 * (lo + hi)).r);
        }
        None
    }

    /// The cross-section of body `index` at station `x`.
    #[must_use]
    pub fn section_at(&self, index: usize, x: f64) -> Option<Section> {
        self.radius_at(index, x).map(|r| self.section_at_radius(r))
    }

    /// Stations `(x, section)` of body `index`, graded: `density` times 10 on
    /// the lip, 6 on the crest, 12 on a boat-tail or cowl, 2 on a cylinder.
    #[must_use]
    pub fn body_sections(&self, index: usize, density: usize) -> Vec<(f64, Section)> {
        let Some(body) = self.bodies.get(index) else {
            return Vec::new();
        };
        let density = density.max(1);
        let mut out: Vec<(f64, Section)> = Vec::new();
        for (k, piece) in body.pieces.iter().enumerate() {
            let count = if piece.is_cylinder() {
                2
            } else if matches!(piece, Piece::Lip { .. }) {
                10 * density
            } else if k < body.rising {
                6 * density
            } else {
                12 * density
            };
            for i in 0..=count {
                if i == 0 && !out.is_empty() {
                    continue;
                }
                let u = i as f64 / count as f64;
                // Grade the lip toward the highlight, where the curvature is large.
                let t = if matches!(piece, Piece::Lip { .. }) {
                    u.powf(1.6)
                } else {
                    u
                };
                let p = piece.eval(t);
                out.push((p.x, self.section_at_radius(p.r)));
            }
        }
        out
    }

    /// One strictly increasing station list for consumers that read a single
    /// polyline per nacelle: the fan-cowl stations, then the core-cowl stations
    /// squeezed behind a 1 mm gap so the step is a short frustum (its area is counted by a
    /// trapezoidal wetted area; [`Self::wetted_area_m2`] excludes it).
    #[must_use]
    pub fn polyline_sections(&self, density: usize) -> Vec<(f64, Section)> {
        let mut out = self.body_sections(0, density);
        if let (true, Some((x0, x1))) = (self.bodies.len() > 1, self.body_x_range(1)) {
            // Squeeze the core body into [x0 + gap, x1]: the total length is kept.
            let k = (x1 - x0 - POLYLINE_STEP_GAP_M) / (x1 - x0);
            out.extend(
                self.body_sections(1, density)
                    .into_iter()
                    .map(|(x, s)| (x0 + POLYLINE_STEP_GAP_M + (x - x0) * k, s)),
            );
        }
        out
    }

    /// Outline `(x, r)` of every body at `density`, for plots.
    #[must_use]
    pub fn outline(&self, density: usize) -> Vec<Vec<(f64, f64)>> {
        (0..self.bodies.len())
            .map(|i| {
                self.body_sections(i, density)
                    .into_iter()
                    .map(|(x, s)| (x, s.a))
                    .collect()
            })
            .collect()
    }

    /// Slope `dr/dx` and curvature `d2r/dx2` at the start and end of every
    /// piece boundary of body `index`: `(x, slope_before, slope_after,
    /// curvature_before, curvature_after)`; the joints of the composite.
    #[must_use]
    pub fn joints(&self, index: usize) -> Vec<(f64, f64, f64, f64, f64)> {
        let Some(body) = self.bodies.get(index) else {
            return Vec::new();
        };
        body.pieces
            .windows(2)
            .map(|pair| {
                let end = pair[0].eval(1.0);
                let start = pair[1].eval(0.0);
                (
                    end.x,
                    end.slope(),
                    start.slope(),
                    end.curvature(),
                    start.curvature(),
                )
            })
            .collect()
    }

    /// Radius at the fan-cowl trailing edge, m.
    #[must_use]
    pub fn fan_exit_radius_m(&self) -> f64 {
        self.shape.fan_nozzle_radius_ratio * self.max_radius_m
    }

    /// Area of the blunt fan-cowl trailing-edge ring, m2 (elliptic ring of
    /// outer radius `r_fn` and thickness `te_thickness_m`).
    #[must_use]
    pub fn trailing_edge_ring_area_m2(&self) -> f64 {
        let r = self.fan_exit_radius_m();
        let t = self.shape.te_thickness_m;
        std::f64::consts::PI * self.shape.ellipticity * (r * r - (r - t).powi(2))
    }

    /// Area of the flat disc that ends the plug (or the aft end), m2.
    #[must_use]
    pub fn plug_tip_area_m2(&self) -> f64 {
        let r_end = self
            .bodies
            .last()
            .map_or(0.0, |b| b.pieces.last().map_or(0.0, |p| p.eval(1.0).r));
        std::f64::consts::PI * self.shape.ellipticity * r_end * r_end
    }

    /// Area of the open fan-exhaust annulus between the trailing-edge ring and
    /// the start of the core cowl, m2 (zero for a single body).
    #[must_use]
    pub fn fan_exit_annulus_area_m2(&self) -> f64 {
        let Some(core) = self.bodies.get(1).and_then(|b| b.pieces.first()) else {
            return 0.0;
        };
        let inner = self.fan_exit_radius_m() - self.shape.te_thickness_m;
        let r_core = core.eval(0.0).r;
        std::f64::consts::PI * self.shape.ellipticity * (inner * inner - r_core * r_core)
    }

    /// Area of the open inlet highlight disc, m2 (the capture area of the
    /// outer envelope; the inlet duct is not modelled).
    #[must_use]
    pub fn highlight_area_m2(&self) -> f64 {
        let r = self.shape.highlight_radius_ratio * self.max_radius_m;
        std::f64::consts::PI * self.shape.ellipticity * r * r
    }
}

fn build_bodies(
    shape: &NacelleShapeV2,
    length_m: f64,
    r_max: f64,
) -> Result<Vec<Body>, NacelleError> {
    let r_hl = shape.highlight_radius_ratio * r_max;
    let delta = r_max - r_hl;
    let l_lip = shape.lip_length_ratio * r_max;
    let n = shape.lip_exponent;
    let w_end = (LIP_JOIN_QUARTER_TURN * FRAC_PI_2).sin().powf(2.0 / n);
    let lip = Piece::Lip {
        x0: 0.0,
        r0: r_hl,
        ax: LIP_AXIS_FRACTION * l_lip,
        delta,
        n,
        w_end,
    };
    let j = lip.eval(1.0);
    let crest = Piece::hermite(
        j.x,
        l_lip - j.x,
        j.r,
        j.slope(),
        j.curvature(),
        r_max,
        0.0,
        0.0,
    );
    let mut fan = vec![lip, crest];
    let rising = fan.len();
    let x_cyl = shape.cowl_cylinder_end_ratio * shape.fan_cowl_length_m;
    if x_cyl - l_lip > 1.0e-9 {
        fan.push(Piece::cylinder(l_lip, x_cyl - l_lip, r_max));
    }
    let x_fn = shape.fan_cowl_length_m;
    let r_fn = shape.fan_nozzle_radius_ratio * r_max;
    let tan_beta = shape.boat_tail_angle_deg.to_radians().tan();
    fan.push(Piece::hermite(
        x_cyl,
        x_fn - x_cyl,
        r_max,
        0.0,
        0.0,
        r_fn,
        -tan_beta,
        0.0,
    ));
    let r_tip = shape.plug_tip_radius_m;
    let mut bodies = Vec::with_capacity(2);
    let two_bodies = shape.core_cowl_length_m > x_fn + 1.0e-9;
    if two_bodies {
        bodies.push(Body {
            kind: NacelleBodyKind::FanCowl,
            pieces: fan,
            rising,
        });
        let x_cn = shape.core_cowl_length_m;
        let r_cn = shape.core_nozzle_radius_ratio * r_max;
        if r_cn >= r_fn - shape.te_thickness_m {
            return Err(NacelleError::NotMonotone(
                "core nozzle radius not below the fan nozzle radius",
            ));
        }
        let r_cn0 = r_cn + CORE_COWL_START_FRACTION * (r_fn - r_cn);
        let drop = r_cn0 - r_cn;
        let mut end_slope = -CORE_END_SLOPE_FACTOR * drop / (x_cn - x_fn);
        if length_m > x_cn + 1.0e-9 {
            // A long shallow plug cannot start steeper than twice its mean slope.
            let plug_mean = (r_cn - r_tip) / (length_m - x_cn);
            end_slope = end_slope.max(-2.0 * plug_mean);
        }
        let mut core = vec![Piece::hermite(
            x_fn,
            x_cn - x_fn,
            r_cn0,
            0.0,
            0.0,
            r_cn,
            end_slope,
            0.0,
        )];
        if length_m > x_cn + 1.0e-9 {
            core.push(plug(x_cn, length_m, r_cn, end_slope, r_tip)?);
        }
        bodies.push(Body {
            kind: NacelleBodyKind::CoreCowl,
            pieces: core,
            rising: 0,
        });
    } else {
        if length_m > x_fn + 1.0e-9 {
            fan.push(plug(x_fn, length_m, r_fn, -tan_beta, r_tip)?);
        }
        bodies.push(Body {
            kind: NacelleBodyKind::FanCowl,
            pieces: fan,
            rising,
        });
    }
    for body in &bodies {
        check_monotone(body, r_max)?;
    }
    Ok(bodies)
}

/// The plug: a quintic from `(x0, r0, slope m0)` to the tip disc at `x1`. The
/// end slope doubles the mean slope less the start slope (a slope ramp), so a
/// plug that starts shallow ends in a steeper, rounder tip.
fn plug(x0: f64, x1: f64, r0: f64, m0: f64, r_tip: f64) -> Result<Piece, NacelleError> {
    let len = x1 - x0;
    let drop = r0 - r_tip;
    if drop <= 0.0 {
        return Err(NacelleError::NotMonotone(
            "plug tip radius not below the nozzle",
        ));
    }
    let mean = drop / len;
    let end = (2.0 * mean - (-m0)).max(0.5 * mean);
    Ok(Piece::hermite(x0, len, r0, m0, 0.0, r_tip, -end, 0.0))
}

fn check_monotone(body: &Body, r_max: f64) -> Result<(), NacelleError> {
    for (k, piece) in body.pieces.iter().enumerate() {
        let rising = k < body.rising;
        for i in 0..=200 {
            let p = piece.eval(i as f64 / 200.0);
            if p.dx < 0.0 || p.r <= 0.0 || p.r > r_max * (1.0 + 1.0e-12) {
                return Err(NacelleError::NotMonotone(
                    "x not increasing or radius outside (0, R]",
                ));
            }
            let slope = p.dr;
            if rising && slope < -SLOPE_TOLERANCE * p.dx.max(1.0) {
                return Err(NacelleError::NotMonotone("radius falls before the crest"));
            }
            if !rising && slope > SLOPE_TOLERANCE * p.dx.max(1.0) {
                return Err(NacelleError::NotMonotone("radius rises after the crest"));
            }
        }
    }
    Ok(())
}
