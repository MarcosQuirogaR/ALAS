// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Retained MSC Student Edition evidence for F-0004.
//!
//! This is deliberately an ignored, opt-in test.  It runs the product's
//! random-vibration route: a SOL 111 unit-force receptance sweep followed by
//! integration against a flat, one-sided force PSD.  Four otherwise identical
//! cases stop at 60, 120, 240 and 500 Hz.  Every deck and solver result stays
//! under `ALAS_MSC_F0004_OUTPUT`; a solver failure is written into the manifest
//! before the test reports failure, so a blocked physical claim is auditable.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod support;

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use alas_config::{DesignRequirements, EngineConfig, MassModelConfig, StructuresConfig};
use alas_struct::mesh::build_wing_mesh_bdf;
use alas_struct::nastran::{
    build_sol103_bulk, build_sol111_random_bulk, build_sol111_sine_bulk_msc, fatal_lines,
    read_force_psd_rms, run_nastran_with_solver,
};
use alas_struct::op2::read_op2;
use alas_struct::sizing::size_wingbox;
use serde_json::{json, Map, Value};
use support::{build_geometry, materials_for, MaterialsRecord};

const CAMPAIGN_HZ: [f64; 4] = [60.0, 120.0, 240.0, 500.0];
const MONITORS: [&str; 4] = ["root", "kink", "engine", "tip"];

