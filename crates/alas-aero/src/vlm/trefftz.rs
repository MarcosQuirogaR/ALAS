// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Far-field (Trefftz-plane) induced drag of a solved vortex lattice.
//!
//! # Why the near-field force is not the induced drag
//!
//! [`super::VlmResult::cd_drag`] is a near-field sum: the Kutta-Joukowski
//! force `rho Gamma V x l` on every bound leg, with `V` the full local
//! velocity at the leg's midpoint. In two dimensions the mutual forces of the
//! bound vortices cancel in pairs, whatever their positions, so the sum is
//! exact. In three dimensions two finite segments that are not collinear do
//! not exert equal and opposite forces on each other. On a swept, cambered or
//! dihedralled lattice the bound legs of one strip sit at different heights
//! and angles from the next, and the unbalanced remainder appears as a
//! streamwise force that is not induced drag. This can produce a negative
//! drag near zero lift or an apparent span efficiency above the planar
//! elliptic bound. Wake energy avoids that force-summation error, while a
//! conservative strip-average reconstruction avoids losing lift during its
//! interpolation to the continuous sheet.
//!
//! # What replaces it
//!
//! Far downstream the wake no longer interacts with the lifting surfaces,
//! and the induced drag is the kinetic energy it leaves per unit length
//! (Munk, *The Minimum Induced Drag of Aerofoils*, NACA Report 121, 1923;
//! Katz and Plotkin, *Low-Speed Aerodynamics*, 2nd ed., 2001, section 8.3;
//! Drela, *Flight Vehicle Aerodynamics*, MIT Press, 2014, section 5.5):
//!
//! `D_i = -(rho / (4 pi)) iint gamma(s) gamma(s') ln|r(s) - r(s')| ds ds'`,
//!
//! where `gamma = -dGamma/ds` is the streamwise vorticity of the wake sheet
//! in the Trefftz plane and `s` runs along the sheet's trace. For a sheet
//! whose circulation vanishes at its free edges the form is positive
//! definite, so the drag is never negative, and among all loadings of a
//! planar sheet of span `b` the elliptic one minimises it at
//! `L^2 / (pi q b^2)`.
//!
//! # Discretisation
//!
//! The lattice wake leaves every horseshoe leg along the body `x` axis
//! (`TRAILING_VORTEX_DIRECTION`), and in linear theory the wake lies in the
//! surface it leaves, so the Trefftz plane sees each surface's sheet along
//! the surface's spanwise trace. That trace is taken through the leading
//! edge of every strip, at its body `(y, z)`. Each section is rotated about
//! an axis through its own leading edge, so the leading-edge line does not
//! move with section incidence or twist: a trailing-edge trace would drop
//! with every degree of stabiliser incidence (by `c sin(i)`, a second-order
//! displacement linear theory neglects), making the drag of the one linear
//! circulation depend on the trim, and on a dihedralled wing it would part
//! the two root sections across the plane of symmetry (0.26 m on the
//! A380-800 at its registered root incidence). A strip's circulation is the
//! sum over its chordwise panels, whose stacked legs leave the same strip
//! edges. Strips that share a leading-edge corner are chained into one sheet
//! per surface (a symmetric wing is one sheet from tip to tip). Along a
//! chain the circulation is
//! reconstructed linearly in arc length between strip midpoints and taken to
//! zero at the chain's free ends. The midpoint values solve a tridiagonal
//! strip-average system: each reconstructed strip mean equals the lattice's
//! strip circulation. This conserves integrated circulation and planar linear
//! lift without distorting the loading at nonuniform mesh transitions.
//! Unconstrained hats lose circulation at the free ends and on nonuniform
//! meshes, giving a coarse elliptic wing an apparent `e > 1`.
//! The sheet carries piecewise-constant vorticity on
//! straight segments. Its energy is evaluated
//! with the inner integral in closed form and an eight-point Gauss-Legendre
//! outer quadrature, graded towards segment ends that touch another segment;
//! a segment's own term is exact. Being the energy of a physical loading,
//! the drag is never negative. Span efficiency converges to the elliptic
//! bound with mesh refinement. The remaining difference between efficiencies
//! formed with conserved linear lift and near-field lift is the latter's
//! nonlinear force projection, beyond this linear wake.
//!
//! Linear-theory assumptions, stated rather than hidden: no wake roll-up or
//! deflection, so the trace is the surface's own line; the drag is taken
//! along the freestream although the wake leaves along body `x`, a
//! difference of order `alpha^2`; and the Prandtl-Glauert stretching is
//! streamwise, so compressibility enters through the solved circulation
//! only.

