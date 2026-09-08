// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Opt-in product-path evidence against an installed MSC Nastran.
//!
//! The ordinary suite proves the exact command tokens and the corrected RBE3
//! card without a commercial dependency. This ignored test proves the boundary
//! they cannot: the public structural analysis path writes that mesh, passes
//! Patran's `analysis.exe` through `a.solver`, and receives a clean SOL 101
//! result from MSC.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod support;

use std::path::{Path, PathBuf};

use alas_config::{DesignRequirements, EngineConfig, MassModelConfig, StructuresConfig};
use alas_struct::mesh::build_wing_mesh_bdf;
use alas_struct::nastran::{
    build_sol101_bulk, build_sol103_bulk, build_sol111_sine_bulk_msc, fatal_lines,
    read_force_psd_rms, read_harmonic_response, read_modes, read_static, run_nastran_with_solver,
    ResultStatus,
};
use alas_struct::op2::{read_op2, Op2, UnreadResultTable};
use alas_struct::sizing::size_wingbox;
use support::{build_geometry, materials_for, MaterialsRecord};

#[test]
#[ignore = "requires ALAS_MSC_RETAINED_RESULTS pointing at a retained alas_product_* directory"]
fn retained_msc_sol101_and_sol103_op2_files_are_consumed_without_solver_reexecution() {
    let root = required_directory("ALAS_MSC_RETAINED_RESULTS");
    for solution in ["sol101", "sol103"] {
        let f06 = root.join(solution).join(format!("wing_{solution}.f06"));
        let text = std::fs::read_to_string(&f06)
            .unwrap_or_else(|error| panic!("read {}: {error}", f06.display()));
        assert!(
            fatal_lines(&text).is_empty(),
            "fatal message in {}",
            f06.display()
        );
        assert!(
            text.contains("* * * END OF JOB * * *"),
            "retained solve did not reach end of job: {}",
            f06.display()
        );
    }

    let static_results = parsed_op2(&root.join("sol101/wing_sol101.op2"));
    assert_eq!(static_results.displacements.len(), 3);
    assert_eq!(static_results.cquad4_stress.len(), 3);
    for subcase in 1..=3 {
        let displacement = static_results.displacements.get(&subcase).unwrap();
        assert_eq!(displacement.node_ids.len(), 568);
        assert!(displacement
            .data
            .iter()
            .flatten()
            .all(|value| value.is_finite()));
        let stress = static_results.cquad4_stress.get(&subcase).unwrap();
        assert_eq!(stress.data.len(), 7_290);
        assert!(stress.data.iter().flatten().all(|value| value.is_finite()));
    }
    assert_eq!(static_results.unread_result_tables.len(), 12);
    assert_eq!(
        static_results.unread_result_tables[0],
        UnreadResultTable::Vector {
            name: "OQG1".to_owned(),
            subcase: 1,
            num_wide: 8,
        }
    );

    let modal_results = parsed_op2(&root.join("sol103/wing_sol103.op2"));
    let modes = modal_results.eigenvectors.get(&1).expect("modal subcase 1");
    assert_eq!(modes.modes.len(), 36);
    assert_eq!(modes.node_ids.len(), 568);
    assert!(modes
        .mode_cycles
        .windows(2)
        .all(|pair| pair[0] > 0.0 && pair[0] < pair[1]));
    assert_eq!(modal_results.unread_result_tables.len(), 36);
    assert!(modal_results.unread_result_tables.iter().all(|table| {
        matches!(table, UnreadResultTable::Vector { name, subcase: 1, num_wide: 8 } if name == "OQG1")
    }));
}