#[test]
#[ignore = "requires ALAS_MSC_LAUNCHER and ALAS_MSC_SOLVER; writes retained F-0004 evidence"]
fn msc_force_psd_rms_convergence_campaign_60_120_240_500_hz() {
    let launcher = required_file("ALAS_MSC_LAUNCHER");
    let solver = required_file("ALAS_MSC_SOLVER");
    let output = campaign_output();
    std::fs::create_dir_all(&output).expect("create F-0004 campaign directory");

    // Fixed mesh/material/damping/PSD controls.  Only freq_sweep_max_hz is
    // varied between cases, so differences are cumulative frequency-band
    // effects rather than remeshing or input-spectrum changes.
    let base = StructuresConfig {
        num_ribs_override: Some(16),
        mesh_chordwise_points: 20,
        n_modes: 30,
        freq_step_hz: 1.0,
        modal_damping_ratio: 0.02,
        random_force_psd_n2_per_hz: 1.0,
        run_sol_static: false,
        run_sol_modes: false,
        run_sol_vibration_sine: false,
        run_sol_vibration_random: true,
        timeout_s: 600.0,
        ..StructuresConfig::default()
    };
    let requirements = DesignRequirements::default();
    let geometry = build_geometry(&[0.15, 0.60], &[true, true]);
    let named = MaterialsRecord {
        skin: "Al 7075-T6".to_owned(),
        web: "Al 7075-T6".to_owned(),
        cap: "CFRP UD".to_owned(),
        rib: "Al 7075-T6".to_owned(),
    };
    let [skin, web, cap, rib] = materials_for(&named);
    let sizing = size_wingbox(&geometry, &base, &requirements, skin, web, cap, rib);
    let (deck, _, node_index) = build_wing_mesh_bdf(
        &geometry,
        &sizing,
        &base,
        &EngineConfig::default(),
        &MassModelConfig::default(),
        &requirements,
        skin,
        web,
        cap,
        rib,
    )
    .expect("the controlled campaign mesh is healthy");
    let mesh_path = output.join("wing_mesh.bdf");
    std::fs::write(&mesh_path, deck.write_bulk_msc()).expect("write controlled campaign mesh");

    let monitors = alas_struct::nastran::monitor_set(&node_index);
    let mut cases = Vec::new();
    let mut failures = Vec::new();
    for &upper_hz in &CAMPAIGN_HZ {
        let case_dir = output.join(format!("case-{upper_hz:.0}hz"));
        std::fs::create_dir_all(&case_dir).expect("create frequency case directory");
        let mut config = base.clone();
        config.freq_sweep_max_hz = upper_hz;
        let sine_deck = build_sol111_sine_bulk_msc(&config, &node_index, "../wing_mesh.bdf");
        let random_deck = build_sol111_random_bulk(&config, &node_index, "../wing_mesh.bdf");
        std::fs::write(case_dir.join("wing_sol111_sine.bdf"), &sine_deck)
            .expect("write MSC unit-force SOL 111 deck");
        // Keep the native random deck beside the solved product deck so the
        // distinction between deck generation and the force-PSD RMS contract
        // is visible to an auditor.
        std::fs::write(case_dir.join("wing_sol111_random_native.bdf"), random_deck)
            .expect("write native random SOL 111 deck");

        let bdf = case_dir.join("wing_sol111_sine.bdf");
        let outcome = run_nastran_with_solver(&bdf, &launcher, Some(&solver), config.timeout_s);
        let mut record = json!({
            "upper_hz_requested": upper_hz,
            "frequency_step_hz": config.freq_step_hz,
            "damping_ratio": config.modal_damping_ratio,
            "force_psd_n2_per_hz": config.random_force_psd_n2_per_hz,
            "sweep_policy": "explicit configured upper frequency; no implicit cap",
            "solver_ok": outcome.ok,
            "solver_detail": outcome.detail,
            "bdf": "wing_sol111_sine.bdf",
            "native_random_bdf": "wing_sol111_random_native.bdf",
            "f06": "wing_sol111_sine.f06",
            "op2": "wing_sol111_sine.op2",
        });
        if !outcome.ok {
            failures.push(format!("{upper_hz:.0} Hz: {}", outcome.detail));
            cases.push(record);
            continue;
        }

        let f06_path = bdf.with_extension("f06");
        let f06 = std::fs::read_to_string(&f06_path).unwrap_or_else(|error| {
            failures.push(format!("{upper_hz:.0} Hz: cannot read f06: {error}"));
            String::new()
        });
        let f06_clean = fatal_lines(&f06).is_empty() && f06.contains("* * * END OF JOB * * *");
        record["f06_clean"] = Value::Bool(f06_clean);
        record["f06_bytes"] = Value::from(f06.len() as u64);
        record["f06_user_warning_count"] = Value::from(
            f06.lines()
                .filter(|line| line.contains("*** USER WARNING MESSAGE"))
                .count() as u64,
        );
        if !f06_clean {
            failures.push(format!("{upper_hz:.0} Hz: f06 lacks a clean end-of-job"));
            cases.push(record);
            continue;
        }

        let op2_path = bdf.with_extension("op2");
        let bytes = std::fs::read(&op2_path).unwrap_or_else(|error| {
            failures.push(format!("{upper_hz:.0} Hz: cannot read op2: {error}"));
            Vec::new()
        });
        record["op2_bytes"] = Value::from(bytes.len() as u64);
        let op2 = match read_op2(&bytes) {
            Ok(op2) => op2,
            Err(error) => {
                failures.push(format!("{upper_hz:.0} Hz: cannot parse op2: {error}"));
                cases.push(record);
                continue;
            }
        };
        let Some(table) = op2.complex_displacements.get(&1) else {
            failures.push(format!(
                "{upper_hz:.0} Hz: OP2 has no complex displacement table"
            ));
            cases.push(record);
            continue;
        };
        let rms = match read_force_psd_rms(&op2, monitors, config.random_force_psd_n2_per_hz) {
            Ok(rms) => rms,
            Err(error) => {
                failures.push(format!("{upper_hz:.0} Hz: RMS integration failed: {error}"));
                cases.push(record);
                continue;
            }
        };
        let rms_object = rms
            .iter()
            .map(|(label, value)| (label.to_owned(), Value::from(value)))
            .collect::<Map<String, Value>>();
        record["frequency_points"] = Value::from(table.freqs.len() as u64);
        record["frequency_first_hz"] = table
            .freqs
            .first()
            .copied()
            .map_or(Value::Null, Value::from);
        record["frequency_last_hz"] = table.freqs.last().copied().map_or(Value::Null, Value::from);
        record["monitor_node_ids"] = json!({
            "root": monitors.root,
            "kink": monitors.kink,
            "engine": monitors.engine,
            "tip": monitors.tip,
        });
        record["rms_m"] = Value::Object(rms_object);
        record["status"] = Value::String("ok".to_owned());
        cases.push(record);
    }

    // A modal solve uses the same mesh and controls, and records which elastic
    // roots each frequency cap can include.  It is diagnostic evidence, not a
    // substitution for the swept response integral.
    let modal = run_modal_case(&output, &deck, &base, &launcher, &solver);
    let convergence_assessment = build_convergence_assessment(&cases);
    let manifest = json!({
        "finding": "F-0004",
        "campaign": "MSC Student Edition force-PSD random-vibration RMS convergence",
        "created_utc_epoch_s": now_epoch(),
        "solver_launcher": launcher.display().to_string(),
        "solver_override": solver.display().to_string(),
        "frequency_cap_policy": {
            "mode": "explicit_configured_band",
            "start_hz": base.freq_step_hz,
            "requested_upper_hz": CAMPAIGN_HZ,
            "implicit_cap_hz": Value::Null,
            "rationale": "A band-limited RMS integral must not silently discard non-negative variance above an engineering cap."
        },
        "controls": {
            "num_ribs": base.num_ribs_override,
            "mesh_chordwise_points": base.mesh_chordwise_points,
            "spar_chord_fractions": [0.15, 0.60],
            "materials": {
                "skin": named.skin,
                "web": named.web,
                "cap": named.cap,
                "rib": named.rib,
            },
            "modal_damping_ratio": base.modal_damping_ratio,
            "force_psd_n2_per_hz": base.random_force_psd_n2_per_hz,
            "frequency_step_hz": base.freq_step_hz,
            "n_modes": base.n_modes,
            "excitation": "1 N unit harmonic force at first engine monitor; flat one-sided force PSD for RMS integration",
        },
        "method": "Each SOL 111 unit-force receptance is integrated as variance = integral(|H(f)|^2 * S_F df), then square-rooted per monitor. The native RANDOM deck is retained for traceability; cumulative RMS numbers use the product force-PSD contract.",
        "mesh": "wing_mesh.bdf",
        "cases": cases,
        "convergence_assessment": convergence_assessment,
        "modal_diagnostic": modal,
        "failures": failures,
    });
    std::fs::write(
        output.join("campaign_manifest.json"),
        serde_json::to_string_pretty(&manifest).expect("serialize campaign manifest"),
    )
    .expect("write campaign manifest");
    write_rms_tables(&output, &cases);
    std::fs::write(
        output.join("convergence_assessment.json"),
        serde_json::to_string_pretty(&build_convergence_assessment(&cases))
            .expect("serialize convergence assessment"),
    )
    .expect("write convergence assessment");
    write_readme(&output, &launcher, &solver, &base, &failures);

    assert!(
        failures.is_empty(),
        "F-0004 MSC campaign had solver/data failures; see {}",
        output.join("campaign_manifest.json").display()
    );
}

