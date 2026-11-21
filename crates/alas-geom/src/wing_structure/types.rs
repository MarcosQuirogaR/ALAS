// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The data types [`super::WingStructureGeometry`] produces and reports
//! errors through, kept apart from its methods so a reader can see the shape
//! of the result without the arithmetic that builds it.

/// One spanwise rib cross-section, ready for sizing/mesh consumption:
/// `RibStation`.
#[derive(Debug, Clone, PartialEq)]
pub struct RibStation {
    /// Position in the generated station list.
    pub index: usize,
    /// Spanwise fraction of the semispan, `0` at the root and `1` at the tip.
    pub eta: f64,
    /// Spanwise position in metres.
    pub y_station: f64,
    /// `false` for a truncated ("transition") rib near the root.
    ///
    /// Always `true` here: classifying full vs. transition ribs is the mesh
    /// builder's job, which knows the skin-start rib, see
    /// `alas/geometry/wing_mesh_bdf.py`'s own docstring. This field exists
    /// so [`RibStation`] carries the same shape the mesh builder consumes.
    pub is_full: bool,
    /// `L_actual / L_nominal` along the rib's own cut direction.
    pub frac_actual: f64,
    /// Absolute XYZ points on the upper surface, leading edge to (possibly
    /// truncated) trailing edge.
    pub extrados: Vec<[f64; 3]>,
    /// Absolute XYZ points on the lower surface, same ordering as
    /// [`RibStation::extrados`].
    pub intrados: Vec<[f64; 3]>,
    /// Node index into [`RibStation::extrados`]/[`RibStation::intrados`] for
    /// each spar, in the order of the sorted `spar_chord_fractions` the
    /// owning [`super::WingStructureGeometry`] was built with.
    ///
    /// `-1` marks a spar this rib's truncated reach does not extend to,
    /// mirroring the sentinel the Python source and its mesh-building
    /// consumer both use (`j_spars[i] >= 0` gates every read).
    pub j_spars: Vec<i32>,
    /// Unit chordwise direction of this rib's cut, in the XY plane.
    pub rib_dir_xy: (f64, f64),
}

/// A spar's 3-point (root/break/tip) reference line in the XY plane.
///
/// `tip` is `None` for a partial-span spar (`full_span = false`): it
/// physically ends at the break station, so there is no break -> tip segment
/// to define. `WingStructureGeometry::compute_spar_intersections` reads the
/// missing point as "this spar doesn't exist outboard of the break."
///
/// Visible to the rest of `wing_structure` and no further: constructed only
/// by [`super::WingStructureGeometry::new`] and read only by
/// `compute_spar_intersections`, both within this module tree.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct SparReferenceLine {
    pub(super) root: (f64, f64),
    pub(super) break_pt: (f64, f64),
    pub(super) tip: Option<(f64, f64)>,
}

/// Why a [`super::WingStructureGeometry`] could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WingStructureError {
    /// `spar_chord_fractions` was empty; there is nothing to build a wingbox
    /// around.
    #[error("at least one spar_chord_fractions entry is required")]
    NoSpars,
    /// The design vector and wing configuration do not describe a
    /// constructible planform (`WingConfig::transport_planform` failed).
    #[error("the wing planform is not physically constructible")]
    InvalidPlanform,
}

/// Spanwise lift of the main wing as two unit shapes, the classical basic and
/// additional loading (USAF DATCOM 4.1.4; Abbott & von Doenhoff, ch. 8): the lift
/// per unit span fraction of a case with total semi-wing lift `F` is
/// `F * additional(eta) + F_level * basic(eta)`, with `F_level` the level-flight
/// semi-wing lift (the design-point dynamic pressure fixes the basic load).
/// `additional` integrates to one over `eta in [0, 1]` (lift per unit total lift,
/// untwisted), `basic` integrates to zero (the zero-lift load from twist and
/// camber, as a fraction of the level semi-wing lift). Both come from the strip
/// sums of two vortex-lattice solves of the wing alone.
#[derive(Debug, Clone, PartialEq)]
pub struct SpanLoadShape {
    /// Strip-centre span fractions, ascending, in `(0, 1)`.
    pub eta: Vec<f64>,
    /// Additional loading at each strip centre (unit integral).
    pub additional: Vec<f64>,
    /// Basic loading at each strip centre (zero integral).
    pub basic: Vec<f64>,
    /// Where the shape comes from, for labelling reports.
    pub label: &'static str,
}