#[test]
#[ignore = "requires ALAS_MSC_LAUNCHER and ALAS_MSC_SOLVER"]
fn installed_msc_sol101_sol103_and_harmonic_sol111_are_consumed() {
    let launcher = required_file("ALAS_MSC_LAUNCHER");
    let solver = required_file("ALAS_MSC_SOLVER");
    let requirements = DesignRequirements::default();
    let config = StructuresConfig {
        num_ribs_override: Some(16),
        mesh_chordwise_points: 20,
        nastran_solver_path: solver.display().to_string(),
        run_sol_static: true,
        run_sol_modes: false,
        run_sol_vibration_sine: false,
        run_sol_vibration_random: false,
        freq_sweep_max_hz: 60.0,
        freq_step_hz: 1.0,
        timeout_s: 300.0,
        ..StructuresConfig::default()
    };
    let geometry = build_geometry(&[0.15, 0.60], &[true, true]);
    let named = MaterialsRecord {
        skin: "Al 7075-T6".to_owned(),
        web: "Al 7075-T6".to_owned(),
        cap: "Al 7075-T6".to_owned(),
        rib: "Al 7075-T6".to_owned(),
    };
    let [skin, web, cap, rib] = materials_for(&named);
    let sizing = size_wingbox(&geometry, &config, &requirements, skin, web, cap, rib);
    let (deck, _, node_index) = build_wing_mesh_bdf(
        &geometry,
        &sizing,
        &config,
        &EngineConfig::default(),
        &MassModelConfig::default(),
        &requirements,
        skin,
        web,
        cap,
        rib,
    )
    .expect("the validation wingbox meshes");
    assert!(deck.write_bulk_msc().contains("RBE3    "));

    let output = scratch_dir();
    std::fs::write(output.join("wing_mesh.bdf"), deck.write_bulk_msc())
        .expect("write MSC-compatible mesh");
    assert_clean_solve(
        &output,
        "sol101",
        build_sol101_bulk(
            &deck,
            &node_index,
            &requirements,
            &config,
            "../wing_mesh.bdf",
        ),
        &launcher,
        &solver,
        config.timeout_s,
    );
    assert_clean_solve(
        &output,
        "sol103",
        build_sol103_bulk(&config, "../wing_mesh.bdf"),
        &launcher,
        &solver,
        config.timeout_s,
    );
    assert_clean_solve(
        &output,
        "sol111_sine",
        build_sol111_sine_bulk_msc(&config, &node_index, "../wing_mesh.bdf"),
        &launcher,
        &solver,
        config.timeout_s,
    );

    let static_results = parsed_op2(&output.join("sol101/wing_sol101.op2"));
    assert_eq!(static_results.displacements.len(), 3);
    assert_eq!(static_results.cquad4_stress.len(), 3);
    let mut tip_t3 = Vec::new();
    let mut peak_stress = Vec::new();
    for subcase in 1..=3 {
        let displacement = static_results.displacements.get(&subcase).unwrap();
        assert_eq!(displacement.node_ids.len(), 568);
        assert!(displacement
            .data
            .iter()
            .flatten()
            .all(|value| value.is_finite()));
        let root = displacement
            .node_ids
            .iter()
            .position(|&node| node == node_index.root_nid)
            .expect("root grid is present");
        assert!(displacement.data[root]
            .iter()
            .all(|value| value.abs() < 1e-8));
        let tip = displacement
            .node_ids
            .iter()
            .position(|&node| node == node_index.tip_nid)
            .expect("tip grid is present");
        tip_t3.push(displacement.data[tip][2]);

        let stress = static_results.cquad4_stress.get(&subcase).unwrap();
        assert_eq!(stress.data.len(), 7_290);
        let peak_von_mises = stress.data.iter().map(|row| row[7]).fold(0.0f64, f64::max);
        assert!(peak_von_mises.is_finite());
        assert!((1e5..1e10).contains(&peak_von_mises));
        peak_stress.push(peak_von_mises);
    }
    assert!(tip_t3[0] * tip_t3[1] < 0.0);
    assert!(tip_t3[0] * tip_t3[2] > 0.0);
    assert!(tip_t3
        .iter()
        .all(|value| (1e-6..100.0).contains(&value.abs())));
    assert!((4.0..5.0).contains(&tip_t3[0]));
    assert!((-2.0..-1.0).contains(&tip_t3[1]));
    assert!((1.0..1.3).contains(&tip_t3[2]));
    assert!((1.0e9..1.2e9).contains(&peak_stress[0]));
    assert!((4.0e8..4.8e8).contains(&peak_stress[1]));
    assert!((2.5e8..3.3e8).contains(&peak_stress[2]));

    let cases = alas_struct::loads::load_cases(&requirements, config.additional_safety_factor);
    let static_report = read_static(&static_results, &node_index, &cases);
    assert_eq!(static_report.status, ResultStatus::Ok);
    assert_eq!(static_report.tip_deflection_m.labels().len(), 3);
    assert_eq!(static_report.root_von_mises_max_pa.labels().len(), 3);

    let modal_results = parsed_op2(&output.join("sol103/wing_sol103.op2"));
    let modes = modal_results.eigenvectors.get(&1).expect("modal subcase 1");
    // RESVEC=YES augments the configured extracted modes with six residual
    // vectors in this run; the OP2 must retain both sets rather than truncate
    // to EIGRL ND.
    assert_eq!(modes.modes.len(), config.n_modes as usize + 6);
    assert_eq!(modes.node_ids.len(), 568);
    assert!(modes
        .mode_cycles
        .windows(2)
        .all(|pair| pair[0] > 0.0 && pair[0] < pair[1]));
    assert!(modes
        .mode_cycles
        .last()
        .is_some_and(|frequency| *frequency < 500.0));
    assert!((1.70..1.72).contains(&modes.mode_cycles[0]));
    assert!((127.0..127.1).contains(modes.mode_cycles.last().unwrap()));
    assert!(modes
        .data
        .iter()
        .flatten()
        .flatten()
        .all(|value| value.is_finite()));
    let modal_report = read_modes(&modal_results, &deck, &node_index);
    assert_eq!(modal_report.status, ResultStatus::Ok);
    assert_eq!(
        modal_report.frequencies_hz.len(),
        config.n_modes as usize + 6
    );

    let harmonic_results = parsed_op2(&output.join("sol111_sine/wing_sol111_sine.op2"));
    let harmonic = harmonic_results
        .complex_displacements
        .get(&1)
        .expect("harmonic subcase 1");
    assert_eq!(harmonic.freqs.len(), 60);
    assert_eq!(harmonic.node_ids.len(), 4);
    assert_eq!(harmonic.freqs.first().copied(), Some(1.0));
    assert_eq!(harmonic.freqs.last().copied(), Some(60.0));
    assert!(harmonic.freqs.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(harmonic
        .real
        .iter()
        .chain(&harmonic.imag)
        .flatten()
        .flatten()
        .all(|value| value.is_finite()));

    let harmonic_report = read_harmonic_response(
        &harmonic_results,
        alas_struct::nastran::monitor_set(&node_index),
    );
    assert_eq!(harmonic_report.status, ResultStatus::Ok);
    assert_eq!(harmonic_report.frf_freq_hz.as_ref().map(Vec::len), Some(60));
    assert_eq!(
        harmonic_report.frf_tip_abs_m_per_n.as_ref().map(Vec::len),
        Some(60)
    );
    assert!((4.9..5.1).contains(&harmonic_report.peak_freq_hz));
    assert!((6.2e-7..6.4e-7).contains(&harmonic_report.peak_frf));
    assert!(harmonic_report.miles_rms_m.is_empty());
    assert!(harmonic_report.nastran_rms_m.is_empty());

    let force_psd_rms = read_force_psd_rms(
        &harmonic_results,
        alas_struct::nastran::monitor_set(&node_index),
        config.random_force_psd_n2_per_hz,
    )
    .expect("the installed SOL 111 result supports force-PSD RMS integration");
    assert_eq!(force_psd_rms.labels(), ["root", "kink", "engine", "tip"]);
    assert!(force_psd_rms.iter().all(|(_, value)| value.is_finite()));
    assert!(force_psd_rms.get("tip").is_some_and(|value| value > 0.0));
    let rms_text = force_psd_rms
        .iter()
        .map(|(label, value)| format!("{label}: {value:.12e} m RMS"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(
        output.join("sol111_sine/force_psd_rms.txt"),
        format!(
            "one-sided force PSD: {:.12e} N^2/Hz\nfrequency band: {:.6}--{:.6} Hz\n{rms_text}\n",
            config.random_force_psd_n2_per_hz, config.freq_step_hz, config.freq_sweep_max_hz,
        ),
    )
    .expect("retain force-PSD RMS evidence");
}

fn assert_clean_solve(
    output: &Path,
    solution: &str,
    deck: String,
    launcher: &Path,
    solver: &Path,
    timeout_seconds: f64,
) {
    let solve_dir = output.join(solution);
    std::fs::create_dir_all(&solve_dir).expect("create solution directory");
    let bdf = solve_dir.join(format!("wing_{solution}.bdf"));
    std::fs::write(&bdf, deck).expect("write solution deck");
    let outcome = run_nastran_with_solver(&bdf, launcher, Some(solver), timeout_seconds);
    assert!(outcome.ok, "MSC {solution} failed: {}", outcome.detail);
    let f06_path = bdf.with_extension("f06");
    let f06 = std::fs::read_to_string(&f06_path).expect("MSC wrote an .f06");
    assert!(fatal_lines(&f06).is_empty());
    assert!(
        f06.contains("* * * END OF JOB * * *"),
        "MSC {solution} print file did not reach end of job: {}",
        f06_path.display()
    );
}

fn required_file(variable: &str) -> PathBuf {
    let path = std::env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {variable} to the installed MSC executable"));
    assert!(
        path.is_file(),
        "{variable} is not a file: {}",
        path.display()
    );
    path
}

fn required_directory(variable: &str) -> PathBuf {
    let path = std::env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {variable} to the retained MSC result directory"));
    assert!(
        path.is_dir(),
        "{variable} is not a directory: {}",
        path.display()
    );
    path
}

fn parsed_op2(path: &Path) -> Op2 {
    let bytes =
        std::fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    read_op2(&bytes).unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

fn scratch_dir() -> PathBuf {
    let base = std::env::var_os("ALAS_MSC_SCRATCH")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new("C:/nas-run").to_path_buf());
    let path = base.join(format!("alas_product_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("create MSC scratch directory");
    path
}
