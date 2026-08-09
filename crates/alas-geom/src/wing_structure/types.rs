// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The data types [`super::WingStructureGeometry`] produces and reports
//! errors through, kept apart from its methods so a reader can see the shape
//! of the result without the arithmetic that builds it.

/// One spanwise rib cross-section, ready for sizing/mesh consumption --
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
    /// builder's job, which knows the skin-start rib -- see
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
}
