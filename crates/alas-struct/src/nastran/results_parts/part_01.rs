// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use std::collections::BTreeSet;

use crate::loads::LoadCase;
use crate::mesh::{Deck, MeshNodeIndex};
use crate::op2::{ComplexVectorTable, Op2};

use super::MonitorNodes;

/// Standard gravity, as upstream's `_read_vibration` writes it.
///
/// Deliberately the local literal rather than a shared constant: this is the
/// number the PSD conversion below was calibrated with, and the acceleration
/// PSD it converts is quoted in g^2/Hz against this same value.
const GRAVITY_M_S2: f64 = 9.81;

/// Modes at or below this frequency are rigid-body modes, and are not reported.
///
/// A free-free-ish model returns a handful of near-zero modes that are numerical
/// artefacts of the constraint set rather than structural modes. The threshold
/// is upstream's, and upstream took it from the reference scripts' own
/// `_get_structural_freqs_nastran`.
const RIGID_BODY_CUTOFF_HZ: f64 = 0.5;

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

/// What a SOL 101 static solve reported.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StaticResult {
    /// `"not_run"`, `"ok"` or `"error"`.
    pub status: ResultStatus,
    /// A human-readable reason when `status` is `Error`.
    pub error: Option<String>,
    /// Out-of-plane tip deflection per load case, in metres.
    pub tip_deflection_m: LabelledValues,
    /// Peak corner von Mises stress per load case, in pascals.
    pub root_von_mises_max_pa: LabelledValues,
}

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

/// Read a SOL 101 result: tip deflection and peak corner stress per load case.
///
/// Subcase identifiers are the load cases' positions, one-based, which is how
/// the deck writer assigned them. A case whose subcase the solve did not write
/// is skipped rather than reported as zero.
pub fn read_static(op2: &Op2, node_index: &MeshNodeIndex, cases: &[LoadCase]) -> StaticResult {
    let mut result = StaticResult {
        status: ResultStatus::Ok,
        ..StaticResult::default()
    };
    for (index, case) in cases.iter().enumerate() {
        let sid = index as i64 + 1;
        let Some(table) = op2.displacements.get(&sid) else {
            continue;
        };
        if let Some(row) = position_of(&table.node_ids, node_index.tip_nid) {
            if let Some(values) = table.data.get(row) {
                result.tip_deflection_m.push(case.name, values[2]);
            }
        }
        // An empty stress table has no maximum, which upstream reaches as a
        // raised ValueError that it catches and ignores.
        if let Some(stress) = op2.cquad4_stress.get(&sid).filter(|s| !s.data.is_empty()) {
            // Column 7 is von Mises in the CQUAD4 corner output layout, and it
            // is not the last column in general.
            let peak = stress
                .data
                .iter()
                .map(|row| row[7].abs())
                .fold(f64::NEG_INFINITY, f64::max);
            result.root_von_mises_max_pa.push(case.name, peak);
        }
    }
    result
}

/// Where each grid sits along the span.
///
/// [`read_modes`] sorts the front-spar line by span station, which upstream
/// reads off the BDF model it is handed. A trait rather than the concrete
/// [`Deck`] because that is what the reader actually wants (one coordinate
/// per grid, not a deck) and because it lets the parity test supply the
/// reference's own grid table instead of rebuilding a mesh around it.
pub trait SpanStations {
    /// The span station of grid `nid`, or `NaN` if there is no such grid.
    fn span_station(&self, nid: i64) -> f64;
}

impl SpanStations for Deck {
    fn span_station(&self, nid: i64) -> f64 {
        self.node_y(nid)
    }
}

/// Read a SOL 103 result: elastic frequencies and their front-spar shapes.
///
/// `stations` supplies the span station of each grid, which the shapes are
/// sorted and sampled by. Modes at or below [`RIGID_BODY_CUTOFF_HZ`] are
/// dropped.
pub fn read_modes(
    op2: &Op2,
    stations: &impl SpanStations,
    node_index: &MeshNodeIndex,
) -> ModesResult {
    let mut result = ModesResult {
        status: ResultStatus::Ok,
        ..ModesResult::default()
    };
    let Some(eigenvectors) = op2.eigenvectors.get(&1) else {
        return result;
    };
    let kept: Vec<usize> = eigenvectors
        .mode_cycles
        .iter()
        .enumerate()
        .filter(|&(_, &cycles)| cycles > RIGID_BODY_CUTOFF_HZ)
        .map(|(index, _)| index)
        .collect();
    result.frequencies_hz = kept
        .iter()
        .map(|&index| eigenvectors.mode_cycles[index])
        .collect();
    if kept.is_empty() {
        return result;
    }

    // The front spar alone: it is the line the deck applies its aerodynamic
    // FORCE cards along, and the line the analytical estimate this is compared
    // against is written for.
    let Some(front_spar) = node_index.spar_upper_nids.first() else {
        result.status = ResultStatus::Error;
        result.error = Some("the mesh index has no spar node lines".to_owned());
        return result;
    };
    let front_spar: BTreeSet<i64> = front_spar.iter().copied().collect();
    let on_front_spar: Vec<usize> = eigenvectors
        .node_ids
        .iter()
        .enumerate()
        .filter(|&(_, nid)| front_spar.contains(nid))
        .map(|(index, _)| index)
        .collect();
    if on_front_spar.is_empty() {
        return result;
    }

    let mut ordered: Vec<(f64, usize)> = on_front_spar
        .iter()
        .map(|&row| (stations.span_station(eigenvectors.node_ids[row]), row))
        .collect();
    ordered.sort_by(|(left, _), (right, _)| left.total_cmp(right));
    result.mode_shape_y_m = Some(ordered.iter().map(|&(y, _)| y).collect());

    for &mode in &kept {
        // Keep one shape slot for every retained frequency, even when an OP2
        // omits that mode's vector table.  `frequencies_hz` and
        // `mode_shapes` are parallel by contract; silently skipping a vector
        // would shift every later shape onto the wrong frequency and could
        // make the report compare unrelated modes.
        let out_of_plane: Vec<f64> = eigenvectors
            .data
            .get(mode)
            .map(|shape| {
                ordered
                    .iter()
                    .map(|&(_, row)| shape.get(row).map_or(f64::NAN, |values| values[2]))
                    .collect()
            })
            .unwrap_or_else(|| vec![f64::NAN; ordered.len()]);
        // Upstream's `float(np.max(np.abs(t3))) or 1.0`: a mode that is
        // identically zero on this line normalizes by one rather than by zero.
        let peak = out_of_plane
            .iter()
            .map(|value| value.abs())
            .fold(f64::NEG_INFINITY, f64::max);
        let peak = if peak == 0.0 || !peak.is_finite() {
            1.0
        } else {
            peak
        };
        result
            .mode_shapes
            .push(out_of_plane.iter().map(|value| value / peak).collect());
    }
    result
}

