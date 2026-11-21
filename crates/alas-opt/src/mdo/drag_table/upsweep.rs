// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The rear-fuselage upsweep drag at the trimmed body attitude.
//!
//! The parasite build-up charges the upsweep at body angle of attack 0
//! (Raymer, 6th ed., Eq. 12.36). The penalty Torenbeek (*Synthesis of
//! Subsonic Airplane Design*, 1982, Fig. 3-27b) measured on an isolated
//! fuselage falls steeply as the nose rises (at 10 deg upsweep, 0.046 of the
//! frontal area at alpha 0 and 0.011 at alpha 5), so a trimmed polar
//! evaluates it at the body attitude its own trim solves fly: each induced node carries the angle of attack it was trimmed at, and
//! the attitude between nodes is a monotone cubic in CL (Fritsch and
//! Carlson, *Monotone Piecewise Cubic Interpolation*, SIAM J. Numer. Anal.
//! 17, 1980), so the polar keeps a continuous slope at the nodes. The
//! incompressible and cruise node sets are blended in Mach as the induced
//! drag is. Without two nodes the attitude is the design trim's,
//! `alpha_trim + (CL - CL_trim) / CL_alpha`.
//!
//! The increment is the upsweep column (which carries the viscous margin)
//! times `F(beta, alpha_a) / F(beta, 0) - 1`, `F` the frontal-area
//! coefficient of [`upsweep_frontal_cd`] and `alpha_a = m (alpha - eps0 - k
//! CL)` the afterbody's flow angle behind the wing (USAF DATCOM 1978, Sec.
//! 4.2.2.1, Method 1, printed pp. 4.2.2.1-1 and -2, Sketch (a); see
//! [`alas_aero::analysis::AfterbodyFlow`]); it is zero without an upsweep
//! and negative whenever the afterbody flies nose up. The body's own
//! profile-drag rise with attitude is not added here: it is inside the
//! lift-dependent viscous term `kappa CD0 CL^2`, which Jenkinson (*Civil Jet
//! Aircraft Design*, 1999, pp. 179-180) makes proportional to the profile
//! drag of the complete aircraft, upsweep included, with its factor fitted
//! on complete jet transports.
//!
//! Where the relief and that factor overlap is limited. A fit on jet
//! transports holds whatever their afterbodies do with attitude. At cruise
//! Mach a wing-mounted-engine jet's afterbody flies at or below zero flow
//! angle over the trimmed clean polar (the wing downwash at the body
//! trailing edge exceeds the body attitude), so this increment is zero
//! there. It is not zero everywhere on a jet: at Mach 0.5 the flow angle
//! turns positive above a clean CL of about 0.8 to 1.0, and the small
//! business jet with its rear-mounted engines carries it at every CL, as
//! the steep turboprop and freighter afterbodies do. On those parts of the
//! polar the relief stands beside the fitted factor without a split of the
//! two from the source; the turboprop and freighter afterbodies lie outside
//! the fit's sample, and nothing else in the polar relieves them.
//!
//! The high-lift base polars carry it at the attitude of the deployed
//! configuration, the clean attitude of `CL - Delta CL`, and at the downwash
//! of the full `CL`: the lift of the deployed devices turns the flow down
//! too. `Delta CL`, the lift the devices add at constant angle of attack, is
//! an assumption ([`high_lift_lift_shift`]).

use alas_aero::analysis::{upsweep_frontal_cd, AfterbodyFlow, ParasiteBreakdown, UPSWEEP_ITEM};

use super::grid::hermite;
use super::{DesignTrim, TrimmedDragTable};

/// The upsweep a table re-evaluates at its trimmed attitude.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
pub(super) struct UpsweepAttitude {
    /// Index of the upsweep column among the table's drag areas.
    pub(super) column: usize,
    /// Centreline upsweep the column was read at, rad.
    pub(super) upsweep_rad: f64,
    /// The afterbody's flow angle behind the wing.
    pub(super) flow: AfterbodyFlow,
}

