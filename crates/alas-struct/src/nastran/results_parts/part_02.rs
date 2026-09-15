// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


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

        // |H|^2 S_F = 2^2 * 3 = 12 m^2/Hz; its 1 to 3 Hz integral is
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

