// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Retained MSC Nastran evidence for Q-0004.
//!
//! This opt-in campaign is intentionally separate from the bounded F-0004
//! random-vibration run.  It varies the structural mesh, solves linear static
//! and normal-mode cases at each resolution, and varies the SOL 111 frequency
//! step on the finest mesh.  The raw decks and solver products remain beside
//! the summaries so a reviewer can reproduce every reported number.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod support;

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use alas_config::{DesignRequirements, EngineConfig, MassModelConfig, StructuresConfig};
use alas_struct::mesh::{build_wing_mesh_bdf, MeshNodeIndex};
use alas_struct::nastran::{
    build_sol101_bulk, build_sol103_bulk, build_sol111_sine_bulk_msc, fatal_lines,
    read_force_psd_rms, read_harmonic_response, run_nastran_with_solver,
};
use alas_struct::op2::{read_op2, Op2};
use alas_struct::sizing::size_wingbox;
use serde_json::{json, Map, Value};
use support::{build_geometry, materials_for, MaterialsRecord};

/// Coarse-to-fine resolutions.  The last row is the retained F-0004 mesh.
const MESH_REFINEMENTS: [(i64, i64); 4] = [(5, 6), (9, 10), (16, 20), (24, 30)];
const FREQUENCY_STEPS_HZ: [f64; 3] = [0.125, 0.0625, 0.03125];
const FREQUENCY_MAX_HZ: f64 = 60.0;

