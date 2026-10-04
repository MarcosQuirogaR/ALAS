// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::vibration::{argmax, trapezoid};
use super::*;
use crate::mesh::{Deck, MeshNodeIndex};
use crate::op2::EigenvectorTable;

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

#[test]
fn modal_shape_slots_stay_parallel_when_an_op2_vector_is_missing() {
    let mut deck = Deck::new();
    deck.add_grid(1, [0.0, 0.0, 0.0]);
    deck.add_grid(2, [0.0, 1.0, 0.0]);
    let node_index = MeshNodeIndex {
        root_nid: 1,
        tip_nid: 2,
        kink_nid: 1,
        spar_upper_nids: vec![vec![1, 2]],
        spar_lower_nids: Vec::new(),
        engine_nids: Vec::new(),
    };
    let mut op2 = Op2::default();
    op2.eigenvectors.insert(
        1,
        EigenvectorTable {
            modes: vec![1, 2],
            eigenvalues: vec![1.0, 4.0],
            mode_cycles: vec![1.0, 2.0],
            node_ids: vec![1, 2],
            // The frequency table has two retained modes, while the
            // vector table contains only the first.  The reader must keep
            // an all-NaN slot for the absent second shape so a later
            // frequency can never inherit the preceding mode's vector.
            data: vec![vec![
                [0.0, 0.0, 0.5, 0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0, 0.0, 0.0],
            ]],
        },
    );

    let result = read_modes(&op2, &deck, &node_index);

    assert_eq!(result.frequencies_hz, vec![1.0, 2.0]);
    assert_eq!(result.mode_shapes.len(), result.frequencies_hz.len());
    assert!(result.mode_shapes[0].iter().all(|value| value.is_finite()));
    assert!(result.mode_shapes[1].iter().all(|value| value.is_nan()));
}