/// Lift the deployed high-lift system adds at constant angle of attack,
/// taken as its maximum-lift increment `CL_max,config - CL_max,clean` (not
/// below zero). An assumption, biased both ways: leading-edge slats raise the
/// stall angle while adding little lift at a fixed angle, so with slats the
/// shift is overstated and the attitude, and the upsweep relief it brings,
/// understated; trailing-edge flaps alone lower the stall angle, so their
/// lift at a fixed angle exceeds the `CL_max` increment, the shift is
/// understated and the relief overstated.
pub fn high_lift_lift_shift(cl_max_config: f64, cl_max_clean: f64) -> f64 {
    (cl_max_config - cl_max_clean).max(0.0)
}

impl UpsweepAttitude {
    /// The upsweep column of `breakdown`, if it has one, its afterbody in
    /// `flow`.
    pub(super) fn of(breakdown: &ParasiteBreakdown, flow: AfterbodyFlow) -> Option<Self> {
        breakdown
            .drag_areas
            .iter()
            .position(|item| item.name == UPSWEEP_ITEM)
            .map(|column| Self {
                column,
                upsweep_rad: breakdown.upsweep_rad,
                flow,
            })
    }
}

/// `(alpha_trim deg, CL_trim, CL_alpha per deg)` of the design trim, on the
/// stretched lattice when the design point has one.
pub(super) fn design_attitude(design: &DesignTrim) -> (f64, f64, f64) {
    if design.stretched.is_some() {
        let view = design.trim.attitude();
        (view.alpha_deg, design.cl, view.cl_alpha)
    } else {
        (design.trim.trim_alpha_deg, design.cl, design.trim.cl_alpha)
    }
}

/// Monotone cubic through `(CL, alpha)` nodes ascending in CL: the value and
/// CL slope at `cl`, continued along the end tangents outside the nodes.
/// Interior tangents are Fritsch and Carlson's weighted harmonic means of
/// the neighbouring secants (zero across a sign change), end tangents their
/// three-point estimate limited to keep the cubic monotone. `None` with
/// fewer than two nodes or a non-finite result.
pub(super) fn monotone_attitude(nodes: &[(f64, f64)], cl: f64) -> Option<(f64, f64)> {
    let n = nodes.len();
    if n < 2 {
        return None;
    }
    let width = |k: usize| nodes[k + 1].0 - nodes[k].0;
    let secant = |k: usize| (nodes[k + 1].1 - nodes[k].1) / width(k);
    let end = |h0: f64, h1: f64, d0: f64, d1: f64| {
        let d = ((2.0 * h0 + h1) * d0 - h0 * d1) / (h0 + h1);
        if d * d0 <= 0.0 {
            0.0
        } else if d0 * d1 < 0.0 && d.abs() > 3.0 * d0.abs() {
            3.0 * d0
        } else {
            d
        }
    };
    let tangent = |k: usize| -> f64 {
        if n == 2 {
            secant(0)
        } else if k == 0 {
            end(width(0), width(1), secant(0), secant(1))
        } else if k == n - 1 {
            end(width(n - 2), width(n - 3), secant(n - 2), secant(n - 3))
        } else {
            let (d0, d1) = (secant(k - 1), secant(k));
            if d0 * d1 <= 0.0 {
                0.0
            } else {
                let (w0, w1) = (2.0 * width(k) + width(k - 1), width(k) + 2.0 * width(k - 1));
                (w0 + w1) / (w0 / d0 + w1 / d1)
            }
        }
    };
    let result = if cl <= nodes[0].0 {
        let slope = tangent(0);
        (nodes[0].1 + (cl - nodes[0].0) * slope, slope)
    } else if cl >= nodes[n - 1].0 {
        let slope = tangent(n - 1);
        (nodes[n - 1].1 + (cl - nodes[n - 1].0) * slope, slope)
    } else {
        let k = nodes[1..n - 1].partition_point(|&(node, _)| node <= cl);
        let (left, right) = (nodes[k], nodes[k + 1]);
        hermite(
            left.0,
            right.0,
            (left.1, tangent(k)),
            (right.1, tangent(k + 1)),
            cl,
        )
    };
    (result.0.is_finite() && result.1.is_finite()).then_some(result)
}