#[test]
#[ignore = "requires ALAS_MSC_LAUNCHER and ALAS_MSC_SOLVER; writes retained Q-0004 evidence"]
fn q0004_msc_mesh_and_frequency_convergence_campaign() {
    let launcher = required_file("ALAS_MSC_LAUNCHER");
    let solver = required_file("ALAS_MSC_SOLVER");
    let output = campaign_output();
    std::fs::create_dir_all(&output).expect("create Q-0004 campaign directory");

    let requirements = DesignRequirements::default();
    let geometry = build_geometry(&[0.15, 0.60], &[true, true]);
    let named = MaterialsRecord {
        skin: "Al 7075-T6".to_owned(),
        web: "Al 7075-T6".to_owned(),
        cap: "CFRP UD".to_owned(),
        rib: "Al 7075-T6".to_owned(),
    };
    let [skin, web, cap, rib] = materials_for(&named);

    let common = StructuresConfig {
        n_modes: 30,
        freq_sweep_max_hz: FREQUENCY_MAX_HZ,
        freq_step_hz: 1.0,
        modal_damping_ratio: 0.02,
        random_force_psd_n2_per_hz: 1.0,
        run_sol_static: false,
        run_sol_modes: false,
        run_sol_vibration_sine: false,
        run_sol_vibration_random: false,
        timeout_s: 600.0,
        ..StructuresConfig::default()
    };

    let mut mesh_cases = Vec::new();
    let mut failures = Vec::new();
    let mut fine_inputs = None;
    for &(num_ribs, chordwise) in &MESH_REFINEMENTS {
        let label = format!("ribs{num_ribs}_cw{chordwise}");
        let case_dir = output.join(format!("mesh-{label}"));
        std::fs::create_dir_all(&case_dir).expect("create mesh case directory");
        let mut config = common.clone();
        config.num_ribs_override = Some(num_ribs);
        config.mesh_chordwise_points = chordwise;
        let sizing = size_wingbox(&geometry, &config, &requirements, skin, web, cap, rib);
        let (deck, health, node_index) = build_wing_mesh_bdf(
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
        .unwrap_or_else(|error| panic!("{label}: build mesh: {error}"));
        let mesh_text = deck.write_bulk_msc();
        let mesh_path = case_dir.join("wing_mesh.bdf");
        std::fs::write(&mesh_path, &mesh_text).expect("write mesh BDF");

        let mut record = json!({
            "refinement": label,
            "num_ribs": num_ribs,
            "mesh_chordwise_points": chordwise,
            "mesh": {
                "nodes": deck.grids().len(),
                "elements": deck.element_count(),
                "cquad4": deck.quads().len(),
                "ctria3": deck.trias().len(),
                "health_ok": health.ok(),
                "warping_bad": health.n_warping_bad,
                "rbe3": health.rbe3_count,
            },
            "mesh_bdf": "wing_mesh.bdf",
            "static": {"status": "not_run"},
            "modal": {"status": "not_run"},
        });
        if !health.ok() {
            failures.push(format!("{label}: mesh health reported warped panels"));
        }

        let static_bdf = case_dir.join("wing_sol101.bdf");
        std::fs::write(
            &static_bdf,
            build_sol101_bulk(&deck, &node_index, &requirements, &config, "wing_mesh.bdf"),
        )
        .expect("write SOL 101 deck");
        match solve_and_parse(&static_bdf, &launcher, &solver, config.timeout_s) {
            Ok(op2) => match static_metrics(&op2, &node_index) {
                Ok(metrics) => record["static"] = metrics,
                Err(error) => {
                    failures.push(format!("{label}: static results: {error}"));
                    record["static"] = json!({"status": "error", "error": error});
                }
            },
            Err(error) => {
                failures.push(format!("{label}: SOL 101: {error}"));
                record["static"] = json!({"status": "error", "error": error});
            }
        }

        let modal_bdf = case_dir.join("wing_sol103.bdf");
        std::fs::write(&modal_bdf, build_sol103_bulk(&config, "wing_mesh.bdf"))
            .expect("write SOL 103 deck");
        match solve_and_parse(&modal_bdf, &launcher, &solver, config.timeout_s) {
            Ok(op2) => match modal_metrics(&op2) {
                Ok(metrics) => record["modal"] = metrics,
                Err(error) => {
                    failures.push(format!("{label}: modal results: {error}"));
                    record["modal"] = json!({"status": "error", "error": error});
                }
            },
            Err(error) => {
                failures.push(format!("{label}: SOL 103: {error}"));
                record["modal"] = json!({"status": "error", "error": error});
            }
        }

        if (num_ribs, chordwise) == *MESH_REFINEMENTS.last().unwrap() {
            fine_inputs = Some((node_index, config));
        }
        mesh_cases.push(record);
    }

    let (fine_node_index, fine_config) = fine_inputs.expect("the refinement list has a fine mesh");
    let monitors = alas_struct::nastran::monitor_set(&fine_node_index);
    let &(fine_ribs, fine_chordwise) = MESH_REFINEMENTS.last().unwrap();
    let fine_mesh_include = format!("../mesh-ribs{fine_ribs}_cw{fine_chordwise}/wing_mesh.bdf");
    let mut frequency_cases = Vec::new();
    for &step_hz in &FREQUENCY_STEPS_HZ {
        let label = format!("step{step_hz:.5}_hz");
        let case_dir = output.join(format!("frequency-{label}"));
        std::fs::create_dir_all(&case_dir).expect("create frequency case directory");
        let mut config = fine_config.clone();
        config.freq_step_hz = step_hz;
        let deck_text = build_sol111_sine_bulk_msc(&config, &fine_node_index, &fine_mesh_include);
        let bdf = case_dir.join("wing_sol111_sine.bdf");
        std::fs::write(&bdf, deck_text).expect("write SOL 111 deck");
        let mut record = json!({
            "frequency_step_hz": step_hz,
            "frequency_max_hz": config.freq_sweep_max_hz,
            "bdf": "wing_sol111_sine.bdf",
        });
        match solve_and_parse(&bdf, &launcher, &solver, config.timeout_s) {
            Ok(op2) => match frequency_metrics(&op2, monitors, config.random_force_psd_n2_per_hz) {
                Ok(metrics) => record["results"] = metrics,
                Err(error) => {
                    failures.push(format!("{label}: frequency results: {error}"));
                    record["results"] = json!({"status": "error", "error": error});
                }
            },
            Err(error) => {
                failures.push(format!("{label}: SOL 111: {error}"));
                record["results"] = json!({"status": "error", "error": error});
            }
        }
        frequency_cases.push(record);
    }

    let assessment = convergence_assessment(&mesh_cases, &frequency_cases);
    let manifest = json!({
        "finding": "Q-0004",
        "campaign": "MSC Nastran mesh and frequency convergence evidence",
        "created_utc_epoch_s": now_epoch(),
        "solver_launcher": launcher.display().to_string(),
        "solver_override": solver.display().to_string(),
        "scope": {
            "mesh": "SOL 101 linear static and SOL 103 normal modes at four mesh resolutions",
            "frequency": "SOL 111 unit-force receptance at three frequency steps on the fine mesh",
            "materials": {
                "skin": named.skin,
                "web": named.web,
                "cap": named.cap,
                "rib": named.rib,
            },
            "spar_chord_fractions": [0.15, 0.60],
            "modal_damping_ratio": common.modal_damping_ratio,
            "frequency_max_hz": FREQUENCY_MAX_HZ,
        },
        "mesh_cases": mesh_cases,
        "frequency_cases": frequency_cases,
        "convergence_assessment": assessment,
        "limitations": [
            "This is solver-backed structural evidence, not certification or a coupled aeroelastic validation.",
            "Patran Analysis Manager is not licensed on the host; Patran import/render evidence is retained separately.",
            "No applicable physical strain, modal-test, or load-test data were available in the workspace for correlation.",
            "FLOWUnsteady was not executable on the host because no Julia/FLOWUnsteady runtime was found.",
        ],
        "failures": failures,
    });
    std::fs::write(
        output.join("campaign_manifest.json"),
        serde_json::to_string_pretty(&manifest).expect("serialize Q-0004 manifest"),
    )
    .expect("write Q-0004 manifest");
    std::fs::write(
        output.join("mesh_cases.json"),
        serde_json::to_string_pretty(&mesh_cases).expect("serialize mesh cases"),
    )
    .expect("write mesh cases");
    std::fs::write(
        output.join("frequency_cases.json"),
        serde_json::to_string_pretty(&frequency_cases).expect("serialize frequency cases"),
    )
    .expect("write frequency cases");
    write_readme(&output, &launcher, &solver, &assessment, &failures);

    assert!(
        failures.is_empty(),
        "Q-0004 MSC campaign had solver/data failures; see {}",
        output.join("campaign_manifest.json").display()
    );
    assert_assessment(&assessment);
}

fn solve_and_parse(
    bdf: &Path,
    launcher: &Path,
    solver: &Path,
    timeout_s: f64,
) -> Result<Op2, String> {
    let outcome = run_nastran_with_solver(bdf, launcher, Some(solver), timeout_s);
    if !outcome.ok {
        return Err(outcome.detail);
    }
    let f06_path = bdf.with_extension("f06");
    let f06 = std::fs::read_to_string(&f06_path)
        .map_err(|error| format!("read {}: {error}", f06_path.display()))?;
    if !fatal_lines(&f06).is_empty() || !f06.contains("* * * END OF JOB * * *") {
        return Err(format!(
            "{} is not a clean MSC end-of-job",
            f06_path.display()
        ));
    }
    let op2_path = bdf.with_extension("op2");
    let bytes = std::fs::read(&op2_path)
        .map_err(|error| format!("read {}: {error}", op2_path.display()))?;
    read_op2(&bytes).map_err(|error| format!("parse {}: {error}", op2_path.display()))
}

fn static_metrics(op2: &Op2, node_index: &MeshNodeIndex) -> Result<Value, String> {
    let mut tip_t3 = Vec::new();
    let mut peak_von_mises: f64 = 0.0;
    for subcase in 1..=3 {
        let displacement = op2
            .displacements
            .get(&subcase)
            .ok_or_else(|| format!("missing displacement subcase {subcase}"))?;
        if !displacement
            .data
            .iter()
            .flatten()
            .all(|value| value.is_finite())
        {
            return Err(format!("non-finite displacement in subcase {subcase}"));
        }
        let tip = displacement
            .node_ids
            .iter()
            .position(|&node| node == node_index.tip_nid)
            .ok_or_else(|| {
                format!(
                    "tip grid {} missing in subcase {subcase}",
                    node_index.tip_nid
                )
            })?;
        tip_t3.push(displacement.data[tip][2]);
        let stress = op2
            .cquad4_stress
            .get(&subcase)
            .ok_or_else(|| format!("missing CQUAD4 stress subcase {subcase}"))?;
        if stress.data.is_empty() || !stress.data.iter().flatten().all(|value| value.is_finite()) {
            return Err(format!("empty or non-finite stress in subcase {subcase}"));
        }
        peak_von_mises = peak_von_mises.max(
            stress
                .data
                .iter()
                .map(|row| row[7].abs())
                .fold(0.0, f64::max),
        );
    }
    Ok(json!({
        "status": "ok",
        "tip_t3_m": tip_t3,
        "tip_deflection_subcase_1_m": tip_t3[0].abs(),
        "peak_von_mises_pa": peak_von_mises,
    }))
}

fn modal_metrics(op2: &Op2) -> Result<Value, String> {
    let modes = op2
        .eigenvectors
        .get(&1)
        .ok_or_else(|| "missing modal subcase 1".to_owned())?;
    if modes.mode_cycles.is_empty()
        || !modes
            .mode_cycles
            .iter()
            .all(|frequency| frequency.is_finite() && *frequency > 0.0)
        || !modes.mode_cycles.windows(2).all(|pair| pair[0] < pair[1])
    {
        return Err(
            "modal frequencies are empty, non-positive, non-finite, or unordered".to_owned(),
        );
    }
    Ok(json!({
        "status": "ok",
        "mode_count": modes.mode_cycles.len(),
        "first_mode_hz": modes.mode_cycles[0],
        "last_mode_hz": modes.mode_cycles.last().copied(),
        "frequencies_hz": modes.mode_cycles,
    }))
}

fn frequency_metrics(
    op2: &Op2,
    monitors: alas_struct::nastran::MonitorNodes,
    psd: f64,
) -> Result<Value, String> {
    let table = op2
        .complex_displacements
        .get(&1)
        .ok_or_else(|| "missing complex displacement subcase 1".to_owned())?;
    if table.freqs.len() < 2 || !table.freqs.windows(2).all(|pair| pair[0] < pair[1]) {
        return Err("frequency grid is missing or unordered".to_owned());
    }
    let harmonic = read_harmonic_response(op2, monitors);
    let rms = read_force_psd_rms(op2, monitors, psd).map_err(|error| error.to_string())?;
    let rms_object = rms
        .iter()
        .map(|(label, value)| (label.to_owned(), Value::from(value)))
        .collect::<Map<String, Value>>();
    let tip_rms = rms
        .get("tip")
        .ok_or_else(|| "RMS report has no tip monitor".to_owned())?;
    if !tip_rms.is_finite() || !harmonic.peak_frf.is_finite() {
        return Err("non-finite harmonic/RMS result".to_owned());
    }
    Ok(json!({
        "status": "ok",
        "frequency_points": table.freqs.len(),
        "frequency_first_hz": table.freqs.first().copied(),
        "frequency_last_hz": table.freqs.last().copied(),
        "rms_m": rms_object,
        "tip_rms_m": tip_rms,
        "peak_frequency_hz": harmonic.peak_freq_hz,
        "peak_tip_frf_m_per_n": harmonic.peak_frf,
    }))
}

fn convergence_assessment(mesh_cases: &[Value], frequency_cases: &[Value]) -> Value {
    let mesh_change = |from: &str, to: &str, key: &str| {
        let value = |label: &str| {
            mesh_cases
                .iter()
                .find(|case| case.get("refinement").and_then(Value::as_str) == Some(label))
                .and_then(|case| value_at_path(case, key))
        };
        percent_change(value(from), value(to))
    };
    let frequency_change = |from: f64, to: f64, key: &str| {
        let value = |step: f64| {
            frequency_cases
                .iter()
                .find(|case| case.get("frequency_step_hz").and_then(Value::as_f64) == Some(step))
                .and_then(|case| case.get("results"))
                .and_then(|result| result.get(key))
                .and_then(Value::as_f64)
        };
        percent_change(value(from), value(to))
    };
    json!({
        "mesh_rule": "the fine-versus-medium change is reported for static tip displacement, peak von Mises stress, and first modal frequency; all are finite solver outputs",
        "mesh": {
            "coarse_to_medium_tip_deflection_pct": mesh_change("ribs5_cw6", "ribs9_cw10", "static.tip_deflection_subcase_1_m"),
            "medium_to_fine_tip_deflection_pct": mesh_change("ribs16_cw20", "ribs24_cw30", "static.tip_deflection_subcase_1_m"),
            "coarse_to_medium_peak_von_mises_pct": mesh_change("ribs5_cw6", "ribs9_cw10", "static.peak_von_mises_pa"),
            "medium_to_fine_peak_von_mises_pct": mesh_change("ribs16_cw20", "ribs24_cw30", "static.peak_von_mises_pa"),
            "coarse_to_medium_first_mode_pct": mesh_change("ribs5_cw6", "ribs9_cw10", "modal.first_mode_hz"),
            "medium_to_fine_first_mode_pct": mesh_change("ribs16_cw20", "ribs24_cw30", "modal.first_mode_hz"),
            "static_tip_deflection_gate_pct": 10.0,
            "peak_von_mises_gate_pct": 10.0,
            "modal_note": "first positive modal roots are retained, but first-root convergence is diagnostic only because mesh refinement reorders local modes",
        },
        "frequency_rule": "the 0.03125 Hz sweep is the reference; the 0.0625 Hz and 0.125 Hz relative changes are reported for tip RMS and peak tip FRF",
        "frequency": {
            "0_0625hz_to_0_03125hz_tip_rms_pct": frequency_change(0.0625, 0.03125, "tip_rms_m"),
            "0_125hz_to_0_03125hz_tip_rms_pct": frequency_change(0.125, 0.03125, "tip_rms_m"),
            "0_0625hz_to_0_03125hz_peak_frf_pct": frequency_change(0.0625, 0.03125, "peak_tip_frf_m_per_n"),
            "0_125hz_to_0_03125hz_peak_frf_pct": frequency_change(0.125, 0.03125, "peak_tip_frf_m_per_n"),
            "tip_rms_gate_pct": 5.0,
            "peak_frf_gate_pct": 5.0,
        },
    })
}

fn assert_assessment(assessment: &Value) {
    let mesh = assessment.get("mesh").expect("mesh assessment");
    let frequency = assessment.get("frequency").expect("frequency assessment");
    for key in [
        "medium_to_fine_tip_deflection_pct",
        "medium_to_fine_peak_von_mises_pct",
        "medium_to_fine_first_mode_pct",
    ] {
        assert!(
            mesh.get(key)
                .and_then(Value::as_f64)
                .is_some_and(f64::is_finite),
            "mesh assessment {key} is not finite: {assessment}"
        );
    }
    assert!(
        mesh.get("medium_to_fine_tip_deflection_pct")
            .and_then(Value::as_f64)
            .is_some_and(|value| value <= 10.0),
        "static mesh tip-deflection gate failed: {assessment}"
    );
    assert!(
        mesh.get("medium_to_fine_peak_von_mises_pct")
            .and_then(Value::as_f64)
            .is_some_and(|value| value <= 10.0),
        "static mesh stress gate failed: {assessment}"
    );
    for key in [
        "0_0625hz_to_0_03125hz_tip_rms_pct",
        "0_125hz_to_0_03125hz_tip_rms_pct",
        "0_0625hz_to_0_03125hz_peak_frf_pct",
        "0_125hz_to_0_03125hz_peak_frf_pct",
    ] {
        assert!(
            frequency
                .get(key)
                .and_then(Value::as_f64)
                .is_some_and(f64::is_finite),
            "frequency assessment {key} is not finite: {assessment}"
        );
    }
    for key in [
        "0_0625hz_to_0_03125hz_tip_rms_pct",
        "0_125hz_to_0_03125hz_tip_rms_pct",
        "0_0625hz_to_0_03125hz_peak_frf_pct",
        "0_125hz_to_0_03125hz_peak_frf_pct",
    ] {
        assert!(
            frequency
                .get(key)
                .and_then(Value::as_f64)
                .is_some_and(|value| value <= 5.0),
            "frequency convergence gate failed for {key}: {assessment}"
        );
    }
}

fn percent_change(from: Option<f64>, to: Option<f64>) -> Value {
    match (from, to) {
        (Some(from), Some(to)) if from.is_finite() && to.is_finite() && from.abs() > 1e-30 => {
            Value::from(100.0 * (to / from - 1.0).abs())
        }
        _ => Value::Null,
    }
}

fn value_at_path(value: &Value, path: &str) -> Option<f64> {
    let mut current = value;
    for key in path.split('.') {
        current = current.get(key)?;
    }
    current.as_f64()
}

fn write_readme(
    output: &Path,
    launcher: &Path,
    solver: &Path,
    assessment: &Value,
    failures: &[String],
) {
    let status = if failures.is_empty() { "PASS" } else { "FAIL" };
    let text = format!(
        concat!(
            "# Q-0004 MSC Nastran convergence campaign\n\nStatus: **{}**\n\n",
            "MSC launcher: `{}`\nMSC solver override: `{}`\n\n",
            "The campaign solves SOL 101 and SOL 103 at ribs/chordwise resolutions ",
            "(5,6), (9,10), (16,20), and (24,30), then solves SOL 111 on the fine mesh at ",
            "0.125, 0.0625, and 0.03125 Hz frequency steps through 60 Hz. See ",
            "`campaign_manifest.json` for raw-result paths and the assessment.\n\n",
            "Assessment:\n```json\n{}\n```\n\nFailures: {:?}\n"
        ),
        status,
        launcher.display(),
        solver.display(),
        serde_json::to_string_pretty(assessment).expect("serialize README assessment"),
        failures,
    );
    std::fs::write(output.join("README.md"), text).expect("write Q-0004 README");
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

fn campaign_output() -> PathBuf {
    let root = std::env::var_os("ALAS_Q0004_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new("audit-artifacts/q0004-msc-convergence").to_path_buf());
    root.join(format!("campaign-{}-{}", now_epoch(), std::process::id()))
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
