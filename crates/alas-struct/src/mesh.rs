// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/geometry/wing_mesh_bdf.py
// Reference: alas @ rust-port-baseline.

//! The wingbox finite-element mesh: the NASTRAN deck a structural solve reads.
//!
//! [`build_wing_mesh_bdf`] turns a sized wingbox into grids, skin and rib
//! shells, spar webs, tapered cap bars, a root constraint, engine point masses
//! and the rivets that tie it together. **The mesh is not simple, and two of
//! its mechanisms are load-bearing rather than incidental**, both are ported
//! as they stand, because the reference's own docstring records that they exist
//! to fix a specific failure:
//!
//! 1. *Zipper bridging.* Ribs are cosine-sampled and the root-adjacent ones are
//!    truncated by the root plane, so two adjacent ribs generally do not carry
//!    the same number of chordwise points. Building skin panels rib-to-rib on
//!    such a mesh leaves gaps. Where the counts differ, the shorter rib's last
//!    grid becomes a shared pivot and the surplus panels close with triangles,
//!    so the skin never has a hole regardless of the mismatch.
//! 2. *Transition-rib rivets.* A rib the root plane truncated is tied to the
//!    three nearest full-length skin grids by a weighted-average rigid element.
//!    Without them a physically shorter rib floats free of the skin around it
//!    instead of deforming with it.
//!
//! Five geometric health checks come back in a [`MeshHealthReport`] rather than
//! being printed: rib-to-leading-edge perpendicularity, shell warping,
//! degenerate triangles, spar straightness, and that no grid sits inboard of
//! the root plane. The severities are the reference's own: a degenerate
//! triangle or a grid at negative span is a corrupt mesh and comes back as a
//! [`MeshError`], while the other three are warnings a caller reports.
//!
//! What this module does *not* do is model the NASTRAN format in general. The
//! reference assembles its deck into a `pyNastran` `BDF`; [`Deck`] is instead
//! the eleven card types this mesh emits and nothing else, which is what lets
//! the deck be read next to the calls that built it.

mod build;
mod cards;
mod elements;
mod health;
mod nodes;
mod rivets;
mod write;

pub use build::build_wing_mesh_bdf;
pub use cards::{
    Cbar, Conm2, Deck, Grid, Mat1, Param, ParamValue, Pbarl, Pshell, Rbe3, Shell, Spc1,
};

/// The shell warping coefficient above which a panel is reported as badly
/// non-planar: `WARPING_THRESHOLD`.
pub const WARPING_THRESHOLD: f64 = 0.05;

/// Span below which a grid is treated as part of the constrained root and kept
/// out of the rivets' independent-grid pool, metres: `_Y_ROOT_EXCL`.
const Y_ROOT_EXCL: f64 = 0.01;

/// How much thinner a secondary (truncated) rib is than a main one:
/// `_SEC_RIB_THICKNESS_FACTOR`.
const SEC_RIB_THICKNESS_FACTOR: f64 = 0.5;

/// The grid identifiers a solve's load and monitor cards need.
///
/// Returned from the same pass that built the mesh rather than recovered later
/// by re-reading a written deck, which is what the reference scripts this
/// module generalizes had to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeshNodeIndex {
    /// Front-spar upper grid at the root.
    pub root_nid: i64,
    /// Front-spar upper grid at the tip.
    pub tip_nid: i64,
    /// Front-spar upper grid nearest the planform break.
    pub kink_nid: i64,
    /// Per spar, the upper-surface grids root to tip.
    pub spar_upper_nids: Vec<Vec<i64>>,
    /// Per spar, the lower-surface grids, paired with
    /// [`MeshNodeIndex::spar_upper_nids`] by position.
    pub spar_lower_nids: Vec<Vec<i64>>,
    /// The grid each wing-mounted engine's mass hangs from.
    pub engine_nids: Vec<i64>,
}

/// What the five geometric health checks found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeshHealthReport {
    /// Grids in the deck.
    pub n_nodes: usize,
    /// Shell and bar elements in the deck.
    pub n_elements: usize,
    /// Ribs whose realized cut is not perpendicular to the local leading edge.
    pub n_perp_warnings: usize,
    /// Shells above [`WARPING_THRESHOLD`].
    pub n_warping_bad: usize,
    /// Largest shell warping coefficient.
    pub warping_max: f64,
    /// Mean shell warping coefficient.
    pub warping_mean: f64,
    /// Quadrilateral shells.
    pub n_cquad4: usize,
    /// Triangular shells.
    pub n_ctria3: usize,
    /// Triangles as a fraction of all shells, or zero when there are none.
    pub triangle_ratio: f64,
    /// Spars whose upper line deviates a millimetre or more from straight.
    pub n_spar_straightness_warnings: usize,
    /// Per spar, its chord fraction and the largest deviation found, metres.
    ///
    /// A spar with fewer than three grids is absent rather than zero: there is
    /// no interior point whose distance from the end-to-end chord could be
    /// measured, so the check has nothing to say about it.
    pub spar_straightness_max_dev_m: Vec<(f64, f64)>,
    /// Rivets written onto transition ribs.
    pub rbe3_count: usize,
    /// Every non-fatal finding, in the order it was made.
    pub warnings: Vec<String>,
}