impl SpanLoadShape {
    /// Build from strip data; additional is normalised to unit integral and
    /// the basic load is made exactly lift-free. `None` for empty, non-finite
    /// or lift-free data.
    pub fn new(
        eta: Vec<f64>,
        additional: Vec<f64>,
        basic: Vec<f64>,
        label: &'static str,
    ) -> Option<Self> {
        let n = eta.len();
        if n < 2 || additional.len() != n || basic.len() != n {
            return None;
        }
        if eta
            .iter()
            .chain(&additional)
            .chain(&basic)
            .any(|v| !v.is_finite())
        {
            return None;
        }
        let mut shape = Self {
            eta,
            additional,
            basic,
            label,
        };
        let total = integral(&shape.nodes(&shape.additional));
        if total <= 0.0 {
            return None;
        }
        for v in &mut shape.additional {
            *v /= total;
        }
        let lift = integral(&shape.nodes(&shape.basic));
        let a = shape.additional.clone();
        for (b, a) in shape.basic.iter_mut().zip(&a) {
            *b -= lift * a;
        }
        Some(shape)
    }

    fn nodes(&self, values: &[f64]) -> Vec<(f64, f64)> {
        let mut nodes = vec![(0.0, values[0])];
        nodes.extend(self.eta.iter().copied().zip(values.iter().copied()));
        nodes.push((1.0, 0.0));
        nodes
    }

    fn at(&self, values: &[f64], eta: f64) -> f64 {
        if !(0.0..=1.0).contains(&eta) {
            return 0.0;
        }
        let nodes = self.nodes(values);
        for p in nodes.windows(2) {
            if eta <= p[1].0 {
                let span = p[1].0 - p[0].0;
                if span <= 0.0 {
                    return p[1].1;
                }
                return p[0].1 + (p[1].1 - p[0].1) * (eta - p[0].0) / span;
            }
        }
        0.0
    }

    /// Additional loading per unit `eta` at `eta`: linear between strip
    /// centres, constant to the root, falling to zero at the tip.
    pub fn density_at(&self, eta: f64) -> f64 {
        self.at(&self.additional, eta)
    }

    /// Basic loading per unit `eta` at `eta`, same interpolation.
    pub fn basic_at(&self, eta: f64) -> f64 {
        self.at(&self.basic, eta)
    }
}

impl SpanLoadShape {
    /// Label of the Schrenk shape.
    pub const SCHRENK_LABEL: &'static str =
        "Schrenk approximation (mean of planform and ellipse), no basic loading";

    /// Schrenk's approximation to the additional loading: the mean of the
    /// planform chord distribution and the ellipse of equal area and span
    /// (Schrenk, "A Simple Approximation Method for Obtaining the Spanwise Lift
    /// Distribution", NACA TM 948, 1940). `chord(eta)` is the local chord; the
    /// basic loading is zero (twist is not represented).
    pub fn schrenk(chord: impl Fn(f64) -> f64) -> Option<Self> {
        const N: usize = 24;
        let eta: Vec<f64> = (0..N).map(|i| (i as f64 + 0.5) / N as f64).collect();
        let c: Vec<f64> = eta.iter().map(|&e| chord(e)).collect();
        let area: f64 = c.iter().sum::<f64>() / N as f64;
        if !(area.is_finite() && area > 0.0) {
            return None;
        }
        let additional: Vec<f64> = eta
            .iter()
            .zip(&c)
            .map(|(&e, &ch)| {
                0.5 * (ch / area + 4.0 / std::f64::consts::PI * (1.0 - e * e).max(0.0).sqrt())
            })
            .collect();
        Self::new(eta, additional, vec![0.0; N], Self::SCHRENK_LABEL)
    }
}

fn integral(nodes: &[(f64, f64)]) -> f64 {
    nodes
        .windows(2)
        .map(|p| 0.5 * (p[0].1 + p[1].1) * (p[1].0 - p[0].0))
        .sum()
}