use std::f64::consts::PI;

use super::VlmResult;
use crate::operating_point::OperatingPoint;

mod quadrature;
use quadrature::{pair_log_integral, self_log_integral};

/// Two leading-edge corners of one surface closer than this fraction of the
/// narrower of their strips are the same corner. Neighbouring strips share
/// their mesh corners exactly, so the tolerance only absorbs round-off in a
/// mirrored half; five percent of a strip cannot join two strips that are
/// not neighbours, whose corners lie a whole strip apart.
const CORNER_MATCH_FRACTION: f64 = 0.05;

/// One straight piece of a wake trace with constant vorticity per unit
/// length.
struct Segment {
    start: [f64; 2],
    end: [f64; 2],
    /// Vorticity per unit length, `-dGamma/ds`, as `(strip, weight)` terms:
    /// the sum of each weight (1/m) times that strip's circulation.
    gamma: Vec<(usize, f64)>,
}

/// One lattice strip seen in the Trefftz plane.
struct Strip {
    left: [f64; 2],
    right: [f64; 2],
    wing_index: usize,
}

/// The Trefftz-plane drag of one lattice geometry as a quadratic form in
/// its strip circulations, `D = rho Gamma^T Q Gamma`.
///
/// `Q` depends on the leading-edge geometry alone, so a lattice solved at
/// many operating points (a polar sweep) builds it once and each solve
/// costs one `n_strips^2` product.
#[derive(Debug, Clone)]
pub struct TrefftzOperator {
    /// One past the last panel of each strip, in panel order.
    strip_ends: Vec<usize>,
    /// Row-major `n_strips x n_strips` form, dimensionless (drag per
    /// `rho Gamma^2`).
    form: Vec<f64>,
}

impl TrefftzOperator {
    /// Build the form from the panels of a solved lattice
    /// ([`VlmResult::panels`]).
    ///
    /// Returns `None` when the panels do not group into strips: each
    /// wing's panels run chordwise within a strip, and a strip's last panel
    /// is its trailing-edge panel. Degenerate strips, branches and closed
    /// wake traces are unsupported and return `None`.
    pub fn from_panels(panels: &[super::PanelSample]) -> Option<Self> {
        let mut strips = Vec::new();
        let mut strip_ends = Vec::new();
        let mut strip_start = 0;
        for (index, panel) in panels.iter().enumerate() {
            if panel.wing_index != panels[strip_start].wing_index {
                return None;
            }
            if panel.is_trailing_edge {
                let leading = &panels[strip_start];
                strips.push(Strip {
                    left: [leading.front_left[1], leading.front_left[2]],
                    right: [leading.front_right[1], leading.front_right[2]],
                    wing_index: panel.wing_index,
                });
                strip_ends.push(index + 1);
                strip_start = index + 1;
            }
        }
        if strips.is_empty() || strip_ends.last() != Some(&panels.len()) {
            return None;
        }
        if strips.iter().any(|strip| {
            let width = length(strip.left, strip.right);
            !width.is_finite() || width <= 0.0
        }) {
            return None;
        }
        let (segments, mapping) = wake_segments(&strips)?;
        let n = strips.len();
        let mut form = vec![0.0; n * n];
        for (i, a) in segments.iter().enumerate() {
            for (j, b) in segments.iter().enumerate().skip(i) {
                let log_integral = if i == j {
                    self_log_integral(length(a.start, a.end))
                } else {
                    // The pair (j, i) has the same value; count it here.
                    2.0 * pair_log_integral(a, b)
                };
                let factor = -log_integral / (4.0 * PI);
                for &(p, wp) in &a.gamma {
                    for &(q, wq) in &b.gamma {
                        form[p * n + q] += factor * wp * wq;
                    }
                }
            }
        }
        // The form above acts on midpoint values. The strip-average map T
        // converts lattice strip means into those values: Q = T^T Q_hat T.
        let mut product = vec![0.0; n * n];
        for p in 0..n {
            for q in 0..n {
                product[p * n + q] = (0..n).map(|r| form[p * n + r] * mapping[r * n + q]).sum();
            }
        }
        for p in 0..n {
            for q in 0..n {
                form[p * n + q] = (0..n)
                    .map(|r| mapping[r * n + p] * product[r * n + q])
                    .sum();
            }
        }
        form.iter()
            .all(|value| value.is_finite())
            .then_some(Self { strip_ends, form })
    }