impl TrimmedDragTable {
    /// Trimmed body angle of attack at `cl` and `mach`, deg, and its CL
    /// slope.
    pub fn body_alpha_and_slope(&self, cl: f64, mach: f64) -> (f64, f64) {
        let cruise = self.induced.alpha_and_slope(cl);
        let blended = match (&self.blend, cruise) {
            (Some(blend), Some((a_c, s_c))) => {
                blend.incompressible.alpha_and_slope(cl).map(|(a_i, s_i)| {
                    let w = blend.weight(mach);
                    (a_i + w * (a_c - a_i), s_i + w * (s_c - s_i))
                })
            }
            (None, cruise) => cruise,
            (Some(_), None) => None,
        };
        blended.unwrap_or_else(|| {
            let (alpha, cl_trim, cl_alpha) = self.design_attitude;
            (alpha + (cl - cl_trim) / cl_alpha, 1.0 / cl_alpha)
        })
    }

    /// Change of the upsweep drag from its alpha 0 value at the trimmed body
    /// attitude of `cl` and `mach`, and its CL slope.
    pub(super) fn upsweep_attitude_value_and_slope(&self, cl: f64, mach: f64) -> (f64, f64) {
        self.upsweep_value_and_slope_shifted(cl, mach, 0.0)
    }

    /// [`Self::upsweep_attitude_value_and_slope`] of a configuration whose
    /// devices add `flap_lift_cl` at constant angle of attack: the body flies
    /// the clean attitude of `cl - flap_lift_cl`, the afterbody the downwash
    /// of the full `cl`.
    fn upsweep_value_and_slope_shifted(&self, cl: f64, mach: f64, flap_lift_cl: f64) -> (f64, f64) {
        let Some(upsweep) = self.upsweep else {
            return (0.0, 0.0);
        };
        let (at_zero, _) = upsweep_frontal_cd(upsweep.upsweep_rad, 0.0);
        if at_zero <= 0.0 {
            return (0.0, 0.0);
        }
        let column = self.drag_area_cd(upsweep.column, mach);
        let (alpha, alpha_slope) = self.body_alpha_and_slope(cl - flap_lift_cl, mach);
        let flow = upsweep.flow;
        let (front, per_deg) = upsweep_frontal_cd(upsweep.upsweep_rad, flow.alpha_deg(alpha, cl));
        let afterbody_slope = flow.ramp_mean * (alpha_slope - flow.eps_per_cl_deg);
        (
            column * (front / at_zero - 1.0),
            column * per_deg / at_zero * afterbody_slope,
        )
    }

    /// [`Self::upsweep_attitude_value_and_slope`]'s value: the CL-dependent
    /// upsweep term every clean drag sum of the table adds to its zero-lift
    /// parasite drag.
    pub fn upsweep_attitude_cd(&self, cl: f64, mach: f64) -> f64 {
        self.upsweep_attitude_value_and_slope(cl, mach).0
    }

    /// The upsweep term of a high-lift configuration whose devices add
    /// `flap_lift_cl` at constant angle of attack
    /// ([`Self::upsweep_value_and_slope_shifted`]).
    pub fn upsweep_attitude_cd_high_lift(&self, cl: f64, mach: f64, flap_lift_cl: f64) -> f64 {
        self.upsweep_value_and_slope_shifted(cl, mach, flap_lift_cl)
            .0
    }

    /// The upsweep column at `mach` (its alpha 0 share of `CD0`), zero
    /// without an upsweep.
    pub fn upsweep_cd0(&self, mach: f64) -> f64 {
        self.upsweep
            .map_or(0.0, |upsweep| self.drag_area_cd(upsweep.column, mach))
    }
}