/// Quantify cumulative RMS convergence against the largest solved band.  A
/// 99% variance-capture threshold is a reporting convention for this campaign,
/// not a certification margin; it makes an implicit cap decision visible.
fn build_convergence_assessment(cases: &[Value]) -> Value {
    let successful = cases
        .iter()
        .filter(|case| case.get("status").and_then(Value::as_str) == Some("ok"))
        .collect::<Vec<_>>();
    let reference = successful
        .iter()
        .find(|case| case.get("upper_hz_requested").and_then(Value::as_f64) == Some(500.0));
    let mut monitors = Map::new();
    for label in MONITORS {
        let reference_rms = reference
            .and_then(|case| case.get("rms_m"))
            .and_then(Value::as_object)
            .and_then(|values| values.get(label))
            .and_then(Value::as_f64);
        let mut bands = Map::new();
        for case in &successful {
            let Some(upper) = case.get("upper_hz_requested").and_then(Value::as_f64) else {
                continue;
            };
            let rms = case
                .get("rms_m")
                .and_then(Value::as_object)
                .and_then(|values| values.get(label))
                .and_then(Value::as_f64);
            let Some(rms) = rms else { continue };
            let capture = reference_rms
                .filter(|reference| reference.is_finite() && *reference > 0.0)
                .map_or(Value::Null, |reference| {
                    Value::from(100.0 * (rms / reference).powi(2))
                });
            let rms_delta = reference_rms
                .filter(|reference| reference.is_finite() && *reference > 0.0)
                .map_or(Value::Null, |reference| {
                    Value::from(100.0 * (rms / reference - 1.0))
                });
            bands.insert(
                format!("{upper:.0}hz"),
                json!({
                    "rms_m": rms,
                    "variance_capture_pct_of_500": capture,
                    "rms_delta_pct_of_500": rms_delta,
                }),
            );
        }
        let converged_at = successful.iter().find_map(|case| {
            let upper = case.get("upper_hz_requested").and_then(Value::as_f64)?;
            let rms = case
                .get("rms_m")
                .and_then(Value::as_object)
                .and_then(|values| values.get(label))
                .and_then(Value::as_f64)?;
            let reference =
                reference_rms.filter(|reference| reference.is_finite() && *reference > 0.0)?;
            ((rms / reference).powi(2) >= 0.99).then_some(upper)
        });
        let increment_240_to_500_pct = variance_increment_pct(&successful, label, 240.0, 500.0);
        monitors.insert(
            label.to_owned(),
            json!({
                "reference_rms_500_m": reference_rms,
                "variance_capture_threshold_pct": 99.0,
                "band_converged_at_hz": converged_at,
                "variance_increment_240_to_500_pct_of_500": increment_240_to_500_pct,
                "bands": bands,
            }),
        );
    }
    json!({
        "reference_upper_hz": 500.0,
        "rule": "band is called converged for a monitor when cumulative variance reaches at least 99% of the 500 Hz result; this is a campaign reporting threshold, not a certification margin",
        "monitors": monitors,
    })
}

