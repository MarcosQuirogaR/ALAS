// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Candidate replay against the unchanged finite-angle implementation.

use super::*;

fn compare(path: &str, current: &Value, reference: &Value) {
    match (current, reference) {
        (Value::Number(current), Value::Number(reference)) => {
            let current = current.as_f64().unwrap_or(f64::NAN);
            let reference = reference.as_f64().unwrap_or(f64::NAN);
            let scale_floor = if path.contains("cd") { 1.0e-6 } else { 1.0 };
            let tolerance = 1.0e-10 * reference.abs().max(scale_floor);
            assert!(
                (current - reference).abs() <= tolerance,
                "{path}: current {current:.17e}, reference {reference:.17e}, tolerance {tolerance:.3e}"
            );
        }
        (Value::Array(current), Value::Array(reference)) => {
            assert_eq!(current.len(), reference.len(), "{path}: array size");
            for (index, (current, reference)) in current.iter().zip(reference).enumerate() {
                compare(&format!("{path}[{index}]"), current, reference);
            }
        }
        (Value::Object(current), Value::Object(reference)) => {
            assert_eq!(current.len(), reference.len(), "{path}: field count");
            for (key, reference) in reference {
                let current = current.get(key).unwrap_or(&Value::Null);
                compare(&format!("{path}.{key}"), current, reference);
            }
        }
        _ => assert_eq!(current, reference, "{path}"),
    }
}

#[test]
fn four_presets_match_finite_angle_baseline_at_both_fidelities() -> Result<(), Box<dyn Error>> {
    rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build_global()?;
    let fixture: Value = serde_json::from_str(include_str!("baseline.json"))?;
    let rows = fixture["rows"].as_array().ok_or("baseline has no rows")?;
    for row in rows {
        let preset = row["preset"].as_str().ok_or("baseline preset is missing")?;
        let config = configuration(preset)?;
        let design = alas_config::presets::get(preset)?.design_vector;
        for (label, fidelity) in [
            ("full", ScreeningFidelity::full()),
            ("screening", ScreeningFidelity::shipped()),
        ] {
            let assessed = assess_product_candidate_with_controls(
                &fidelity.configure(&config),
                &design,
                fidelity.controls(),
            )?;
            compare(
                &format!("{preset}.{label}"),
                &numerical_evidence(&assessed),
                &row[label]["outputs"],
            );
        }
    }
    Ok(())
}
