// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/integration/nastran_runner.py (_read_static, _read_modes,
// _read_vibration and the four result dataclasses).
// Reference: alas @ rust-port-baseline.

//! What a solve produced, read back off the result file.
//!
//! Three readers, one per solution, each reducing an OP2 table to the handful
//! of numbers this program reports: the tip deflection and peak root stress of
//! each static load case, the elastic mode frequencies and their front-spar
//! shapes, and the frequency response at the four monitor grids.
//!
//! Almost all of the content here is the reader's behaviour when something is
//! *absent*. That is not defensiveness -- it is the normal case. A solve can
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
//!   its `miles_rms_m` in monitor order -- root, kink, engine, tip -- and both
//!   are read back in that order by anything that reports them. Sorted by name
//!   they would read `engine, kink, root, tip`, which is nothing.
//! * The mode-shape reader returns `None` rather than an empty vector for
//!   `mode_shape_y_m`, because upstream distinguishes them: `None` means it
//!   never got as far as building a shape.

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
/// [`Deck`] because that is what the reader actually wants -- one coordinate
/// per grid, not a deck -- and because it lets the parity test supply the
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
        let Some(shape) = eigenvectors.data.get(mode) else {
            continue;
        };
        let out_of_plane: Vec<f64> = ordered
            .iter()
            .map(|&(_, row)| shape.get(row).map_or(f64::NAN, |values| values[2]))
            .collect();
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
/// response PSD directly. Reporting both is the point -- they are the estimate
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

/// Integrate a one-sided force PSD through the solved unit-force response.
///
/// A SOL 111 sine sweep with the product deck applies one newton at the engine
/// grid, so its displacement magnitude is a receptance `H` in m/N.  A force
/// PSD `S_F` in N^2/Hz consequently produces displacement PSD `|H|^2 S_F` in
/// m^2/Hz. Integrating the latter over the solved frequency band gives variance
/// in m^2, whose square root is displacement RMS in m. This route deliberately
/// does not use the frozen acceleration-PSD configuration or Miles' rule.
pub fn read_force_psd_rms(
    sine: &Op2,
    monitors: MonitorNodes,
    force_psd_n2_per_hz: f64,
) -> Result<LabelledValues, String> {
    if !force_psd_n2_per_hz.is_finite() || force_psd_n2_per_hz <= 0.0 {
        return Err(
            "random excitation force PSD must be finite and greater than zero (N^2/Hz)".to_owned(),
        );
    }
    let Some(table) = sine.complex_displacements.get(&1) else {
        return Err(
            "SOL 111 did not return complex displacement data for the force-PSD RMS calculation"
                .to_owned(),
        );
    };
    if table.freqs.len() < 2 {
        return Err(
            "SOL 111 requires at least two frequency points for force-PSD RMS integration"
                .to_owned(),
        );
    }

    let mut rms = LabelledValues::default();
    for (label, nid) in monitors.labelled() {
        let Some(row) = position_of(&table.node_ids, nid) else {
            continue;
        };
        let response_psd: Vec<f64> = magnitudes(table, row)
            .into_iter()
            .map(|receptance| receptance.powi(2) * force_psd_n2_per_hz)
            .collect();
        let variance = trapezoid(&response_psd, &table.freqs);
        if variance.is_finite() && variance >= 0.0 {
            rms.push(label, variance.sqrt());
        }
    }
    if rms.is_empty() {
        return Err(
            "SOL 111 did not return any configured monitor grids for force-PSD RMS integration"
                .to_owned(),
        );
    }
    Ok(rms)
}

/// Where `nid` sits in a result table's node line, if it is there at all.
fn position_of(node_ids: &[i64], nid: i64) -> Option<usize> {
    node_ids.iter().position(|&candidate| candidate == nid)
}

/// The out-of-plane response magnitude at one grid, over every frequency.
fn magnitudes(table: &ComplexVectorTable, row: usize) -> Vec<f64> {
    table
        .real
        .iter()
        .zip(&table.imag)
        .map(|(real, imag)| {
            let re = real.get(row).map_or(f64::NAN, |values| values[2]);
            let im = imag.get(row).map_or(f64::NAN, |values| values[2]);
            re.hypot(im)
        })
        .collect()
}