    /// The induced-drag coefficient of `result`, which must have been solved
    /// on the lattice this operator was built from, referenced to `s_ref`
    /// (m^2) and the dynamic pressure of `op_point`.
    pub fn induced_drag_coefficient(
        &self,
        result: &VlmResult,
        op_point: &OperatingPoint,
        s_ref: f64,
    ) -> Option<f64> {
        let q = op_point.dynamic_pressure();
        if !(q.is_finite() && q > 0.0 && s_ref.is_finite() && s_ref > 0.0)
            || self.strip_ends.last() != Some(&result.vortex_strengths.len())
        {
            return None;
        }
        let mut start = 0;
        let circulation: Vec<f64> = self
            .strip_ends
            .iter()
            .map(|&end| {
                let sum = result.vortex_strengths[start..end].iter().sum();
                start = end;
                sum
            })
            .collect();
        let n = circulation.len();
        let quadratic: f64 = (0..n)
            .map(|p| {
                circulation[p]
                    * (0..n)
                        .map(|q| self.form[p * n + q] * circulation[q])
                        .sum::<f64>()
            })
            .sum();
        let drag = op_point.atmosphere.density() * quadratic;
        let coefficient = drag / (q * s_ref);
        (coefficient.is_finite() && coefficient >= 0.0).then_some(coefficient)
    }
}

/// The Trefftz-plane induced-drag coefficient of `result`, referenced to
/// `s_ref` (m^2) and the dynamic pressure of `op_point`.
///
/// Returns `None` when the panels do not group into strips (a result whose
/// panel list does not end each strip with its trailing-edge panel) or the
/// reference quantities are not positive and finite.
pub fn trefftz_induced_drag_coefficient(
    result: &VlmResult,
    op_point: &OperatingPoint,
    s_ref: f64,
) -> Option<f64> {
    TrefftzOperator::from_panels(&result.panels)?.induced_drag_coefficient(result, op_point, s_ref)
}

/// Chain the strips of each surface through shared leading-edge corners and
/// lay the linearly interpolated circulation along each chain as constant-
/// vorticity segments.
fn wake_segments(strips: &[Strip]) -> Option<(Vec<Segment>, Vec<f64>)> {
    // successor[k]: the strip of the same surface whose left corner is
    // strip k's right corner, the nearest one within the matching tolerance.
    let successor: Vec<Option<usize>> = strips
        .iter()
        .enumerate()
        .map(|(k, strip)| {
            let width = length(strip.left, strip.right);
            strips
                .iter()
                .enumerate()
                .filter(|&(j, other)| j != k && other.wing_index == strip.wing_index)
                .map(|(j, other)| (j, other, length(strip.right, other.left)))
                .filter(|&(_, other, gap)| {
                    gap <= CORNER_MATCH_FRACTION * width.min(length(other.left, other.right))
                })
                .min_by(|a, b| a.2.total_cmp(&b.2))
                .map(|(j, _, _)| j)
        })
        .collect();
    let mut has_predecessor = vec![false; strips.len()];
    for next in successor.iter().flatten() {
        if has_predecessor[*next] {
            return None;
        }
        has_predecessor[*next] = true;
    }

    let mut chains = Vec::new();
    let mut visited = vec![false; strips.len()];
    let starts = (0..strips.len()).filter(|&k| !has_predecessor[k]);
    // A closed loop has no free ends; reject it rather than silently omitting
    // its energy from the drag of the other surfaces.
    for start in starts {
        let mut chain = Vec::new();
        let mut current = Some(start);
        while let Some(k) = current {
            if visited[k] {
                break;
            }
            visited[k] = true;
            chain.push(k);
            current = successor[k];
        }
        chains.push(chain);
    }

    if visited.iter().any(|&seen| !seen) {
        return None;
    }
    let mut segments = Vec::new();
    let n = strips.len();
    let mut mapping = vec![0.0; n * n];
    for chain in &chains {
        push_chain_segments(strips, chain, &mut segments);
        strip_average_mapping(strips, chain, &mut mapping)?;
    }
    Some((segments, mapping))
}

