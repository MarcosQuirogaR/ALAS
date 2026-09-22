// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The subset of the NASTRAN bulk-data vocabulary the wingbox mesh speaks.
//!
//! The reference builds its deck into a `pyNastran` `BDF` object, which models
//! every card in the format. Nothing here needs that: the mesh emits eleven
//! card types and never reads one back, so this is a plain record of what was
//! added, in the order it was added, and [`super::write`] turns it into the
//! file a solver reads. Keeping it that narrow is what makes the deck's
//! contents reviewable next to `alas/geometry/wing_mesh_bdf.py`'s own calls.
//!
//! Element identifiers are assigned by the builder from one shared counter, so
//! they are carried on the cards rather than implied by position: which element
//! got which identifier is part of what the parity test checks.

use std::collections::HashMap;

/// A `GRID` point: an identifier and its basic-frame coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grid {
    /// Grid identifier.
    pub nid: i64,
    /// Basic-frame `(x, y, z)`, metres.
    pub xyz: [f64; 3],
}

/// A `MAT1` isotropic material.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat1 {
    /// Material identifier.
    pub mid: i64,
    /// Young's modulus, Pa.
    pub e: f64,
    /// Shear modulus, Pa.
    pub g: f64,
    /// Poisson's ratio.
    pub nu: f64,
    /// Density, kg/m^3.
    pub rho: f64,
}

/// A `PSHELL` shell property.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pshell {
    /// Property identifier.
    pub pid: i64,
    /// Membrane material identifier.
    pub mid1: i64,
    /// Thickness, m.
    pub t: f64,
    /// Bending material identifier.
    pub mid2: i64,
}

/// A `PBARL` bar property given by a named cross-section and its dimensions.
#[derive(Debug, Clone, PartialEq)]
pub struct Pbarl {
    /// Property identifier.
    pub pid: i64,
    /// Material identifier.
    pub mid: i64,
    /// Cross-section name, `"I"` for every spar cap this mesh builds.
    pub section: &'static str,
    /// The section's dimensions, in the order the named section expects them.
    pub dim: Vec<f64>,
}

/// A `CQUAD4` or `CTRIA3` shell element.
///
/// One type for both: they differ only in how many grids they name, and the
/// zipper bridging that produces them treats them as the same panel.
#[derive(Debug, Clone, PartialEq)]
pub struct Shell {
    /// Element identifier.
    pub eid: i64,
    /// Property identifier.
    pub pid: i64,
    /// Corner grids, three or four of them.
    pub nodes: Vec<i64>,
}

/// A `CBAR` beam element oriented by an explicit vector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cbar {
    /// Element identifier.
    pub eid: i64,
    /// Property identifier.
    pub pid: i64,
    /// Grid at end A.
    pub ga: i64,
    /// Grid at end B.
    pub gb: i64,
    /// Orientation vector.
    pub x: [f64; 3],
    /// Offset interpretation flag.
    pub offt: &'static str,
}

/// A `CONM2` concentrated mass hung off one grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Conm2 {
    /// Element identifier.
    pub eid: i64,
    /// Grid the mass attaches to.
    pub nid: i64,
    /// Coordinate system the offset is measured in.
    pub cid: i64,
    /// Mass, kg.
    pub mass: f64,
    /// Offset from the grid to the mass centre, m.
    pub offset: [f64; 3],
}

/// An `RBE3` weighted-average rigid element.
///
/// One weight and one component group, which is all the transition-rib rivets
/// ever use; a general `RBE3` may carry several.
#[derive(Debug, Clone, PartialEq)]
pub struct Rbe3 {
    /// Element identifier.
    pub eid: i64,
    /// The dependent grid whose motion is averaged.
    pub refgrid: i64,
    /// Dependent components, as the digit string NASTRAN expects.
    pub refc: &'static str,
    /// Dimensionless interpolation weight on the single independent group.
    pub weight: f64,
    /// Independent components.
    pub comp: &'static str,
    /// The independent grids.
    pub gijs: Vec<i64>,
}

/// An `SPC1` single-point constraint over a list of grids.
#[derive(Debug, Clone, PartialEq)]
pub struct Spc1 {
    /// Constraint set identifier.
    pub sid: i64,
    /// Constrained components, as a digit string.
    pub components: &'static str,
    /// The constrained grids.
    pub nodes: Vec<i64>,
}

/// One `PARAM` value, which NASTRAN allows to be a name, an integer or a real.
#[derive(Debug, Clone, PartialEq)]
pub enum ParamValue {
    /// A character value, such as `YES`.
    Name(&'static str),
    /// An integer value.
    Int(i64),
}

/// A `PARAM` card.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    /// Parameter name.
    pub key: &'static str,
    /// Its value.
    pub value: ParamValue,
}

/// The bulk-data deck the mesh builder fills in.
///
/// Cards are appended in build order and never removed, so the deck is a
/// transcript of what the builder did rather than a model that has to be kept
/// consistent. [`Deck::write_bulk`](super::write) renders it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Deck {
    pub(crate) grids: Vec<Grid>,
    pub(crate) materials: Vec<Mat1>,
    pub(crate) shell_properties: Vec<Pshell>,
    pub(crate) bar_properties: Vec<Pbarl>,
    pub(crate) quads: Vec<Shell>,
    pub(crate) trias: Vec<Shell>,
    pub(crate) bars: Vec<Cbar>,
    pub(crate) masses: Vec<Conm2>,
    pub(crate) rigid_elements: Vec<Rbe3>,
    pub(crate) constraints: Vec<Spc1>,
    pub(crate) params: Vec<Param>,
    /// Grid identifier to its position in `grids`, so a coordinate lookup does
    /// not scan. The builder's own coordinate-keyed deduplication map is
    /// separate and lives with the builder: it is how the deck gets built, not
    /// something the deck itself has to remember.
    positions: HashMap<i64, usize>,
}

impl Deck {
    /// An empty deck.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a `GRID`, returning its identifier.
    pub(crate) fn add_grid(&mut self, nid: i64, xyz: [f64; 3]) -> i64 {
        self.positions.insert(nid, self.grids.len());
        self.grids.push(Grid { nid, xyz });
        nid
    }

    /// The coordinates of grid `nid`, if the deck has one.
    pub fn grid_xyz(&self, nid: i64) -> Option<[f64; 3]> {
        self.positions
            .get(&nid)
            .and_then(|&position| self.grids.get(position))
            .map(|grid| grid.xyz)
    }

    /// The `y` coordinate of grid `nid`: the one component every consumer of
    /// a finished mesh asks for, since the deck is a semi-wing and `y` is span.
    ///
    /// Returns `NaN` for a grid the deck does not have, which no caller can
    /// reach: identifiers come from the same [`super::MeshNodeIndex`] the deck
    /// was built with.
    pub fn node_y(&self, nid: i64) -> f64 {
        self.grid_xyz(nid).map_or(f64::NAN, |xyz| xyz[1])
    }

    /// Every `GRID` in the deck, in the order they were added.
    pub fn grids(&self) -> &[Grid] {
        &self.grids
    }

    /// Every `CQUAD4`, in the order they were added.
    pub fn quads(&self) -> &[Shell] {
        &self.quads
    }

    /// Every `CTRIA3`, in the order they were added.
    pub fn trias(&self) -> &[Shell] {
        &self.trias
    }

    /// The element count NASTRAN reports: shells and bars, but not the
    /// concentrated masses or the rigid elements, which the format counts
    /// separately.
    pub fn element_count(&self) -> usize {
        self.quads.len() + self.trias.len() + self.bars.len()
    }
}
