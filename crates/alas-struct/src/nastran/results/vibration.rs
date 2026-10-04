// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reading the frequency-response (SOL 111) results.

use super::*;

/// Standard gravity, as upstream's `_read_vibration` writes it.
///
/// Deliberately the local literal rather than a shared constant: this is the
/// number the PSD conversion below was calibrated with, and the acceleration
/// PSD it converts is quoted in g^2/Hz against this same value.
const GRAVITY_M_S2: f64 = 9.81;

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
pub(super) fn position_of(node_ids: &[i64], nid: i64) -> Option<usize> {
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
pub(super) fn argmax(values: &[f64]) -> Option<usize> {
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
pub(super) fn trapezoid(y: &[f64], x: &[f64]) -> f64 {
    y.windows(2)
        .zip(x.windows(2))
        .map(|(y, x)| (x[1] - x[0]) * (y[1] + y[0]) / 2.0)
        .sum()
}