/// Read the two SOL 111 results: the tip frequency response, and the RMS
/// displacement at each monitor grid by two routes.
///
/// Either solve may be absent, and each populates a different half of the
/// result: the sine sweep gives the transfer function and, through Miles' rule,
/// an RMS estimate from it; the random solve gives an RMS by integrating the
/// response PSD directly. Reporting both is the point; they are the estimate
/// and the solve of the same quantity.
pub fn read_vibration(
    sine: Option<&Op2>,
    random: Option<&Op2>,
    modal_freqs: &[f64],
    monitors: MonitorNodes,
    damping_ratio: f64,
    psd_base_g2_per_hz: f64,
) -> VibrationResult {
    let mut result = VibrationResult {
        status: ResultStatus::Ok,
        ..VibrationResult::default()
    };
    let psd_si = psd_base_g2_per_hz * GRAVITY_M_S2.powi(2);

    if let Some(table) = sine.and_then(|op2| op2.complex_displacements.get(&1)) {
        if let Some(row) = position_of(&table.node_ids, monitors.tip) {
            let response = magnitudes(table, row);
            if let Some(peak) = argmax(&response) {
                result.peak_frf = response[peak];
                result.peak_freq_hz = table.freqs[peak];
            }
            result.frf_freq_hz = Some(table.freqs.clone());
            result.frf_tip_abs_m_per_n = Some(response);
        }

        // Upstream reads the first modal frequency and tests it for truth, so a
        // solve whose first mode came back as exactly zero skips Miles' rule
        // the same way one with no modes at all does.
        let has_first_mode = matches!(modal_freqs.first(), Some(&first) if first != 0.0);
        if has_first_mode {
            for (label, nid) in monitors.labelled() {
                let Some(row) = position_of(&table.node_ids, nid) else {
                    continue;
                };
                let response = magnitudes(table, row);
                let Some(peak) = argmax(&response) else {
                    continue;
                };
                // Miles' rule: the RMS response of a lightly damped
                // single-degree-of-freedom system to a flat-topped base PSD.
                let rms = response[peak]
                    * (std::f64::consts::PI * table.freqs[peak] * psd_si / (4.0 * damping_ratio))
                        .sqrt();
                result.miles_rms_m.push(label, rms);
            }
        }
    }

    if let Some(table) = random.and_then(|op2| op2.complex_displacements.get(&1)) {
        for (label, nid) in monitors.labelled() {
            let Some(row) = position_of(&table.node_ids, nid) else {
                continue;
            };
            let spectrum: Vec<f64> = magnitudes(table, row)
                .iter()
                .map(|value| value.powi(2) * psd_si)
                .collect();
            result
                .nastran_rms_m
                .push(label, trapezoid(&spectrum, &table.freqs).sqrt());
        }
    }
    result
}

/// Read the validated deterministic SOL 111 receptance only.
///
/// The excitation deck applies a unit harmonic force, so the displacement
/// magnitude is a receptance in metres per newton. This deliberately does not
/// calculate either legacy RMS estimate: both combine that force receptance
/// with an acceleration PSD and are dimensionally invalid.
pub fn read_harmonic_response(sine: &Op2, monitors: MonitorNodes) -> VibrationResult {
    let mut result = VibrationResult {
        status: ResultStatus::Ok,
        ..VibrationResult::default()
    };
    let Some(table) = sine.complex_displacements.get(&1) else {
        return result;
    };
    let Some(row) = position_of(&table.node_ids, monitors.tip) else {
        return result;
    };
    let response = magnitudes(table, row);
    if let Some(peak) = argmax(&response) {
        result.peak_frf = response[peak];
        result.peak_freq_hz = table.freqs[peak];
    }
    result.frf_freq_hz = Some(table.freqs.clone());
    result.frf_tip_abs_m_per_n = Some(response);
    result
}