/// The index of the largest value, the first of them if several tie.
///
/// `None` for an empty slice, where upstream would raise and the caller would
/// have nothing to record.
fn argmax(values: &[f64]) -> Option<usize> {
    values
        .iter()
        .enumerate()
        .fold(
            None,
            |best: Option<(usize, f64)>, (index, &value)| match best {
                Some((_, incumbent)) if incumbent >= value => best,
                _ => Some((index, value)),
            },
        )
        .map(|(index, _)| index)
}

/// The trapezoidal integral of `y` over `x`.
///
/// Reproduces `numpy.trapezoid`, including summing the panels in ascending
/// order: with a handful of samples numpy's pairwise summation degenerates to
/// exactly this loop.
fn trapezoid(y: &[f64], x: &[f64]) -> f64 {
    y.windows(2)
        .zip(x.windows(2))
        .map(|(y, x)| (x[1] - x[0]) * (y[1] + y[0]) / 2.0)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labelled_values_keep_the_order_they_were_recorded_in() {
        let mut values = LabelledValues::default();
        values.push("root", 1.0);
        values.push("tip", 2.0);
        values.push("engine", 3.0);
        // Sorted, "engine" would come first, and a monitor table would read in
        // an order that means nothing.
        assert_eq!(values.labels(), ["root", "tip", "engine"]);
        assert_eq!(values.get("engine"), Some(3.0));
        assert_eq!(values.get("kink"), None);
        assert_eq!(values.len(), 3);
    }

    #[test]
    fn force_psd_rms_has_displacement_units_after_frequency_integration() {
        let mut op2 = Op2::default();
        op2.complex_displacements.insert(
            1,
            ComplexVectorTable {
                freqs: vec![1.0, 3.0],
                node_ids: vec![42],
                real: vec![
                    vec![[0.0, 0.0, 2.0, 0.0, 0.0, 0.0]],
                    vec![[0.0, 0.0, 2.0, 0.0, 0.0, 0.0]],
                ],
                imag: vec![
                    vec![[0.0, 0.0, 0.0, 0.0, 0.0, 0.0]],
                    vec![[0.0, 0.0, 0.0, 0.0, 0.0, 0.0]],
                ],
            },
        );
        let monitors = MonitorNodes {
            root: 42,
            kink: 42,
            engine: 42,
            tip: 42,
        };

        let rms = read_force_psd_rms(&op2, monitors, 3.0).expect("valid force PSD RMS");

        // |H|^2 S_F = 2^2 * 3 = 12 m^2/Hz; its 1--3 Hz integral is
        // 24 m^2, so every aliased monitor reports sqrt(24) metres.
        for label in ["root", "kink", "engine", "tip"] {
            assert!((rms.get(label).unwrap_or(f64::NAN) - 24.0_f64.sqrt()).abs() < 1e-12);
        }
    }

    #[test]
    fn force_psd_rms_rejects_non_positive_spectra() {
        let error = read_force_psd_rms(
            &Op2::default(),
            MonitorNodes {
                root: 1,
                kink: 2,
                engine: 3,
                tip: 4,
            },
            0.0,
        )
        .expect_err("zero force PSD is not physical");
        assert!(error.contains("greater than zero"));
    }

    #[test]
    fn the_status_strings_are_the_ones_the_reference_writes() {
        assert_eq!(ResultStatus::default().as_str(), "not_run");
        assert_eq!(ResultStatus::Ok.as_str(), "ok");
        assert_eq!(ResultStatus::Error.as_str(), "error");
    }

    #[test]
    fn argmax_takes_the_first_of_several_equal_maxima() {
        assert_eq!(argmax(&[1.0, 3.0, 3.0, 2.0]), Some(1));
        assert_eq!(argmax(&[]), None);
        assert_eq!(argmax(&[5.0]), Some(0));
    }

    #[test]
    fn a_trapezoid_over_one_panel_is_its_average_times_its_width() {
        assert_eq!(trapezoid(&[2.0, 4.0], &[0.0, 3.0]), 9.0);
        // Fewer than two samples enclose no area.
        assert_eq!(trapezoid(&[2.0], &[1.0]), 0.0);
        assert_eq!(trapezoid(&[], &[]), 0.0);
    }

    #[test]
    fn a_uniform_integrand_integrates_to_its_span() {
        let x: Vec<f64> = (0..5).map(f64::from).collect();
        let y = vec![2.0; 5];
        assert_eq!(trapezoid(&y, &x), 8.0);
    }
}