/// Map strip means to continuous midpoint values by matching the integral
/// over each strip. The tridiagonal strip-average matrix is diagonally
/// dominant; its geometry-only factorization is reused for every unit column.
fn strip_average_mapping(strips: &[Strip], chain: &[usize], mapping: &mut [f64]) -> Option<()> {
    let count = chain.len();
    let widths: Vec<f64> = chain
        .iter()
        .map(|&k| length(strips[k].left, strips[k].right))
        .collect();
    let mut diagonal = vec![0.5; count];
    let mut lower = vec![0.0; count];
    let mut upper = vec![0.0; count];
    for i in 0..count {
        let width = widths[i];
        if i > 0 {
            let previous = widths[i - 1];
            diagonal[i] += previous / (4.0 * (previous + width));
            lower[i] = width / (4.0 * (previous + width));
        }
        if i + 1 < count {
            let next = widths[i + 1];
            diagonal[i] += next / (4.0 * (width + next));
            upper[i] = width / (4.0 * (width + next));
        }
    }
    for i in 1..count {
        lower[i] /= diagonal[i - 1];
        diagonal[i] -= lower[i] * upper[i - 1];
    }
    if diagonal
        .iter()
        .any(|&pivot| !pivot.is_finite() || pivot <= 0.0)
    {
        return None;
    }
    for source in 0..count {
        let mut values = vec![0.0; count];
        values[source] = 1.0;
        for i in 1..count {
            values[i] -= lower[i] * values[i - 1];
        }
        values[count - 1] /= diagonal[count - 1];
        for i in (0..count - 1).rev() {
            values[i] = (values[i] - upper[i] * values[i + 1]) / diagonal[i];
        }
        for (i, &target) in chain.iter().enumerate() {
            mapping[target * strips.len() + chain[source]] = values[i];
        }
    }
    Some(())
}

/// Append the segments of one chain: zero circulation at its free corners,
/// midpoint hats, linear in arc length through shared corners. The operator
/// applies the conservative strip-average map after forming their energy.
fn push_chain_segments(strips: &[Strip], chain: &[usize], segments: &mut Vec<Segment>) {
    // Knots along the trace: (point, circulation, is a strip midpoint).
    // `Some(None)` is a free end (zero circulation), `Some(Some(k))` strip
    // k's circulation at its midpoint, `None` a corner without a value.
    let mut knots: Vec<([f64; 2], Option<Option<usize>>)> = Vec::with_capacity(2 * chain.len() + 1);
    for (position, &k) in chain.iter().enumerate() {
        let strip = &strips[k];
        if position == 0 {
            knots.push((strip.left, Some(None)));
        }
        knots.push((midpoint(strip.left, strip.right), Some(Some(k))));
        let last = position + 1 == chain.len();
        knots.push((strip.right, last.then_some(None)));
    }
    // Between consecutive knots that carry a circulation the value is linear
    // in arc length; corners in between (value `None`) only bend the trace.
    let mut anchor = 0;
    while anchor + 1 < knots.len() {
        let Some(next) = (anchor + 1..knots.len()).find(|&i| knots[i].1.is_some()) else {
            break;
        };
        let arc: f64 = (anchor..next)
            .map(|i| length(knots[i].0, knots[i + 1].0))
            .sum();
        if arc > 0.0 {
            let (Some(from), Some(to)) = (knots[anchor].1, knots[next].1) else {
                break;
            };
            let mut gamma = Vec::with_capacity(2);
            if let Some(k) = to {
                gamma.push((k, -1.0 / arc));
            }
            if let Some(k) = from {
                gamma.push((k, 1.0 / arc));
            }
            for i in anchor..next {
                segments.push(Segment {
                    start: knots[i].0,
                    end: knots[i + 1].0,
                    gamma: gamma.clone(),
                });
            }
        }
        anchor = next;
    }
}

fn length(a: [f64; 2], b: [f64; 2]) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

fn midpoint(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    lerp(a, b, 0.5)
}

fn lerp(a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
    [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]
}

// A test asserts on geometry it constructed directly, so a failed unwrap is
// the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests;