fn variance_increment_pct(cases: &[&Value], label: &str, lower_hz: f64, upper_hz: f64) -> Value {
    let variance = |upper: f64| {
        cases
            .iter()
            .find(|case| case.get("upper_hz_requested").and_then(Value::as_f64) == Some(upper))
            .and_then(|case| case.get("rms_m"))
            .and_then(Value::as_object)
            .and_then(|values| values.get(label))
            .and_then(Value::as_f64)
            .map(|rms| rms * rms)
    };
    match (variance(lower_hz), variance(upper_hz)) {
        (Some(lower), Some(upper)) if upper.is_finite() && upper > 0.0 => {
            Value::from(100.0 * (upper - lower) / upper)
        }
        _ => Value::Null,
    }
}

fn run_modal_case(
    output: &Path,
    deck: &alas_struct::mesh::Deck,
    config: &StructuresConfig,
    launcher: &Path,
    solver: &Path,
) -> Value {
    let modal_dir = output.join("modal");
    std::fs::create_dir_all(&modal_dir).expect("create modal directory");
    let bdf = modal_dir.join("wing_sol103.bdf");
    std::fs::write(&bdf, build_sol103_bulk(config, "../wing_mesh.bdf"))
        .expect("write SOL 103 deck");
    let outcome = run_nastran_with_solver(&bdf, launcher, Some(solver), config.timeout_s);
    let mut record = json!({
        "solver_ok": outcome.ok,
        "solver_detail": outcome.detail,
        "bdf": "wing_sol103.bdf",
        "f06": "wing_sol103.f06",
        "op2": "wing_sol103.op2",
    });
    if !outcome.ok {
        return record;
    }
    let f06 = std::fs::read_to_string(bdf.with_extension("f06")).unwrap_or_default();
    record["f06_clean"] =
        Value::Bool(fatal_lines(&f06).is_empty() && f06.contains("* * * END OF JOB * * *"));
    let bytes = std::fs::read(bdf.with_extension("op2")).unwrap_or_default();
    match read_op2(&bytes) {
        Ok(op2) => {
            if let Some(eigenvectors) = op2.eigenvectors.get(&1) {
                record["mode_count"] = Value::from(eigenvectors.mode_cycles.len() as u64);
                record["frequencies_hz"] = json!(eigenvectors.mode_cycles);
                record["modes_in_each_band"] = json!(CAMPAIGN_HZ.map(|upper| {
                    eigenvectors
                        .mode_cycles
                        .iter()
                        .filter(|&&frequency| frequency <= upper)
                        .count()
                }));
            }
        }
        Err(error) => record["op2_error"] = Value::String(error.to_string()),
    }
    // Keep the function's `deck` argument in the signature as a deliberate
    // reminder that the modal result belongs to the exact campaign mesh.
    let _ = deck.node_y(1);
    record
}

