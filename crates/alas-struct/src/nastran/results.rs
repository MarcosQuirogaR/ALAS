// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What a solve produced, read back off the result file.
//!
//! Three readers, one per solution, each reducing an OP2 table to the handful
//! of numbers this program reports: the tip deflection and peak root stress of
//! each static load case, the elastic mode frequencies and their front-spar
//! shapes, and the frequency response at the four monitor grids.
//!
//! Almost all of the content here is the reader's behaviour when something is
//! *absent*. That is not defensiveness; it is the normal case. A solve can
//! write some subcases and not others, a `.op2` can carry displacements and no
//! stress table, a monitor grid can be missing from the result, and every mode
//! a modal solve finds can be a rigid-body mode below the reporting threshold.
//! None of those is an error: each one means a smaller answer, and upstream
//! returns exactly that. The parity fixture is built scenario by scenario
//! around these branches for the same reason.
//!
//! Two shapes differ from upstream's, both because Python's dictionaries carry
//! ordering that a `BTreeMap` would silently reorder:
//!
//! * The label-keyed results are [`LabelledValues`], an insertion-ordered list
//!   of pairs. Upstream's `tip_deflection_m` comes out in load-case order and
//!   its `miles_rms_m` in monitor order (root, kink, engine, tip) and both
//!   are read back in that order by anything that reports them. Sorted by name
//!   they would read `engine, kink, root, tip`, which is nothing.
//! * The mode-shape reader returns `None` rather than an empty vector for
//!   `mode_shape_y_m`, because upstream distinguishes them: `None` means it
//!   never got as far as building a shape.

mod static_and_modes;
#[cfg(test)]
mod tests;
mod vibration;

pub use static_and_modes::{read_modes, read_static, SpanStations};
use vibration::position_of;
pub use vibration::{read_force_psd_rms, read_harmonic_response, read_vibration};

use std::collections::BTreeSet;

use crate::loads::LoadCase;
use crate::mesh::{Deck, MeshNodeIndex};
use crate::op2::{ComplexVectorTable, Op2};

use super::MonitorNodes;

/// Whether a solve produced a usable result.
///
/// The three states of upstream's `status` string, as an enum so a caller
/// branches on a variant rather than on `== "ok"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResultStatus {
    /// No solve has been attempted.
    #[default]
    NotRun,
    /// The solve ran and its result is populated.
    Ok,
    /// The solve failed; `error` says why.
    Error,
}

impl ResultStatus {
    /// The string upstream uses (`"not_run"`/`"ok"`/`"error"`), which is what
    /// the fixture records and the parity test compares.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotRun => "not_run",
            Self::Ok => "ok",
            Self::Error => "error",
        }
    }
}

/// Values keyed by a label, in the order they were recorded.
///
/// See the module docs: the order is the load-case order or the monitor order,
/// and it is the order a report renders them in.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LabelledValues(Vec<(String, f64)>);

impl LabelledValues {
    /// Record `value` under `label`, at the end.
    pub fn push(&mut self, label: impl Into<String>, value: f64) {
        self.0.push((label.into(), value));
    }

    /// The value recorded under `label`, if any.
    pub fn get(&self, label: &str) -> Option<f64> {
        self.0
            .iter()
            .find(|(recorded, _)| recorded == label)
            .map(|&(_, value)| value)
    }

    /// The labels, in order.
    pub fn labels(&self) -> Vec<&str> {
        self.0.iter().map(|(label, _)| label.as_str()).collect()
    }

    /// The pairs, in order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, f64)> {
        self.0.iter().map(|(label, value)| (label.as_str(), *value))
    }

    /// How many values were recorded.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether nothing was recorded.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

pub use super::static_spanwise::StaticResult;

/// What a SOL 103 normal-modes solve reported.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModesResult {
    /// `"not_run"`, `"ok"` or `"error"`.
    pub status: ResultStatus,
    /// A human-readable reason when `status` is `Error`.
    pub error: Option<String>,
    /// Elastic mode frequencies in hertz, rigid-body modes removed.
    pub frequencies_hz: Vec<f64>,
    /// Per mode, the front-spar out-of-plane shape normalized to a peak of 1,
    /// sampled at [`ModesResult::mode_shape_y_m`] and in the same order as
    /// [`ModesResult::frequencies_hz`].
    ///
    /// Upstream's reason for reporting the shape and not only the frequency:
    /// with 30 modes requested, a real solve finds torsional and local-panel
    /// modes that the four-entry Rayleigh trial-shape table has no counterpart
    /// for, so pairing the two by index compares unrelated modes. A caller
    /// matches on frequency instead, and needs the shape to do it.
    pub mode_shapes: Vec<Vec<f64>>,
    /// The span stations the shapes are sampled at, ascending. `None` when the
    /// reader never got as far as building one.
    pub mode_shape_y_m: Option<Vec<f64>>,
}

/// What the two SOL 111 frequency-response solves reported.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VibrationResult {
    /// `"not_run"`, `"ok"` or `"error"`.
    pub status: ResultStatus,
    /// A human-readable reason when `status` is `Error`.
    pub error: Option<String>,
    /// The swept frequencies of the sine solve, in hertz.
    pub frf_freq_hz: Option<Vec<f64>>,
    /// Tip response magnitude at each of those frequencies, in m/N.
    pub frf_tip_abs_m_per_n: Option<Vec<f64>>,
    /// The largest tip response in the sweep.
    pub peak_frf: f64,
    /// The frequency it occurred at, in hertz.
    pub peak_freq_hz: f64,
    /// Miles'-rule RMS displacement per monitor grid, in metres.
    pub miles_rms_m: LabelledValues,
    /// RMS displacement per monitor grid from the solved unit-force SOL 111
    /// receptance integrated against a force PSD.
    pub nastran_rms_m: LabelledValues,
    /// Why force-PSD RMS could not be produced from the harmonic response.
    pub random_response_error: Option<String>,
}

/// All three solves' results together.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NastranResults {
    /// The static solve.
    pub static_solve: StaticResult,
    /// The normal-modes solve.
    pub modes: ModesResult,
    /// The two vibration solves.
    pub vibration: VibrationResult,
}
