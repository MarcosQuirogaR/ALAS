// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use std::f64::consts::PI;

use super::airfoil::Airfoil;
use super::spacing::{cosspace, linspace};
use super::vector3::{
    add3, blend3, cross3, dot3, matvec3, norm3, project_to_yz_and_normalize, rotation_matrix_3d,
    scale3, sub3,
};

/// `blend_with_another_airfoil`'s `n_points_per_side`, upstream's default and
/// the only value [`Wing::subdivide_sections`] ever calls it with.
pub(super) const SUBDIVIDE_BLEND_N_POINTS_PER_SIDE: usize = 100;

/// A wing cross-section: leading-edge position, chord, twist and airfoil --
/// `WingXSec`, scoped to the fields this program uses (no control surfaces,
/// no analysis-specific options; see the module doc).
#[derive(Debug, Clone, PartialEq)]
pub struct WingXSec {
    /// Leading-edge coordinates, in geometry axes.
    pub xyz_le: [f64; 3],
    /// Chord at this cross-section.
    pub chord: f64,
    /// Twist angle in degrees, about the leading edge. Mutated in place by
    /// the trim phase (a later module); an ordinary public field, not a
    /// derived quantity.
    pub twist: f64,
    /// The airfoil section at this cross-section.
    pub airfoil: Airfoil,
}

impl WingXSec {
    /// A new cross-section at `xyz_le`, `chord`, `twist` (degrees) and
    /// `airfoil`.
    pub fn new(xyz_le: [f64; 3], chord: f64, twist: f64, airfoil: Airfoil) -> Self {
        Self {
            xyz_le,
            chord,
            twist,
            airfoil,
        }
    }

    /// A copy of this cross-section translated by `xyz`.
    pub fn translate(&self, xyz: [f64; 3]) -> Self {
        Self {
            xyz_le: add3(self.xyz_le, xyz),
            ..self.clone()
        }
    }
}

/// A wing: a name, an ordered list of cross-sections, and whether it is
/// mirrored about the XZ plane -- `Wing`, scoped to the fields and methods
/// this program uses (see the module doc).
#[derive(Debug, Clone, PartialEq)]
pub struct Wing {
    /// The wing's name, e.g. `"Main Wing"`.
    pub name: String,
    /// Cross-sections from root to tip. Lofted linearly between adjacent
    /// pairs.
    pub xsecs: Vec<WingXSec>,
    /// Whether the wing is mirrored about the XZ plane. If `true`, every
    /// quantity this module computes accounts for both halves.
    pub symmetric: bool,
}

/// Which spacing function [`Wing::subdivide_sections`] uses to place the new
/// cross-sections along each lofted interval -- upstream's pluggable
/// `spacing_function: Callable[[float, float, int], np.ndarray]`, narrowed to
/// the two functions this program's inputs ever name. See the module doc for
/// which caller reaches which variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpacingFunction {
    /// Evenly spaced -- `np.linspace`, `alas-geom::builder`'s choice (its
    /// only caller never names an explicit one, so this is upstream's
    /// default).
    Linspace,
    /// Bunched near both ends of each interval -- `np.cosspace`,
    /// `VortexLatticeMethod.run()`'s `spanwise_spacing_function` default.
    Cosspace,
}

impl SpacingFunction {
    /// `num` points from `start` to `stop` under this spacing.
    pub(super) fn spaced(self, start: f64, stop: f64, num: usize) -> Vec<f64> {
        match self {
            Self::Linspace => linspace(start, stop, num),
            Self::Cosspace => cosspace(start, stop, num),
        }
    }
}

/// [`Wing::subdivide_sections`] rejects a ratio less than 2 -- the same
/// condition upstream's `raise ValueError("`ratio` must be an integer
/// greater than or equal to 2.")` guards, restated as a typed error since
/// this crate does not panic (`CONTRIBUTING.md`). The "integer" half of
/// upstream's check has no counterpart here: `ratio`'s type already
/// guarantees that.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum SubdivideSectionsError {
    /// `ratio` was less than 2.
    #[error("`ratio` must be greater than or equal to 2, got {0}")]
    RatioTooSmall(usize),
    /// Blending two distinct airfoils at a subdivision boundary failed to
    /// repanel -- see [`Airfoil::blend_with_another_airfoil`].
    #[error(transparent)]
    Blend(#[from] alas_math::CubicSplineError),
}