fn write_rms_tables(output: &Path, cases: &[Value]) {
    let mut csv = String::from(
        "upper_hz,frequency_points,first_hz,last_hz,root_rms_m,kink_rms_m,engine_rms_m,tip_rms_m,status\n",
    );
    for case in cases {
        let upper = number(case, "upper_hz_requested");
        let points = number(case, "frequency_points");
        let first = number(case, "frequency_first_hz");
        let last = number(case, "frequency_last_hz");
        let rms = case.get("rms_m").and_then(Value::as_object);
        let fields = MONITORS.map(|label| {
            rms.and_then(|values| values.get(label))
                .and_then(Value::as_f64)
        });
        let values = fields.map(|value| value.map_or(String::new(), |x| format!("{x:.16e}")));
        csv.push_str(&format!(
            "{upper},{points},{first},{last},{},{},{},{},{}\n",
            values[0],
            values[1],
            values[2],
            values[3],
            case.get("status")
                .and_then(Value::as_str)
                .unwrap_or("error")
        ));
    }
    std::fs::write(output.join("rms_by_band.csv"), csv).expect("write RMS CSV");

    let successful = cases
        .iter()
        .filter_map(|case| {
            let upper = case.get("upper_hz_requested")?.as_f64()?;
            let rms = case.get("rms_m")?.as_object()?;
            let values = MONITORS.map(|label| rms.get(label)?.as_f64());
            Some((upper, values))
        })
        .collect::<Vec<_>>();
    let reference = successful.iter().find(|(upper, _)| *upper == 500.0);
    let mut convergence = String::from(
        "upper_hz,monitor,rms_m,variance_m2,delta_variance_from_previous_m2,delta_variance_pct_of_500\n",
    );
    for (index, (upper, values)) in successful.iter().enumerate() {
        for monitor_index in 0..MONITORS.len() {
            let Some(rms) = values[monitor_index] else {
                continue;
            };
            let variance = rms * rms;
            let previous = index
                .checked_sub(1)
                .and_then(|i| successful.get(i))
                .and_then(|(_, values)| values[monitor_index])
                .map(|value| value * value);
            let delta = previous.map_or(variance, |value| variance - value);
            let denominator = reference
                .and_then(|(_, values)| values[monitor_index])
                .map(|value| value * value)
                .unwrap_or(f64::NAN);
            let pct = if denominator.is_finite() && denominator > 0.0 {
                100.0 * delta / denominator
            } else {
                f64::NAN
            };
            convergence.push_str(&format!(
                "{upper:.0},{},{rms:.16e},{variance:.16e},{delta:.16e},{pct:.8e}\n",
                MONITORS[monitor_index]
            ));
        }
    }
    std::fs::write(output.join("rms_convergence.csv"), convergence)
        .expect("write RMS convergence CSV");
}

fn write_readme(
    output: &Path,
    launcher: &Path,
    solver: &Path,
    config: &StructuresConfig,
    failures: &[String],
) {
    let status = if failures.is_empty() {
        "PASS"
    } else {
        "BLOCKED"
    };
    let text = format!(
        concat!(
            "# F-0004 MSC random-vibration RMS campaign\n\nStatus: **{}**\n\n",
            "`PASS` means all requested MSC solves produced clean end-of-job files and parseable OP2 data; it does not mean a single lower frequency cap is universally converged. See `convergence_assessment.json` for the monitor-specific result.\n\n",
            "This retained campaign uses MSC Student Edition through process-local paths. ",
            "The policy is explicit: the SOL 111 frequency band starts at {:.3} Hz and ",
            "honors each configured upper bound (60/120/240/500 Hz); there is no implicit 60 Hz cap.\n\n",
            "Controls: 16 ribs, 20 chordwise mesh points, 2% critical modal damping, ",
            "one-sided flat force PSD {:.6e} N^2/Hz, {} requested modes, and a 1 Hz grid. ",
            "The mesh, solved unit-force decks/results, and unsolved native RANDOM decks are retained.\n\n",
            "MSC print files contain eight non-fatal USER WARNING 4382 entries per solve (beam-property library fallback); no fatal messages were reported.\n\n",
            "RMS method: integrate each monitor's unit-force receptance as `sqrt(integral(|H(f)|^2 S_F df))`. ",
            "The native RANDOM deck is included for traceability; cumulative values in `rms_by_band.csv` ",
            "and `rms_convergence.csv` use the product force-PSD contract.\n\n",
            "Launcher: `{}`\n\nSolver override: `{}`\n\n",
            "Artifacts: `campaign_manifest.json`, `rms_by_band.csv`, `rms_convergence.csv`, `wing_mesh.bdf`, ",
            "`case-*hz/`, and `modal/`.\n"
        ),
        status,
        config.freq_step_hz,
        config.random_force_psd_n2_per_hz,
        config.n_modes,
        launcher.display(),
        solver.display(),
    );
    std::fs::write(output.join("README.md"), text).expect("write campaign README");
}

fn number(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_f64)
        .map_or(String::new(), |number| format!("{number:.8}"))
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
    let root = std::env::var_os("ALAS_MSC_F0004_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new("audit-artifacts/f0004-msc-rms").to_path_buf());
    root.join(format!("campaign-{}-{}", now_epoch(), std::process::id()))
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
