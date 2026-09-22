// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/integration/nastran_runner.py
// Reference: alas @ rust-port-baseline.

//! NASTRAN static, modal and vibration solves: the decks, the run, the results.
//!
//! This is P9's second external-solver row, and it means the same thing the
//! first one did: the numbers a solve reports come out of a compiled solver,
//! not out of arithmetic this crate performs. What is this crate's own is the
//! deck it hands over ([`build_sol101_bulk`] and its three siblings) and
//! that is what the parity test holds to the reference.
//!
//! The decks are text, written card by card, and are compared as text. Only the
//! `FORCE` cards escape that: their magnitudes distribute a load case's total
//! over the mesh's front-spar node line, which is arithmetic over grid
//! coordinates, so the parity test compares the number rather than its
//! rendering. Everything else, every case-control line, every set
//! identifier, every comment, is byte-for-byte.
//!
//! Three identifier ranges never overlap: load combinations from 1, gravity
//! sets from 100, force sets from 200. The reference records why, and it is not
//! a style preference: an earlier scheme multiplied the subcase number, and
//! pull-up's force set collided with push-down's gravity set, which NASTRAN
//! would have merged into one load set without complaining.

mod analysis;
mod decks;
mod format;
mod results;
mod run;
pub(crate) mod text;

pub use analysis::run_nastran_analysis;
pub use decks::{
    build_sol101_bulk, build_sol103_bulk, build_sol111_random_bulk, build_sol111_sine_bulk,
    build_sol111_sine_bulk_msc, elliptic_forces_by_y,
};
pub use format::free_field;
pub use results::{
    read_force_psd_rms, read_harmonic_response, read_modes, read_static, read_vibration,
    LabelledValues, ModesResult, NastranResults, ResultStatus, SpanStations, StaticResult,
    VibrationResult,
};
pub use run::{
    fatal_lines, msc_solver_arguments, run_nastran, run_nastran_with_solver, solver_arguments,
    tail, NastranRunOutcome,
};

use crate::mesh::MeshNodeIndex;

/// The four grids a vibration solve reports response at.
///
/// A dictionary upstream, keyed by the same four names; a struct here because
/// the set is fixed and every consumer reads all four.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorNodes {
    /// The wing root.
    pub root: i64,
    /// The planform break.
    pub kink: i64,
    /// Where the engine hangs, and where the sweep is driven from.
    pub engine: i64,
    /// The wing tip.
    pub tip: i64,
}

impl MonitorNodes {
    /// The four grids, in the order the reference's dictionary lists them,
    /// which is the order a vibration result reports its monitors in.
    pub fn labelled(self) -> [(&'static str, i64); 4] {
        [
            ("root", self.root),
            ("kink", self.kink),
            ("engine", self.engine),
            ("tip", self.tip),
        ]
    }

    /// The four grids as a list, duplicates included.
    pub fn all(self) -> Vec<i64> {
        vec![self.root, self.kink, self.engine, self.tip]
    }
}

/// The monitor grids for one mesh: `_monitor_set`.
///
/// An aircraft with no wing-mounted engine has nothing hanging off the wing to
/// drive the sweep from, so the break station stands in: it is the stiffness
/// discontinuity a response is most likely to show up at.
pub fn monitor_set(node_index: &MeshNodeIndex) -> MonitorNodes {
    MonitorNodes {
        root: node_index.root_nid,
        kink: node_index.kink_nid,
        engine: node_index
            .engine_nids
            .first()
            .copied()
            .unwrap_or(node_index.kink_nid),
        tip: node_index.tip_nid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(engines: Vec<i64>) -> MeshNodeIndex {
        MeshNodeIndex {
            root_nid: 4,
            tip_nid: 110,
            kink_nid: 46,
            spar_upper_nids: Vec::new(),
            spar_lower_nids: Vec::new(),
            engine_nids: engines,
        }
    }

    #[test]
    fn the_first_wing_engine_is_where_a_sweep_is_driven_from() {
        let monitors = monitor_set(&index(vec![37, 61]));
        assert_eq!(monitors.engine, 37);
        assert_eq!(monitors.all(), vec![4, 46, 37, 110]);
    }

    #[test]
    fn a_wing_with_no_engine_drives_the_sweep_from_the_break() {
        let monitors = monitor_set(&index(Vec::new()));
        assert_eq!(monitors.engine, monitors.kink);
        // The set the deck writes is deduplicated, so this leaves three grids.
        let mut ids = monitors.all();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids, vec![4, 46, 110]);
    }

    #[test]
    fn the_monitor_labels_are_the_order_a_vibration_result_reports_them_in() {
        let labels: Vec<&str> = monitor_set(&index(vec![37]))
            .labelled()
            .iter()
            .map(|&(label, _)| label)
            .collect();
        assert_eq!(labels, ["root", "kink", "engine", "tip"]);
    }
}