impl MeshHealthReport {
    /// Whether the mesh is free of the one defect this module exists to
    /// prevent: badly warped, non-planar skin panels.
    ///
    /// Perpendicularity and straightness are reported but do not gate this,
    /// which is the reference scripts' own severity split. A nonzero
    /// straightness deviation at the root is expected rather than a defect: the
    /// root rib's spar anchor follows the streamwise root chord, since that is
    /// where the constrained cut is, while every station outboard of it follows
    /// the rib direction perpendicular to the leading edge, and the two
    /// conventions are not collinear where they meet.
    pub fn ok(&self) -> bool {
        self.n_warping_bad == 0
    }
}

/// A mesh defect severe enough that there is no deck to return.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum MeshError {
    /// The zipper bridging produced triangles with no area.
    #[error(
        "{count} CTRIA3 elements are degenerate (zero area or duplicate nodes): \
         the zipper-triangle skin-bridging logic produced an invalid mesh"
    )]
    DegenerateTriangles {
        /// How many triangles were degenerate.
        count: usize,
    },
    /// A grid landed inboard of the root plane, which the semi-wing model has
    /// no meaning for.
    #[error(
        "{count} nodes have Y < 0 (worst={worst:.4} m): the wing must not extend \
         past the root; check the rib lengths' root-plane truncation"
    )]
    NodeBelowRoot {
        /// How many grids were inboard of the root.
        count: usize,
        /// The most negative span found, metres.
        worst: f64,
    },
}

/// Whether the rib at position `pos` in the main skin-rib list gets
/// trailing-edge panels: `_te_rib_selected`.
///
/// `step_<n>` with a step of zero is the one input the reference cannot answer:
/// it reaches a modulo by zero, which raises rather than returning a decision.
/// There is nothing to reproduce in a raised exception, so it takes the same
/// exit as any other unrecognized mode, every rib selected.
fn te_rib_selected(mode: &str, pos: usize, y_station: f64, y_break: f64, n_inboard: usize) -> bool {
    let mode = mode.to_ascii_lowercase();
    match mode.as_str() {
        "all" => return true,
        "none" => return false,
        "alternate" => return pos % 2 == 0,
        "inboard" => return y_station <= y_break,
        "outboard" => return y_station > y_break,
        "inboard_alternate" => {
            if y_station <= y_break {
                return true;
            }
            // Python's `-` on unsigned positions would wrap here; the count of
            // inboard ribs never exceeds the position of an outboard one.
            return (pos.saturating_sub(n_inboard)) % 2 == 0;
        }
        _ => {}
    }
    if let Some(rest) = mode.strip_prefix("step_") {
        if let Some(step) = rest.split('_').next().and_then(|s| s.parse::<i64>().ok()) {
            if step != 0 {
                return pos as i64 % step == 0;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_trailing_edge_mode_selects_the_ribs_its_name_promises() {
        assert!(te_rib_selected("all", 3, 5.0, 10.0, 2));
        assert!(!te_rib_selected("none", 0, 5.0, 10.0, 2));
        assert!(te_rib_selected("alternate", 4, 5.0, 10.0, 2));
        assert!(!te_rib_selected("alternate", 3, 5.0, 10.0, 2));
        assert!(te_rib_selected("inboard", 7, 9.9, 10.0, 2));
        assert!(!te_rib_selected("inboard", 7, 10.1, 10.0, 2));
        assert!(!te_rib_selected("outboard", 7, 10.0, 10.0, 2));
        assert!(te_rib_selected("outboard", 7, 10.1, 10.0, 2));
    }

    #[test]
    fn inboard_alternate_takes_every_inboard_rib_then_every_other_one_outboard() {
        assert!(te_rib_selected("inboard_alternate", 1, 4.0, 10.0, 3));
        assert!(te_rib_selected("inboard_alternate", 3, 11.0, 10.0, 3));
        assert!(!te_rib_selected("inboard_alternate", 4, 11.0, 10.0, 3));
        assert!(te_rib_selected("inboard_alternate", 5, 11.0, 10.0, 3));
    }

    #[test]
    fn a_step_mode_keeps_every_nth_rib_and_a_malformed_one_keeps_all_of_them() {
        assert!(te_rib_selected("step_3", 6, 5.0, 10.0, 2));
        assert!(!te_rib_selected("step_3", 7, 5.0, 10.0, 2));
        // No number, an unparseable one, and the modulo-by-zero the reference
        // raises on all fall through to selecting every rib.
        assert!(te_rib_selected("step_", 7, 5.0, 10.0, 2));
        assert!(te_rib_selected("step_x", 7, 5.0, 10.0, 2));
        assert!(te_rib_selected("step_0", 7, 5.0, 10.0, 2));
        assert!(te_rib_selected("nonsense", 7, 5.0, 10.0, 2));
    }

    #[test]
    fn the_mode_name_is_matched_without_regard_to_case() {
        assert!(!te_rib_selected("NONE", 0, 5.0, 10.0, 2));
        assert!(te_rib_selected("Step_3", 3, 5.0, 10.0, 2));
    }

    #[test]
    fn a_report_is_ok_exactly_when_no_panel_is_badly_warped() {
        let mut report = MeshHealthReport {
            n_perp_warnings: 4,
            n_spar_straightness_warnings: 2,
            ..Default::default()
        };
        assert!(report.ok());
        report.n_warping_bad = 1;
        assert!(!report.ok());
    }
}
